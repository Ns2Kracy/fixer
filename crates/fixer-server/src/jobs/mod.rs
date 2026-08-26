pub(crate) mod events;
pub mod model;

use std::{num::NonZeroUsize, sync::Arc};

use thiserror::Error;
use tokio::sync::Mutex;

use crate::{
    jobs::{
        events::{JobEventHub, JobEventStream, SubscribeError},
        model::{JobInputDto, JobState},
    },
    store::{JobId, JobRecord, JobUpdate, SqliteJobStore, StoreError},
};

/// Persistent job service shared by HTTP handlers and background workers.
#[derive(Clone)]
pub struct JobRuntime {
    store: SqliteJobStore,
    events: JobEventHub,
    operations: Arc<Mutex<()>>,
}

impl JobRuntime {
    /// Creates a runtime with a fixed non-zero total replay-event capacity.
    pub fn new(store: SqliteJobStore, event_capacity: NonZeroUsize) -> Self {
        Self {
            store,
            events: JobEventHub::new(event_capacity.get()),
            operations: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) async fn create(&self, input: JobInputDto) -> Result<JobRecord, RuntimeError> {
        let _operation = self.operations.lock().await;
        let job = self.store.create_job(input).await?;
        self.events.publish_state(job.id(), job.state())?;
        Ok(job)
    }

    pub(crate) async fn get(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        self.store.get_job(id).await.map_err(Into::into)
    }

    pub(crate) async fn cancel(&self, id: JobId) -> Result<JobRecord, RuntimeError> {
        let _operation = self.operations.lock().await;
        let job = self.store.get_job(id).await?;
        if job.state() != JobState::Queued {
            return Err(RuntimeError::CancellationConflict(job.state()));
        }
        let job = self
            .store
            .transition(
                id,
                JobState::Queued,
                JobState::Cancelled,
                JobUpdate::default(),
            )
            .await?;
        self.events.publish_state(id, job.state())?;
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
