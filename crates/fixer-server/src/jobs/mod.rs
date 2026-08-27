pub(crate) mod artifacts;
pub(crate) mod events;
pub mod model;
mod worker;

use std::{
    num::NonZeroUsize,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex as StdMutex, OnceLock},
};

use fixer_sdk::output::{ExecutionPolicy, OperationStatus, OutputPlanExt};
use futures_util::FutureExt;
use thiserror::Error;
use tokio::sync::{Mutex, Notify, oneshot, watch};

use crate::{
    FsPolicy, FsPolicyError,
    jobs::{
        events::{JobEventHub, JobEventStream, SubscribeError},
        model::{
            ExecutionSummary, JobInputDto, JobState, PlanSummary, ProgressSummary,
            ReviewDecisionDto, ReviewSummary,
        },
        worker::{RETRY_DELAYS, SharedWorkerFlow, WorkerFlow},
    },
    store::{ExecutionReservation, JobId, JobRecord, JobUpdate, SqliteJobStore, StoreError},
};

pub use worker::{JobFlowError, SdkJobFlow, SearchSummary, WorkerPool};

const EXECUTION_FINGERPRINT: &str = "approved-v1";

#[derive(Default)]
pub(crate) struct ExecutionTaskRegistry {
    state: StdMutex<ExecutionTaskState>,
    changed: Notify,
}

#[derive(Default)]
struct ExecutionTaskState {
    closing: bool,
    pending_registrations: usize,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

struct ExecutionRegistrationPermit {
    registry: Arc<ExecutionTaskRegistry>,
    active: bool,
}

impl ExecutionTaskRegistry {
    fn begin_registration(self: &Arc<Self>) -> Result<ExecutionRegistrationPermit, RuntimeError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closing {
            return Err(RuntimeError::ExecutionShuttingDown);
        }
        state.pending_registrations = state
            .pending_registrations
            .checked_add(1)
            .ok_or(RuntimeError::CountOverflow)?;
        Ok(ExecutionRegistrationPermit {
            registry: Arc::clone(self),
            active: true,
        })
    }

    pub(crate) async fn close_and_wait(&self) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (tasks, complete) = {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.closing = true;
                state.tasks.retain(|task| !task.is_finished());
                let tasks = std::mem::take(&mut state.tasks);
                let complete = state.pending_registrations == 0 && tasks.is_empty();
                (tasks, complete)
            };
            if complete {
                return;
            }
            if tasks.is_empty() {
                notified.await;
            } else {
                for task in tasks {
                    let _ = task.await;
                }
            }
        }
    }
}

impl ExecutionRegistrationPermit {
    fn register(mut self, task: tokio::task::JoinHandle<()>) {
        let mut state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.pending_registrations -= 1;
        state.tasks.retain(|task| !task.is_finished());
        state.tasks.push(task);
        self.active = false;
        drop(state);
        self.registry.changed.notify_waiters();
    }
}

impl Drop for ExecutionRegistrationPermit {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.pending_registrations -= 1;
        drop(state);
        self.registry.changed.notify_waiters();
    }
}

#[derive(Clone)]
pub struct JobRuntime {
    store: SqliteJobStore,
    events: JobEventHub,
    operations: Arc<Mutex<()>>,
    wake_workers: Arc<Notify>,
    request_flow: Arc<OnceLock<SharedWorkerFlow>>,
    execution_tasks: Arc<ExecutionTaskRegistry>,
    fs_policy: Option<Arc<FsPolicy>>,
}

impl JobRuntime {
    pub fn new(store: SqliteJobStore, event_capacity: NonZeroUsize) -> Self {
        Self {
            store,
            events: JobEventHub::new(event_capacity.get()),
            operations: Arc::new(Mutex::new(())),
            wake_workers: Arc::new(Notify::new()),
            request_flow: Arc::new(OnceLock::new()),
            execution_tasks: Arc::new(ExecutionTaskRegistry::default()),
            fs_policy: None,
        }
    }

    /// Restricts job inputs and output plans to canonical media roots.
    pub fn with_fs_policy(mut self, policy: FsPolicy) -> Self {
        self.fs_policy = Some(Arc::new(policy));
        self
    }

