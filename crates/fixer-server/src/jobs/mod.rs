pub(crate) mod events;
pub mod model;
mod worker;

use std::{num::NonZeroUsize, panic::AssertUnwindSafe, sync::Arc};

use futures_util::FutureExt;
use thiserror::Error;
use tokio::sync::{Mutex, Notify, watch};

use crate::{
    jobs::{
        events::{JobEventHub, JobEventStream, SubscribeError},
        model::{JobInputDto, JobState, ProgressSummary, ReviewSummary},
        worker::{RETRY_DELAYS, SharedWorkerFlow, WorkerFlow},
    },
    store::{JobId, JobRecord, JobUpdate, SqliteJobStore, StoreError},
};

pub use worker::{JobFlowError, SdkJobFlow, SearchSummary, WorkerPool};

#[derive(Clone)]
pub struct JobRuntime {
    store: SqliteJobStore,
    events: JobEventHub,
    operations: Arc<Mutex<()>>,
    wake_workers: Arc<Notify>,
}

impl JobRuntime {
    pub fn new(store: SqliteJobStore, event_capacity: NonZeroUsize) -> Self {
        Self {
            store,
            events: JobEventHub::new(event_capacity.get()),
            operations: Arc::new(Mutex::new(())),
            wake_workers: Arc::new(Notify::new()),
        }
    }

    pub fn start_workers(&self, worker_count: NonZeroUsize, flow: SdkJobFlow) -> WorkerPool {
        self.start_worker_flow(worker_count, WorkerFlow::Configured(flow))
    }

    pub(crate) fn start_local_workers(&self, worker_count: NonZeroUsize) -> WorkerPool {
        self.start_worker_flow(worker_count, WorkerFlow::Local)
    }

    fn start_worker_flow(&self, worker_count: NonZeroUsize, flow: WorkerFlow) -> WorkerPool {
        let flow: SharedWorkerFlow = Arc::new(flow);
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
        WorkerPool::new(shutdown, handles)
    }

    pub(crate) async fn create(&self, input: JobInputDto) -> Result<JobRecord, RuntimeError> {
        let _operation = self.operations.lock().await;
        let job = self.store.create_job(input).await?;
        self.events.publish_state(job.id(), job.state())?;
        self.wake_workers.notify_waiters();
        Ok(job)
    }

    pub(crate) async fn get(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        self.store.get_job(id).await.map_err(Into::into)
    }

    pub(crate) async fn cancel(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        let _operation = self.operations.lock().await;
        let job = self.store.get_job(id).await?;
        if !job.state().can_transition_to(JobState::Cancelled) {
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
    #[error("job in state {0} cannot be cancelled at this stage")]
    CancellationConflict(JobState),
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
