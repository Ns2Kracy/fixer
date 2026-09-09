pub(crate) mod artifacts;
pub(crate) mod events;
pub mod model;
mod worker;

use std::{
    num::NonZeroUsize,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex as StdMutex, OnceLock},
};

use fixer_core::{OperationOutcome, OutputOperation, OutputPlan, ReplacementManifest};
use fixer_sdk::output::{ExecutionError, ExecutionFailure, ExecutionPolicy, OutputPlanExt};
use futures_util::FutureExt;
use thiserror::Error;
use tokio::sync::{Mutex, Notify, mpsc, oneshot, watch};

use crate::{
    FsPolicy, FsPolicyError,
    ingestion::model::IngestionSourceId,
    jobs::{
        events::{JobEventHub, JobEventStream, SubscribeError},
        model::{
            AutoReviewReason, ExecutionFailureSummary, ExecutionSummary, JobInputDto, JobState,
            PlanSummary, ProgressSummary, ReviewDecisionDto, ReviewSummary,
        },
        worker::{RETRY_DELAYS, SharedWorkerFlow, WorkerFlow},
    },
    store::{ExecutionReservation, JobId, JobRecord, JobUpdate, SqliteJobStore, StoreError},
};

pub use worker::{JobFlowError, SdkJobFlow, SearchSummary, WorkerPool};

const EXECUTION_FINGERPRINT: &str = "approved-v1";
const AUTO_EXECUTION_KEY: &str = "ingestion-auto-v1";

struct PreparedReview {
    candidate_count: u64,
    conflict_count: u64,
    selected_target: Option<fixer_core::ProviderTarget>,
    automatic: worker::AutoDecision,
}

const fn is_terminal(state: JobState) -> bool {
    matches!(
        state,
        JobState::Completed | JobState::Failed | JobState::Cancelled | JobState::Interrupted
    )
}