    pub fn start_workers(&self, worker_count: NonZeroUsize, flow: SdkJobFlow) -> WorkerPool {
        self.start_worker_flow(worker_count, WorkerFlow::Configured(flow))
    }

    pub(crate) fn start_local_workers(&self, worker_count: NonZeroUsize) -> WorkerPool {
        self.start_worker_flow(worker_count, WorkerFlow::Local)
    }

    fn start_worker_flow(&self, worker_count: NonZeroUsize, flow: WorkerFlow) -> WorkerPool {
        let flow = Arc::clone(self.request_flow.get_or_init(|| Arc::new(flow)));
        let (shutdown, receiver) = watch::channel(false);
        let handles = (0..worker_count.get())
            .map(|_| {
                let runtime = self.clone();
                let flow = Arc::clone(&flow);
                let receiver = receiver.clone();
                tokio::spawn(async move { runtime.worker_loop(flow, receiver).await })
            })
            .collect();
        self.wake_workers.notify_waiters();
        WorkerPool::new(shutdown, handles, Arc::clone(&self.execution_tasks))
    }

    pub(crate) async fn create(&self, input: JobInputDto) -> Result<JobRecord, RuntimeError> {
        let input = if let Some(policy) = &self.fs_policy {
            let canonical = policy.validate_read(input.input_path())?;
            JobInputDto::new(
                input.media_kind(),
                canonical.to_string_lossy().into_owned(),
                input.apply(),
            )
        } else {
            input
        };
        let _operation = self.operations.lock().await;
        let job = self.store.create_job(input).await?;
        self.events.publish_state(job.id(), job.state())?;
        self.wake_workers.notify_waiters();
        Ok(job)
    }

    pub(crate) async fn get(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        self.store.get_job(id).await.map_err(Into::into)
    }

    pub(crate) async fn list(
        &self,
        limit: usize,
        state: Option<JobState>,
    ) -> Result<Vec<JobRecord>, RuntimeError> {
        self.store.list_jobs(limit, state).await.map_err(Into::into)
    }

    pub(crate) async fn retry(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        let _operation = self.operations.lock().await;
        let job = self
            .store
            .transition(
                id,
                JobState::Interrupted,
                JobState::Queued,
                JobUpdate::default().with_progress(ProgressSummary::new("queued", 0, None)),
            )
            .await?;
        self.publish_transition(&job)?;
        self.wake_workers.notify_waiters();
        Ok(job)
    }

    pub(crate) async fn review_artifacts(
        &self,
        id: JobId,
        candidate_index: Option<u64>,
    ) -> Result<(artifacts::ReviewArtifacts, u64), RuntimeError> {
        let job = self.store.get_job(id).await?;
        if job.review().is_none() {
            return Err(RuntimeError::ArtifactConflict(job.state()));
        }
        let flow = self
            .request_flow
            .get()
            .ok_or(RuntimeError::WorkerFlowUnavailable)?;
        let search = flow.scan(job.input()).await?.search().await?;
        let (candidates, candidates_truncated) = search.candidate_artifacts()?;
        let selected_index = candidate_index.unwrap_or_else(|| {
            job.review_decision()
                .map_or(0, ReviewDecisionDto::candidate_index)
        });
        let resolved = search.resolve_selected(selected_index).await?;
        let mut details = resolved.review_diagnostics();
        details.candidates = candidates;
        details.candidates_truncated = candidates_truncated;
        Ok((details, selected_index))
    }

