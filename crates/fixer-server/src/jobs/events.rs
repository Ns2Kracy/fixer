use std::{
    collections::VecDeque,
    convert::Infallible,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::response::sse::Event;
use futures_util::{Stream, stream};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::{
    jobs::model::{ExecutionSummary, JobState, ProgressSummary},
    store::JobId,
};

static NEXT_RUNTIME_EPOCH: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
struct JobEvent {
    sequence: u64,
    job_id: JobId,
    kind: &'static str,
    data: Value,
}

impl JobEvent {
    fn new(job_id: JobId, kind: &'static str, data: Value) -> Self {
        Self {
            sequence: 0,
            job_id,
            kind,
            data,
        }
    }

    fn state(job_id: JobId, state: JobState) -> Self {
        Self::new(
            job_id,
            "state",
            json!({
                "schema_version": 1,
                "job_id": job_id.get(),
                "state": state,
            }),
        )
    }

    fn into_sse(self, epoch: &str) -> Event {
        Event::default()
            .id(format!("{epoch}:{}", self.sequence))
            .event(self.kind)
            .data(self.data.to_string())
    }
}

#[derive(Debug)]
struct EventState {
    epoch: Arc<str>,
    next_sequence: u64,
    retained: VecDeque<JobEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubscribeError {
    Expired,
    Invalid,
    SequenceExhausted,
}

pub(crate) type JobEventStream =
    Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send + 'static>>;

/// Globally bounded event history with atomic replay/live subscription.
#[derive(Debug, Clone)]
pub(crate) struct JobEventHub {
    capacity: usize,
    state: Arc<Mutex<EventState>>,
    sender: broadcast::Sender<JobEvent>,
}

impl JobEventHub {
    pub(crate) fn new(capacity: usize) -> Self {
        debug_assert!(capacity > 0);
        let (sender, _) = broadcast::channel(capacity);
        Self {
            capacity,
            state: Arc::new(Mutex::new(EventState {
                epoch: runtime_epoch(),
                next_sequence: 0,
                retained: VecDeque::with_capacity(capacity),
            })),
            sender,
        }
    }

    pub(crate) fn publish_state(
        &self,
        job_id: JobId,
        state: JobState,
    ) -> Result<(), SubscribeError> {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.publish_locked(&mut guard, JobEvent::state(job_id, state))
    }

    pub(crate) fn publish_progress(
        &self,
        job_id: JobId,
        progress: &ProgressSummary,
    ) -> Result<(), SubscribeError> {
        self.publish(JobEvent::new(
            job_id,
            "progress",
            json!({
                "schema_version": 1,
                "job_id": job_id.get(),
                "progress": progress,
            }),
        ))
    }

    pub(crate) fn publish_completion(
        &self,
        job_id: JobId,
        execution: &ExecutionSummary,
    ) -> Result<(), SubscribeError> {
        self.publish(JobEvent::new(
            job_id,
            "completion",
            json!({
                "schema_version": 1,
                "job_id": job_id.get(),
                "execution": execution,
            }),
        ))
    }

    pub(crate) fn publish_review(
        &self,
        job_id: JobId,
        candidate_count: u64,
        conflict_count: u64,
    ) -> Result<(), SubscribeError> {
        self.publish(JobEvent::new(
            job_id,
            "review",
            json!({
                "schema_version": 1,
                "job_id": job_id.get(),
                "candidate_count": candidate_count,
                "conflict_count": conflict_count,
            }),
        ))
    }

    fn publish(&self, event: JobEvent) -> Result<(), SubscribeError> {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.publish_locked(&mut guard, event)
    }

    pub(crate) fn ensure_state(
        &self,
        job_id: JobId,
        state: JobState,
    ) -> Result<(), SubscribeError> {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.retained.iter().any(|event| event.job_id == job_id) {
            return Ok(());
        }
        self.publish_locked(&mut guard, JobEvent::state(job_id, state))
    }

    pub(crate) fn subscribe(
        &self,
        job_id: JobId,
        raw_cursor: Option<&str>,
    ) -> Result<JobEventStream, SubscribeError> {
        let guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cursor = raw_cursor.map(parse_cursor).transpose()?;
        let sequence = match cursor {
            Some((epoch, sequence)) => {
                if epoch != guard.epoch.as_ref() {
                    return Err(SubscribeError::Expired);
                }
                if sequence > guard.next_sequence {
                    return Err(SubscribeError::Invalid);
                }
                if let Some(oldest) = guard.retained.front() {
                    let next = sequence.checked_add(1).ok_or(SubscribeError::Invalid)?;
                    if next < oldest.sequence {
                        return Err(SubscribeError::Expired);
                    }
                }
                Some(sequence)
            }
            None => None,
        };
        let replay = guard
            .retained
            .iter()
            .filter(|event| {
                event.job_id == job_id && sequence.is_none_or(|sequence| event.sequence > sequence)
            })
            .cloned()
            .collect::<VecDeque<_>>();
        let receiver = self.sender.subscribe();
        let subscription = Subscription {
            epoch: Arc::clone(&guard.epoch),
            job_id,
            replay,
            receiver,
        };
        drop(guard);
        Ok(subscription.into_stream())
    }

    fn publish_locked(
        &self,
        state: &mut EventState,
        mut event: JobEvent,
    ) -> Result<(), SubscribeError> {
        let sequence = state
            .next_sequence
            .checked_add(1)
            .ok_or(SubscribeError::SequenceExhausted)?;
        state.next_sequence = sequence;
        event.sequence = sequence;
        state.retained.push_back(event.clone());
        while state.retained.len() > self.capacity {
            state.retained.pop_front();
        }
        let _ = self.sender.send(event);
        Ok(())
    }
}

struct Subscription {
    epoch: Arc<str>,
    job_id: JobId,
    replay: VecDeque<JobEvent>,
    receiver: broadcast::Receiver<JobEvent>,
}

impl Subscription {
    fn into_stream(self) -> JobEventStream {
        Box::pin(stream::unfold(self, |mut subscription| async move {
            if let Some(event) = subscription.replay.pop_front() {
                let epoch = Arc::clone(&subscription.epoch);
                return Some((Ok(event.into_sse(&epoch)), subscription));
            }
            loop {
                match subscription.receiver.recv().await {
                    Ok(event) if event.job_id == subscription.job_id => {
                        let epoch = Arc::clone(&subscription.epoch);
                        return Some((Ok(event.into_sse(&epoch)), subscription));
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_))
                    | Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        }))
    }
}

fn parse_cursor(value: &str) -> Result<(&str, u64), SubscribeError> {
    let (epoch, sequence) = value.split_once(':').ok_or(SubscribeError::Invalid)?;
    if epoch.is_empty() {
        return Err(SubscribeError::Invalid);
    }
    let sequence = sequence
        .parse::<u64>()
        .map_err(|_| SubscribeError::Invalid)?;
    Ok((epoch, sequence))
}

fn runtime_epoch() -> Arc<str> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let local = NEXT_RUNTIME_EPOCH.fetch_add(1, Ordering::Relaxed);
    Arc::from(format!("{nanos:x}-{:x}-{local:x}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use tokio::time::{Duration, timeout};

    use super::JobEventHub;
    use crate::{jobs::model::JobState, store::JobId};

    #[test]
    fn retained_history_is_globally_capacity_bounded() {
        let hub = JobEventHub::new(3);
        for raw_id in 1..=20 {
            let id = JobId::from_database(raw_id).unwrap();
            hub.publish_state(id, JobState::Queued).unwrap();
        }
        let state = hub
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(state.retained.len(), 3);
    }

    #[tokio::test]
    async fn live_subscription_cannot_miss_a_publish_after_its_snapshot() {
        let hub = JobEventHub::new(4);
        let id = JobId::from_database(1).unwrap();
        let mut stream = hub.subscribe(id, None).unwrap();
        hub.publish_state(id, JobState::Queued).unwrap();
        let _event = timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("live event timed out")
            .expect("live stream ended")
            .expect("live stream failed");
    }
}