fn execution_failure_summary(failure: &ExecutionFailure) -> ExecutionFailureSummary {
    let operation_index = failure
        .report()
        .operations()
        .iter()
        .find(|operation| operation.outcome() == OperationOutcome::Failed)
        .map(fixer_core::OperationReport::operation_index);
    let (code, message) = match failure.error() {
        ExecutionError::UnsafeTarget { .. } => (
            "unsafe_target",
            "An output target is outside the permitted destination",
        ),
        ExecutionError::TargetExists { .. } => (
            "target_exists",
            "An output target already exists; choose another destination or resolve the collision",
        ),
        ExecutionError::StalePlan { .. } => (
            "stale_plan",
            "Files changed after planning; rebuild the output plan before retrying",
        ),
        ExecutionError::ReplacementNotAllowed { .. } => (
            "replacement_not_allowed",
            "A correction target changed after the prior scrape and was left untouched",
        ),
        ExecutionError::SourceUnavailable { .. } => (
            "source_unavailable",
            "A source file is no longer available; restore it and retry",
        ),
        ExecutionError::RelativeSymlinkUnavailable { .. } => (
            "relative_symlink_unavailable",
            "A relative link cannot be created for this source and destination",
        ),
        ExecutionError::ReflinkUnsupported { .. } => (
            "reflink_unsupported",
            "This filesystem does not support copy-on-write clones for the selected files",
        ),
        ExecutionError::InvalidPlan(_) => (
            "invalid_plan",
            "The output plan is invalid; rebuild it before retrying",
        ),
        ExecutionError::Io { .. } => (
            "filesystem_error",
            "A filesystem operation failed; check permissions and available space",
        ),
        _ => (
            "execution_failed",
            "The output operation failed; rebuild the plan and retry",
        ),
    };
    ExecutionFailureSummary::new(operation_index, code, message)
}

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
        drop(state);
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
                drop(state);
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
    dispatch: Arc<StdMutex<Option<mpsc::Sender<JobId>>>>,
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
            dispatch: Arc::new(StdMutex::new(None)),
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

    pub fn start_workers(&self, queue_capacity: NonZeroUsize, flow: SdkJobFlow) -> WorkerPool {
        self.start_worker_flow(queue_capacity, WorkerFlow::Configured(flow))
    }

    pub(crate) fn start_local_workers(&self, queue_capacity: NonZeroUsize) -> WorkerPool {
        self.start_worker_flow(queue_capacity, WorkerFlow::Local)
    }

    fn start_worker_flow(&self, queue_capacity: NonZeroUsize, flow: WorkerFlow) -> WorkerPool {
        let flow = Arc::clone(self.request_flow.get_or_init(|| Arc::new(flow)));
        let (sender, queue) = mpsc::channel(queue_capacity.get());
        *self
            .dispatch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender.clone());
        let (shutdown, receiver) = watch::channel(false);

        let recovery_runtime = self.clone();
        let recovery_receiver = receiver.clone();
        let recovery = tokio::spawn(async move {
            recovery_runtime
                .recover_queued(sender, recovery_receiver)
                .await;
        });
        let runtime = self.clone();
        let worker = tokio::spawn(async move { runtime.worker_loop(flow, queue, receiver).await });
        WorkerPool::new(
            shutdown,
            recovery,
            vec![worker],
            Arc::clone(&self.execution_tasks),
        )
    }

    pub(crate) async fn create(&self, input: JobInputDto) -> Result<JobRecord, RuntimeError> {
        let input = self.validate_input(input)?;
        let dispatch = self.reserve_dispatch().await?;
        let operation = self.operations.lock().await;
        let job = self.store.create_job(input).await?;
        let published = self.publish_created(&job);
        drop(operation);
        let dispatched = self.dispatch(dispatch, job.id()).await;
        published?;
        dispatched?;
        Ok(job)
    }

    pub(crate) async fn create_run(
        &self,
        input: JobInputDto,
    ) -> Result<(JobRecord, bool), RuntimeError> {
        let input = self.validate_input(input)?;
        let dispatch = self.reserve_dispatch().await?;
        let operation = self.operations.lock().await;
        if let Some(existing) = self
            .store
            .list_jobs(10_000, None)
            .await?
            .into_iter()
            .find(|job| !is_terminal(job.state()) && job.input() == &input)
        {
            return Ok((existing, false));
        }
        let job = self.store.create_job(input).await?;
        let published = self.publish_created(&job);
        drop(operation);
        let dispatched = self.dispatch(dispatch, job.id()).await;
        published?;
        dispatched?;
        Ok((job, true))
    }

    pub(crate) async fn create_for_source(
        &self,
        source_id: IngestionSourceId,
        input: JobInputDto,
    ) -> Result<JobRecord, RuntimeError> {
        let input = self.validate_input(input)?;
        let dispatch = self.reserve_dispatch().await?;
        let operation = self.operations.lock().await;
        let job = match self.store.create_job_for_source(source_id, input).await {
            Ok(job) => job,
            Err(error @ StoreError::IngestionSourceJobConflict { .. }) => {
                let Some(job) = self.store.get_source_job(source_id).await? else {
                    return Err(error.into());
                };
                return Ok(job);
            }
            Err(error) => return Err(error.into()),
        };
        let published = self.publish_created(&job);
        drop(operation);
        let dispatched = self.dispatch(dispatch, job.id()).await;
        published?;
        dispatched?;
        Ok(job)
    }

    fn validate_input(&self, input: JobInputDto) -> Result<JobInputDto, RuntimeError> {
        let Some(policy) = &self.fs_policy else {
            return Ok(input);
        };
        let canonical = policy.validate_read(input.input_path())?;
        let mut validated = JobInputDto::new(
            input.media_kind(),
            canonical.to_string_lossy().into_owned(),
            input.apply(),
        )
        .with_selection(input.selection().clone());
        if input.unattended() {
            validated = validated.with_unattended();
        }
        if let Some(run_id) = input.correction_of() {
            validated = validated.with_correction_of(run_id);
        }
        if let Some(organization) = input.organization() {
            let mut organization = organization.clone();
            let destination = policy
                .validate_read(&organization.destination_path)
                .map_err(RuntimeError::OrganizationFilesystemPolicy)?;
            if !destination.is_dir() {
                return Err(RuntimeError::OrganizationFilesystemPolicy(
                    FsPolicyError::PathNotDirectory {
                        path: organization.destination_path.into(),
                    },
                ));
            }
            organization.destination_path = destination.to_string_lossy().into_owned();
            validated = validated.with_organization(organization);
        }
        Ok(validated)
    }

    fn publish_created(&self, job: &JobRecord) -> Result<(), RuntimeError> {
        self.events.publish_state(job.id(), job.state())?;
        Ok(())
    }

    pub(crate) async fn get(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        self.store.get_job(id).await.map_err(Into::into)
    }
    async fn reserve_dispatch(&self) -> Result<Option<mpsc::OwnedPermit<JobId>>, RuntimeError> {
        let sender = self
            .dispatch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(sender) = sender else {
            return Ok(None);
        };
        sender
            .reserve_owned()
            .await
            .map(Some)
            .map_err(|_| RuntimeError::QueueUnavailable)
    }

    async fn dispatch(
        &self,
        permit: Option<mpsc::OwnedPermit<JobId>>,
        id: JobId,
    ) -> Result<(), RuntimeError> {
        if let Some(permit) = permit {
            let _sender = permit.send(id);
            return Ok(());
        }
        let sender = self
            .dispatch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(sender) = sender else {
            return Ok(());
        };
        sender
            .send(id)
            .await
            .map_err(|_| RuntimeError::QueueUnavailable)
    }


    pub(crate) async fn list(
        &self,
        limit: usize,
        state: Option<JobState>,
    ) -> Result<Vec<JobRecord>, RuntimeError> {
        self.store.list_jobs(limit, state).await.map_err(Into::into)
    }

    pub(crate) async fn retry(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        let dispatch = self.reserve_dispatch().await?;
        let operation = self.operations.lock().await;
        let job = self
            .store
            .transition(
                id,
                JobState::Interrupted,
                JobState::Queued,
                JobUpdate::default().with_progress(ProgressSummary::new("queued", 0, None)),
            )
            .await?;
        let published = self.publish_transition(&job);
        self.release_job_config(id);
        drop(operation);
        let dispatched = self.dispatch(dispatch, id).await;
        published?;
        dispatched?;
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
        let scanned = if is_terminal(job.state()) {
            flow.scan_current(job.input()).await
        } else {
            flow.scan(id, job.input()).await
        };
        let search = scanned?.search().await?;
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
        let scanned = if is_terminal(job.state()) {
            flow.scan_current(job.input()).await
        } else {
            flow.scan(id, job.input()).await
        };
        let search = scanned?.search().await?;
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
        self.release_job_config(id);
        Ok(job)
    }

    pub(crate) async fn review(
        &self,
        id: JobId,
        decision: ReviewDecisionDto,
    ) -> Result<JobRecord, RuntimeError> {
        self.review_with_auto_guard(id, decision, false).await
    }

    async fn review_with_auto_guard(
        &self,
        id: JobId,
        decision: ReviewDecisionDto,
        automatic: bool,
    ) -> Result<JobRecord, RuntimeError> {
        let job = self.store.get_job(id).await?;
        if job.state() != JobState::AwaitingConfirmation {
            return Err(RuntimeError::ReviewConflict(job.state()));
        }
        let (plan, fingerprint) = self
            .reconstruct_plan(id, job.input(), &decision, automatic)
            .await?;
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
        let (plan, fingerprint) = self
            .reconstruct_plan(
                id,
                job.input(),
                &decision,
                idempotency_key == AUTO_EXECUTION_KEY,
            )
            .await?;
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
        let replacement = self.replacement_manifest(id).await?;
        let execution = tokio::task::spawn_blocking(move || match replacement {
            Some(manifest) => plan.execute_replacing(&manifest),
            None => plan.execute(ExecutionPolicy::default()),
        })
        .await;
        let (next, summary) = match execution {
            Ok(Ok(report)) => (
                JobState::Completed,
                ExecutionSummary::new(
                    u64::try_from(report.operations().len())
                        .map_err(|_| RuntimeError::CountOverflow)?,
                    0,
                )
                .with_operations(report.operations().to_vec()),
            ),
            Ok(Err(failure)) => {
                let completed = failure
                    .report()
                    .operations()
                    .iter()
                    .filter(|operation| operation.outcome() != OperationOutcome::Failed)
                    .count();
                let failed = failure
                    .report()
                    .operations()
                    .iter()
                    .filter(|operation| operation.outcome() == OperationOutcome::Failed)
                    .count();
                (
                    JobState::Failed,
                    ExecutionSummary::new(
                        u64::try_from(completed).map_err(|_| RuntimeError::CountOverflow)?,
                        u64::try_from(failed).map_err(|_| RuntimeError::CountOverflow)?,
                    )
                    .with_failure(execution_failure_summary(&failure))
                    .with_operations(failure.report().operations().to_vec()),
                )
            }
            Err(_) => (
                JobState::Failed,
                ExecutionSummary::new(0, 0).with_failure(ExecutionFailureSummary::new(
                    None,
                    "execution_task_failed",
                    "The output task stopped before it could report an operation",
                )),
            ),
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
                    .with_execution(summary.clone()),
            )
            .await?;
        self.events.publish_completion(id, &summary)?;
        Ok(job)
    }

    async fn replacement_manifest(
        &self,
        id: JobId,
    ) -> Result<Option<ReplacementManifest>, RuntimeError> {
        let run = self.store.get_job(id).await?;
        let Some(parent_id) = run.input().correction_of() else {
            return Ok(None);
        };
        let parent = self.store.get_job(JobId::from_database(parent_id)?).await?;
        let execution = parent
            .execution()
            .ok_or(RuntimeError::ArtifactConflict(parent.state()))?;
        ReplacementManifest::from_reports(execution.operations())
            .map(Some)
            .map_err(|error| RuntimeError::Flow(JobFlowError::InvalidInput(error.to_string())))
    }

    async fn finish_reserved_panic(
        &self,
        id: JobId,
        total_operations: u64,
    ) -> Result<JobRecord, RuntimeError> {
        let summary = ExecutionSummary::new(0, 0).with_failure(ExecutionFailureSummary::new(
            None,
            "execution_panicked",
            "The output task stopped unexpectedly",
        ));
        let job = self
            .transition_with_retry(
                id,
                JobState::Writing,
                JobState::Failed,
                JobUpdate::default()
                    .with_progress(ProgressSummary::new("failed", 0, Some(total_operations)))
                    .with_execution(summary.clone()),
            )
            .await?;
        self.events.publish_completion(id, &summary)?;
        Ok(job)
    }

    async fn reconstruct_plan(
        &self,
        id: JobId,
        input: &JobInputDto,
        decision: &ReviewDecisionDto,
        automatic: bool,
    ) -> Result<(fixer_core::OutputPlan, String), RuntimeError> {
        let flow = self
            .request_flow
            .get()
            .ok_or(RuntimeError::WorkerFlowUnavailable)?;
        let scanned = flow.scan(id, input).await?;
        let search = scanned.search().await?;
        let (candidates, candidates_truncated) = search.candidate_artifacts()?;
        let resolved = search.resolve_selected(decision.candidate_index()).await?;
        let conflict_count = resolved.conflict_count()?;
        if automatic {
            let mut diagnostics = resolved.review_diagnostics();
            diagnostics.candidates = candidates;
            diagnostics.candidates_truncated = candidates_truncated;
            let expected = worker::AutoDecision::Execute {
                candidate_index: decision.candidate_index(),
            };
            if worker::auto_decision(input, &diagnostics, conflict_count) != expected {
                return Err(RuntimeError::StalePlan);
            }
        }
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

    async fn recover_queued(
        &self,
        sender: mpsc::Sender<JobId>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let queued = loop {
            match self.store.queued_job_ids().await {
                Ok(queued) => break queued,
                Err(_) => {
                    tokio::select! {
                        () = tokio::time::sleep(RETRY_DELAYS[RETRY_DELAYS.len() - 1]) => {},
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() { return; }
                        }
                    }
                }
            }
        };
        for id in queued {
            tokio::select! {
                result = sender.send(id) => {
                    if result.is_err() { return; }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { return; }
                }
            }
        }
    }

    async fn worker_loop(
        &self,
        flow: SharedWorkerFlow,
        mut queue: mpsc::Receiver<JobId>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        loop {
            let _dispatched_id = tokio::select! {
                biased;
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() { return; }
                    continue;
                }
                id = queue.recv() => {
                    let Some(id) = id else { return; };
                    id
                }
            };
            if *shutdown.borrow() {
                return;
            }
            let job = loop {
                match self.claim_with_retry().await {
                    Ok(job) => break job,
                    Err(_) => {
                        tokio::select! {
                            () = tokio::time::sleep(RETRY_DELAYS[RETRY_DELAYS.len() - 1]) => {},
                            changed = shutdown.changed() => {
                                if changed.is_err() || *shutdown.borrow() { return; }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn process_claimed(
        &self,
        job: JobRecord,
            };
            let Some(job) = job else {
                continue;
            };
            let id = job.id();
            if *shutdown.borrow() {
                let update =
                    JobUpdate::default().with_progress(ProgressSummary::new("queued", 0, None));
                if self
                    .transition_with_retry(id, JobState::Scanning, JobState::Queued, update)
                    .await
                    .is_err()
                {
                    self.interrupt_active(id).await;
                }
                return;
            }
            let process = AssertUnwindSafe(self.process_claimed(job, flow.as_ref(), &shutdown))
                .catch_unwind()
                .await;
            if process.is_err() {
                // The per-job panic is contained so this fixed worker remains alive.
        flow: &WorkerFlow,
            self.interrupt_active(id).await;
        shutdown: &watch::Receiver<bool>,
    ) {
        let id = job.id();
        if self.stop_requested(shutdown, id).await {
            return;
        }
        let Ok(scanned) = flow.scan(id, job.input()).await else {
            self.finish_active(id, JobState::Scanning, JobState::Failed, "failed")
                .await;
            return;
        };
        if self.stop_requested(shutdown, id).await {
            return;
        }
        if self
            .transition_stage_with_retry(id, JobState::Scanning, JobState::Searching, "searching")
            .await
            .is_err()
        {
            flow.release(id);
            return;
        }

        let Ok(search) = scanned.search().await else {
            self.finish_active(id, JobState::Searching, JobState::Failed, "failed")
                .await;
            return;
        };
        if self.stop_requested(shutdown, id).await {
            return;
        }
        if self
            .transition_stage_with_retry(id, JobState::Searching, JobState::Resolving, "resolving")
            .await
            .is_err()
        {
            flow.release(id);
            return;
        }

        let Ok(prepared) = self.prepare_review(job.input(), search).await else {
            self.finish_active(id, JobState::Resolving, JobState::Failed, "failed")
                .await;
            return;
        };
        let PreparedReview {
            candidate_count,
            conflict_count,
            selected_target,
            automatic,
        } = prepared;

        if self.stop_requested(shutdown, id).await {
            return;
        }
        let mut summary = ReviewSummary::new(candidate_count, conflict_count);
        if let Some(target) = selected_target {
            summary = summary.with_selected_target(target);
        }
        if let worker::AutoDecision::NeedsReview { reason } = automatic {
            summary = summary.with_automation_reason(reason);
        }
        let update = JobUpdate::default()
            .with_progress(ProgressSummary::new("awaiting_confirmation", 1, Some(1)))
            .with_review(summary);
        if self
            .transition_with_retry(
                id,
                JobState::Resolving,
                JobState::AwaitingConfirmation,
                update,
            )
            .await
            .is_err()
        {
            flow.release(id);
            return;
        }
        let _ = self
            .events
            .publish_review(id, candidate_count, conflict_count);

        if let worker::AutoDecision::Execute { candidate_index } = automatic {
            let decision = ReviewDecisionDto::new(candidate_index, Vec::new());
            if self
                .review_with_auto_guard(id, decision, true)
                .await
                .is_ok()
            {
                let _ = self.execute(id, AUTO_EXECUTION_KEY).await;
            }
        }
    }

    async fn prepare_review(
        &self,
        input: &JobInputDto,
        search: worker::SearchArtifact,
    ) -> Result<PreparedReview, JobFlowError> {
        let candidate_count = search.candidate_count();
        let (candidates, candidates_truncated) = search.candidate_artifacts()?;
        let mut review = artifacts::ReviewArtifacts {
            candidates,
            candidates_truncated,
            warnings: Vec::new(),
            warnings_truncated: false,
            conflicts: Vec::new(),
            conflicts_truncated: false,
        };
        let selected_target = review
            .candidates
            .first()
            .map(artifacts::CandidateArtifact::target)
            .transpose()?;
        let resolved = search.resolve_selected(0).await?;
        let conflict_count = resolved.conflict_count()?;
        let diagnostics = resolved.review_diagnostics();
        review.warnings = diagnostics.warnings;
        review.warnings_truncated = diagnostics.warnings_truncated;
        review.conflicts = diagnostics.conflicts;
        review.conflicts_truncated = diagnostics.conflicts_truncated;
        let mut automatic = worker::auto_decision(input, &review, conflict_count);
        if matches!(automatic, worker::AutoDecision::Execute { .. }) {
            automatic = resolved.plan().map_or_else(
                |_| worker::AutoDecision::NeedsReview {
                    reason: AutoReviewReason::InvalidPlan,
                },
                |plan| self.auto_plan_decision(input, &plan, automatic),
            );
        }
        Ok(PreparedReview {
            candidate_count,
            conflict_count,
            selected_target,
            automatic,
        })
    }

    fn auto_plan_decision(
        &self,
        input: &JobInputDto,
        plan: &OutputPlan,
        approved: worker::AutoDecision,
    ) -> worker::AutoDecision {
        let Some(policy) = &self.fs_policy else {
            return worker::AutoDecision::NeedsReview {
                reason: AutoReviewReason::InvalidPlan,
            };
        };
        if policy.validate_plan(plan).is_err() {
            return worker::AutoDecision::NeedsReview {
                reason: AutoReviewReason::InvalidPlan,
            };
        }
        if input.correction_of().is_none()
            && plan.operations().iter().any(|operation| {
                let target = operation.target().map_or_else(
                    || plan.output_root.clone(),
                    |target| {
                        if target.is_absolute() {
                            target.to_path_buf()
                        } else {
                            plan.output_root.join(target)
                        }
                    },
                );
                match operation {
                    OutputOperation::CreateDirectory { .. } => target.exists() && !target.is_dir(),
                    _ => target.exists(),
                }
            })
        {
            return worker::AutoDecision::NeedsReview {
                reason: AutoReviewReason::DestinationCollision,
            };
        }
        approved
    }

    async fn stop_requested(&self, shutdown: &watch::Receiver<bool>, id: JobId) -> bool {
        if *shutdown.borrow() {
            self.interrupt_active(id).await;
            true
        } else {
            false
        }
    }

    async fn claim_with_retry(&self) -> Result<Option<JobRecord>, StoreError> {
        let mut last_error = None;
        for delay in std::iter::once(None).chain(RETRY_DELAYS.map(Some)) {
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            let _operation = self.operations.lock().await;
            match self
                .store
                .claim_oldest_queued(ProgressSummary::new("scanning", 0, None))
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
                    if is_terminal(next) {
                        self.release_job_config(id);
                    }
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
                Err(StoreError::StateConflict { .. }) => {}
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
            }
        }
    }

    fn release_job_config(&self, id: JobId) {
        if let Some(flow) = self.request_flow.get() {
            flow.release(id);
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
    #[error("organization destination violates the filesystem policy: {0}")]
    OrganizationFilesystemPolicy(#[source] FsPolicyError),
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
    #[error("the scrape queue is unavailable")]
    QueueUnavailable,

#[cfg(test)]
mod queue_tests {
    use super::*;

    #[tokio::test]
    async fn dispatch_rechecks_sender_after_unreserved_submission() {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteJobStore::open(directory.path().join("jobs.sqlite"))
            .await
            .unwrap();
        let runtime = JobRuntime::new(store, NonZeroUsize::new(8).unwrap());
        let (sender, mut receiver) = mpsc::channel(1);
        *runtime
            .dispatch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
        let id = JobId::from_database(1).unwrap();

        runtime.dispatch(None, id).await.unwrap();

        assert_eq!(receiver.recv().await, Some(id));
    }
}