    pub(crate) async fn plan_artifacts(
        &self,
        id: JobId,
    ) -> Result<(artifacts::PlanArtifacts, bool), RuntimeError> {
        let job = self.store.get_job(id).await?;
        let Some(decision) = job.review_decision() else {
            return Err(RuntimeError::ArtifactConflict(job.state()));
        };
        let Some(summary) = job.plan() else {
            return Err(RuntimeError::ArtifactConflict(job.state()));
        };
        let flow = self
            .request_flow
            .get()
            .ok_or(RuntimeError::WorkerFlowUnavailable)?;
        let search = flow.scan(job.input()).await?.search().await?;
        let resolved = search.resolve_selected(decision.candidate_index()).await?;
        let conflict_count = resolved.conflict_count()?;
        let expected = (0..conflict_count).collect::<Vec<_>>();
        if decision.accepted_conflict_indexes() != expected {
            return Err(RuntimeError::ConflictAcknowledgementMismatch {
                expected: conflict_count,
            });
        }
        let plan = resolved.plan()?;
        let fingerprint = resolved.plan_fingerprint(decision, &plan)?;
        let operation_count =
            u64::try_from(plan.operations().len()).map_err(|_| RuntimeError::CountOverflow)?;
        if operation_count != summary.operation_count()
            || summary.fingerprint() != Some(fingerprint.as_str())
        {
            return Err(RuntimeError::StalePlan);
        }
        if let Some(policy) = &self.fs_policy {
            policy.validate_plan(&plan)?;
        }
        Ok((artifacts::plan(&plan)?, summary.requires_confirmation()))
    }

    pub(crate) async fn cancel(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        let _operation = self.operations.lock().await;
        let job = self.store.get_job(id).await?;
        if job.state() == JobState::Writing || !job.state().can_transition_to(JobState::Cancelled) {
            return Err(RuntimeError::CancellationConflict(job.state()));
        }
        let job = self
            .store
            .transition(
                id,
                job.state(),
                JobState::Cancelled,
                JobUpdate::default().with_progress(ProgressSummary::new("cancelled", 0, None)),
            )
            .await?;
        self.publish_transition(&job)?;
        Ok(job)
    }

    pub(crate) async fn review(
        &self,
        id: JobId,
        decision: ReviewDecisionDto,
    ) -> Result<JobRecord, RuntimeError> {
        let job = self.store.get_job(id).await?;
        if job.state() != JobState::AwaitingConfirmation {
            return Err(RuntimeError::ReviewConflict(job.state()));
        }
        let (plan, fingerprint) = self.reconstruct_plan(job.input(), &decision).await?;
        if let Some(policy) = &self.fs_policy {
            policy.validate_plan(&plan)?;
        }
        let operation_count =
            u64::try_from(plan.operations().len()).map_err(|_| RuntimeError::CountOverflow)?;
        let _operation = self.operations.lock().await;
        let job = self
            .store
            .transition(
                id,
                JobState::AwaitingConfirmation,
                JobState::Planning,
                JobUpdate::default()
                    .with_progress(ProgressSummary::new("planning", 1, Some(1)))
                    .with_review_decision(decision)
                    .with_plan(
                        PlanSummary::new(operation_count, true).with_fingerprint(fingerprint),
                    ),
            )
            .await?;
        self.publish_transition(&job)?;
        Ok(job)
    }

    pub(crate) async fn execute(
        &self,
        id: JobId,
        idempotency_key: &str,
    ) -> Result<JobRecord, RuntimeError> {
        let job = self.store.get_job(id).await?;
        let Some(decision) = job.review_decision().cloned() else {
            return Err(RuntimeError::ExecutionConflict(job.state()));
        };
        if !matches!(job.state(), JobState::Planning) {
            let _operation = self.operations.lock().await;
            return match self
                .store
                .reserve_execution(id, idempotency_key, EXECUTION_FINGERPRINT)
                .await?
            {
                ExecutionReservation::Existing(job) => Ok(job),
                ExecutionReservation::Reserved(_) => {
                    unreachable!("non-planning jobs cannot reserve")
                }
            };
        }
        if !job.input().apply() {
            return Err(RuntimeError::ApprovalNotEnabled);
        }
        let (plan, fingerprint) = self.reconstruct_plan(job.input(), &decision).await?;
        let reviewed_plan = job
            .plan()
            .ok_or(RuntimeError::ExecutionConflict(job.state()))?;
        let expected_operations = reviewed_plan.operation_count();
        let actual_operations =
            u64::try_from(plan.operations().len()).map_err(|_| RuntimeError::CountOverflow)?;
        if actual_operations != expected_operations
            || reviewed_plan.fingerprint() != Some(fingerprint.as_str())
        {
            return Err(RuntimeError::StalePlan);
        }
        if let Some(policy) = &self.fs_policy {
            policy.validate_plan(&plan)?;
        }

        let registration = self.execution_tasks.begin_registration()?;
        let reservation = {
            let _operation = self.operations.lock().await;
            self.store
                .reserve_execution(id, idempotency_key, EXECUTION_FINGERPRINT)
                .await?
        };
        let reserved = match reservation {
            ExecutionReservation::Existing(job) => return Ok(job),
            ExecutionReservation::Reserved(job) => job,
        };

        let (sender, receiver) = oneshot::channel();
        let runtime = self.clone();
        let task = tokio::spawn(async move {
            tokio::task::yield_now().await;
            let result =
                AssertUnwindSafe(runtime.run_reserved_execution(id, plan, actual_operations))
                    .catch_unwind()
                    .await;
            let result = match result {
                Ok(result) => result,
                Err(_) => runtime.finish_reserved_panic(id, actual_operations).await,
            };
            let _ = sender.send(result);
        });
        registration.register(task);
        let _ = self.publish_transition(&reserved);
        receiver
            .await
            .map_err(|_| RuntimeError::ExecutionTaskClosed)?
    }

    async fn run_reserved_execution(
        &self,
        id: JobId,
        plan: fixer_core::OutputPlan,
        actual_operations: u64,
    ) -> Result<JobRecord, RuntimeError> {
        let execution =
            tokio::task::spawn_blocking(move || plan.execute(ExecutionPolicy::default())).await;
        let (next, summary) = match execution {
            Ok(Ok(report)) => (
                JobState::Completed,
                ExecutionSummary::new(
                    u64::try_from(report.operations().len())
                        .map_err(|_| RuntimeError::CountOverflow)?,
                    0,
                ),
            ),
            Ok(Err(failure)) => {
                let completed = failure
                    .report()
                    .operations()
                    .iter()
                    .filter(|operation| operation.status != OperationStatus::Failed)
                    .count();
                let failed = failure
                    .report()
                    .operations()
                    .iter()
                    .filter(|operation| operation.status == OperationStatus::Failed)
                    .count();
                (
                    JobState::Failed,
                    ExecutionSummary::new(
                        u64::try_from(completed).map_err(|_| RuntimeError::CountOverflow)?,
                        u64::try_from(failed).map_err(|_| RuntimeError::CountOverflow)?,
                    ),
                )
            }
            Err(_) => (JobState::Failed, ExecutionSummary::new(0, 0)),
        };
        let stage = if next == JobState::Completed {
            "completed"
        } else {
            "failed"
        };
        let completed_operations = summary.completed_operations();
        let job = self
            .transition_with_retry(
                id,
                JobState::Writing,
                next,
                JobUpdate::default()
                    .with_progress(ProgressSummary::new(
                        stage,
                        completed_operations,
                        Some(actual_operations),
                    ))
                    .with_execution(summary),
            )
            .await?;
        self.events.publish_completion(id, &summary)?;
        Ok(job)
    }

    async fn finish_reserved_panic(
        &self,
        id: JobId,
        total_operations: u64,
    ) -> Result<JobRecord, RuntimeError> {
        let summary = ExecutionSummary::new(0, 0);
        let job = self
            .transition_with_retry(
                id,
                JobState::Writing,
                JobState::Failed,
                JobUpdate::default()
                    .with_progress(ProgressSummary::new("failed", 0, Some(total_operations)))
                    .with_execution(summary),
            )
            .await?;
        self.events.publish_completion(id, &summary)?;
        Ok(job)
    }

    async fn reconstruct_plan(
        &self,
        input: &JobInputDto,
        decision: &ReviewDecisionDto,
    ) -> Result<(fixer_core::OutputPlan, String), RuntimeError> {
        let flow = self
            .request_flow
            .get()
            .ok_or(RuntimeError::WorkerFlowUnavailable)?;
        let scanned = flow.scan(input).await?;
        let search = scanned.search().await?;
        let resolved = search.resolve_selected(decision.candidate_index()).await?;
        let conflict_count = resolved.conflict_count()?;
        let expected = (0..conflict_count).collect::<Vec<_>>();
        if decision.accepted_conflict_indexes() != expected {
            return Err(RuntimeError::ConflictAcknowledgementMismatch {
                expected: conflict_count,
            });
        }
        let plan = resolved.plan()?;
        let fingerprint = resolved.plan_fingerprint(decision, &plan)?;
        Ok((plan, fingerprint))
    }

    pub(crate) async fn event_stream(
        &self,
        id: JobId,
        cursor: Option<&str>,
    ) -> Result<JobEventStream, RuntimeError> {
        let _operation = self.operations.lock().await;
        let job = self.store.get_job(id).await?;
        self.events.ensure_state(id, job.state())?;
        self.events
            .subscribe(id, cursor)
            .map_err(RuntimeError::from)
    }

    async fn worker_loop(&self, flow: SharedWorkerFlow, mut shutdown: watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow() {
                return;
            }
            let notified = self.wake_workers.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            match self.claim_next_with_retry().await {
                Ok(Some(job)) => {
                    let id = job.id();
                    let process =
                        AssertUnwindSafe(self.process_claimed(job, flow.as_ref(), &shutdown))
                            .catch_unwind()
                            .await;
                    if process.is_err() {
                        // The per-job panic is contained so this fixed worker remains alive.
                    }
                    self.interrupt_active(id).await;
                }
                Ok(None) => {
                    tokio::select! {
                        _ = &mut notified => {},
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() { return; }
                        }
                    }
                }
                Err(_) => {
                    tokio::select! {
                        _ = tokio::time::sleep(RETRY_DELAYS[RETRY_DELAYS.len() - 1]) => {},
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() { return; }
                        }
                    }
                }
            }
        }
    }

    async fn process_claimed(
        &self,
        job: JobRecord,
        flow: &WorkerFlow,
        shutdown: &watch::Receiver<bool>,
    ) {
        let id = job.id();
        if self.stop_requested(shutdown, id).await {
            return;
        }
        let scanned = match flow.scan(job.input()).await {
            Ok(scanned) => scanned,
            Err(_) => {
                self.finish_active(id, JobState::Scanning, JobState::Failed, "failed")
                    .await;
                return;
            }
        };
        if self.stop_requested(shutdown, id).await {
            return;
        }
        if self
            .transition_stage_with_retry(id, JobState::Scanning, JobState::Searching, "searching")
            .await
            .is_err()
        {
            return;
        }

        let search = match scanned.search().await {
            Ok(search) => search,
            Err(_) => {
                self.finish_active(id, JobState::Searching, JobState::Failed, "failed")
                    .await;
                return;
            }
        };
        if self.stop_requested(shutdown, id).await {
            return;
        }
        if self
            .transition_stage_with_retry(id, JobState::Searching, JobState::Resolving, "resolving")
            .await
            .is_err()
        {
            return;
        }

        let summary = match search.resolve().await {
            Ok(summary) => summary,
            Err(_) => {
                self.finish_active(id, JobState::Resolving, JobState::Failed, "failed")
                    .await;
                return;
            }
        };
        if self.stop_requested(shutdown, id).await {
            return;
        }
        let update = JobUpdate::default()
            .with_progress(ProgressSummary::new("awaiting_confirmation", 1, Some(1)))
            .with_review(ReviewSummary::new(
                summary.candidate_count(),
                summary.conflict_count(),
            ));
        if self
            .transition_with_retry(
                id,
                JobState::Resolving,
                JobState::AwaitingConfirmation,
                update,
            )
            .await
            .is_ok()
        {
            let _ =
                self.events
                    .publish_review(id, summary.candidate_count(), summary.conflict_count());
        }
    }

    async fn stop_requested(&self, shutdown: &watch::Receiver<bool>, id: JobId) -> bool {
        if *shutdown.borrow() {
            self.interrupt_active(id).await;
            true
        } else {
            false
        }
    }

    async fn claim_next_with_retry(&self) -> Result<Option<JobRecord>, StoreError> {
        let mut last_error = None;
        for delay in std::iter::once(None).chain(RETRY_DELAYS.map(Some)) {
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            let _operation = self.operations.lock().await;
            match self
                .store
                .claim_next_queued(ProgressSummary::new("scanning", 0, None))
                .await
            {
                Ok(job) => {
                    if let Some(job) = &job {
                        let _ = self.publish_transition(job);
                    }
                    return Ok(job);
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.expect("at least one claim attempt is made"))
    }

    async fn transition_stage_with_retry(
        &self,
        id: JobId,
        expected: JobState,
        next: JobState,
        stage: &'static str,
    ) -> Result<JobRecord, StoreError> {
        self.transition_with_retry(
            id,
            expected,
            next,
            JobUpdate::default().with_progress(ProgressSummary::new(stage, 0, None)),
        )
        .await
    }

    async fn transition_with_retry(
        &self,
        id: JobId,
        expected: JobState,
        next: JobState,
        update: JobUpdate,
    ) -> Result<JobRecord, StoreError> {
        let mut last_error = None;
        for delay in std::iter::once(None).chain(RETRY_DELAYS.map(Some)) {
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            let _operation = self.operations.lock().await;
            match self
                .store
                .transition(id, expected, next, update.clone())
                .await
            {
                Ok(job) => {
                    let _ = self.publish_transition(&job);
                    return Ok(job);
                }
                Err(error @ (StoreError::StateConflict { .. } | StoreError::NotFound { .. })) => {
                    return Err(error);
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.expect("at least one transition attempt is made"))
    }

    async fn finish_active(
        &self,
        id: JobId,
        expected: JobState,
        next: JobState,
        stage: &'static str,
    ) {
        if self
            .transition_stage_with_retry(id, expected, next, stage)
            .await
            .is_err()
        {
            self.interrupt_active(id).await;
        }
    }

    async fn interrupt_active(&self, id: JobId) {
        loop {
            let job = match self.store.get_job(id).await {
                Ok(job) => job,
                Err(StoreError::NotFound { .. }) => return,
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    continue;
                }
            };
            if !matches!(
                job.state(),
                JobState::Scanning | JobState::Searching | JobState::Resolving
            ) {
                return;
            }
            match self
                .transition_stage_with_retry(id, job.state(), JobState::Interrupted, "interrupted")
                .await
            {
                Ok(_) | Err(StoreError::NotFound { .. }) => return,
                Err(StoreError::StateConflict { .. }) => continue,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
            }
        }
    }

    fn publish_transition(&self, job: &JobRecord) -> Result<(), RuntimeError> {
        self.events.publish_state(job.id(), job.state())?;
        if let Some(progress) = job.progress() {
            self.events.publish_progress(job.id(), progress)?;
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum RuntimeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    FilesystemPolicy(#[from] FsPolicyError),
    #[error("job in state {0} cannot be cancelled at this stage")]
    CancellationConflict(JobState),
    #[error("job in state {0} cannot be reviewed")]
    ReviewConflict(JobState),
    #[error("job in state {0} has no reconstructable review or plan artifacts")]
    ArtifactConflict(JobState),
    #[error("job in state {0} cannot be executed")]
    ExecutionConflict(JobState),
    #[error("all resolved conflicts must be acknowledged; expected {expected} indexes")]
    ConflictAcknowledgementMismatch { expected: u64 },
    #[error("job was not created with apply enabled")]
    ApprovalNotEnabled,
    #[error("the active worker flow is unavailable")]
    WorkerFlowUnavailable,
    #[error("reconstructed output plan no longer matches the reviewed plan")]
    StalePlan,
    #[error("supervised execution task closed before returning its durable result")]
    ExecutionTaskClosed,
    #[error("job execution is shutting down")]
    ExecutionShuttingDown,
    #[error("job count exceeds the persistent summary range")]
    CountOverflow,
    #[error(transparent)]
    Flow(#[from] JobFlowError),
    #[error("requested job events are no longer retained")]
    EventHistoryExpired,
    #[error("job event cursor is invalid")]
    InvalidEventCursor,
    #[error("job event sequence is exhausted")]
    EventSequenceExhausted,
}

impl From<SubscribeError> for RuntimeError {
    fn from(error: SubscribeError) -> Self {
        match error {
            SubscribeError::Expired => Self::EventHistoryExpired,
            SubscribeError::Invalid => Self::InvalidEventCursor,
            SubscribeError::SequenceExhausted => Self::EventSequenceExhausted,
        }
    }
}
