use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use notify::{Config as NotifyConfig, PollWatcher, RecursiveMode, Watcher};
use thiserror::Error;
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::JoinHandle,
    time::{Instant, MissedTickBehavior},
};

use super::{
    IngestionNotification, IngestionRuntime,
    discovery::{DiscoveredItem, DiscoveryError, DiscoveryOutcome, discover},
    model::{IngestionRule, IngestionRuleId, RuleStatus, SourceFingerprint},
};
use crate::{
    jobs::{
        RuntimeError,
        model::{JobInputDto, JobOrganizationDto},
    },
    store::StoreError,
    workspace::{DirectoryRef, WorkspaceStateError},
};

const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(2);
const DEFAULT_RECONCILIATION: Duration = Duration::from_secs(60);
const WATCHER_ERROR: &str = "Folder monitoring is unavailable; periodic retries will continue";
const RECONCILIATION_ERROR: &str = "Folder scan failed; retrying automatically";
const REMOVED_ROOT_ERROR: &str =
    "Configured library root was removed; choose a new folder to re-enable this rule";

/// Timings for filesystem stability checks and periodic missed-event recovery.
#[derive(Debug, Clone, Copy)]
pub struct IngestionSupervisorConfig {
    debounce: Duration,
    reconciliation: Duration,
    poll_interval: Option<Duration>,
}

impl IngestionSupervisorConfig {
    pub fn new(debounce: Duration, reconciliation: Duration) -> Self {
        Self {
            debounce: debounce.max(Duration::from_millis(1)),
            reconciliation: reconciliation.max(Duration::from_millis(1)),
            poll_interval: None,
        }
    }

    /// Uses the portable polling watcher instead of the platform-native backend.
    pub fn with_polling_watcher(mut self, interval: Duration) -> Self {
        self.poll_interval = Some(interval.max(Duration::from_millis(1)));
        self
    }
}

impl Default for IngestionSupervisorConfig {
    fn default() -> Self {
        Self::new(DEFAULT_DEBOUNCE, DEFAULT_RECONCILIATION)
    }
}

/// Background lifecycle handle for folder ingestion.
pub struct IngestionSupervisor {
    shutdown: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl IngestionSupervisor {
    pub fn start(runtime: IngestionRuntime) -> Self {
        Self::start_with_config(runtime, IngestionSupervisorConfig::default())
    }

    pub fn start_with_config(runtime: IngestionRuntime, config: IngestionSupervisorConfig) -> Self {
        let (shutdown, receiver) = watch::channel(false);
        let task = tokio::spawn(run_supervisor(runtime, config, receiver));
        Self {
            shutdown,
            task: Some(task),
        }
    }

    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for IngestionSupervisor {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

#[derive(Debug, Clone)]
struct ResolvedRule {
    rule: IngestionRule,
    source: PathBuf,
    destination: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ObservationKey {
    rule_id: IngestionRuleId,
    source_root: PathBuf,
}

#[derive(Debug)]
struct Observation {
    fingerprint: SourceFingerprint,
    first_seen: Instant,
}

struct Engine {
    runtime: IngestionRuntime,
    config: IngestionSupervisorConfig,
    active: Vec<ResolvedRule>,
    watched: BTreeSet<PathBuf>,
    dirty: HashSet<IngestionRuleId>,
    observations: HashMap<ObservationKey, Observation>,
}

impl Engine {
    fn new(runtime: IngestionRuntime, config: IngestionSupervisorConfig) -> Self {
        Self {
            runtime,
            config,
            active: Vec::new(),
            watched: BTreeSet::new(),
            dirty: HashSet::new(),
            observations: HashMap::new(),
        }
    }

    async fn reload(&mut self, watcher: Option<&mut Box<dyn Watcher + Send>>) {
        let rules = match self.runtime.store.list_ingestion_rules(100).await {
            Ok(rules) => rules,
            Err(error) => {
                tracing::warn!(%error, "could not reload ingestion rules");
                return;
            }
        };
        let mut active = Vec::new();
        for rule in rules.into_iter().filter(IngestionRule::enabled) {
            match resolve_rule(&self.runtime, rule.clone()) {
                Ok(resolved) => active.push(resolved),
                Err(error) => {
                    tracing::warn!(rule_id = rule.id().get(), %error, "ingestion rule path is unavailable");
                    if matches!(error, WorkspaceStateError::RootNotFound) {
                        self.disable_rule(rule.id(), REMOVED_ROOT_ERROR).await;
                    } else {
                        self.record_error(rule.id(), RECONCILIATION_ERROR).await;
                    }
                }
            }
        }

        self.sync_watches(watcher, &active).await;
        let active_ids = active
            .iter()
            .map(|rule| rule.rule.id())
            .collect::<HashSet<_>>();
        self.dirty.retain(|id| active_ids.contains(id));
        self.observations
            .retain(|key, _| active_ids.contains(&key.rule_id));
        self.dirty.extend(active_ids);
        self.active = active;
    }

    async fn sync_watches(
        &mut self,
        watcher: Option<&mut Box<dyn Watcher + Send>>,
        active: &[ResolvedRule],
    ) {
        let desired = active
            .iter()
            .map(|rule| rule.source.clone())
            .collect::<BTreeSet<_>>();
        if let Some(watcher) = watcher {
            for path in self.watched.difference(&desired) {
                let _ = watcher.unwatch(path);
            }
            let mut successful = self
                .watched
                .intersection(&desired)
                .cloned()
                .collect::<BTreeSet<_>>();
            for path in desired.difference(&self.watched) {
                match watcher.watch(path, RecursiveMode::Recursive) {
                    Ok(()) => {
                        successful.insert(path.clone());
                    }
                    Err(error) => {
                        tracing::warn!(path = %path.display(), %error, "could not watch ingestion source");
                        for rule in active.iter().filter(|rule| rule.source == *path) {
                            self.record_error(rule.rule.id(), WATCHER_ERROR).await;
                        }
                    }
                }
            }
            self.watched = successful;
            for rule in active.iter().filter(|rule| {
                self.watched.contains(&rule.source) && rule.rule.last_error().is_some()
            }) {
                self.clear_error(rule.rule.id()).await;
            }
        } else {
            self.watched.clear();
            for rule in active {
                self.record_error(rule.rule.id(), WATCHER_ERROR).await;
            }
        }
    }

    fn mark_event(&mut self) {
        self.dirty
            .extend(self.active.iter().map(|rule| rule.rule.id()));
    }

    fn mark_rule(&mut self, id: IngestionRuleId) {
        if self.active.iter().any(|rule| rule.rule.id() == id) {
            self.dirty.insert(id);
        }
    }

    async fn process_dirty(&mut self) {
        let dirty = self.dirty.iter().copied().collect::<Vec<_>>();
        for id in dirty {
            let Some(rule) = self
                .active
                .iter()
                .find(|rule| rule.rule.id() == id)
                .cloned()
            else {
                self.dirty.remove(&id);
                continue;
            };
            match self.reconcile_rule(&rule).await {
                Ok(pending) => {
                    if !pending {
                        self.dirty.remove(&id);
                    }
                }
                Err(error) => {
                    tracing::warn!(rule_id = id.get(), %error, "ingestion reconciliation failed");
                    self.record_error(id, RECONCILIATION_ERROR).await;
                }
            }
        }
    }

    async fn reconcile_rule(&mut self, rule: &ResolvedRule) -> Result<bool, SupervisorError> {
        let source = rule.source.clone();
        let scan_source = source.clone();
        let mode = rule.rule.media_kind_mode();
        let candidates = tokio::task::spawn_blocking(move || {
            discover(&scan_source, mode)?
                .into_iter()
                .map(|item| {
                    let fingerprint = fingerprint(&scan_source, item.source_root())?;
                    Ok((item, fingerprint))
                })
                .collect::<Result<Vec<_>, SupervisorError>>()
        })
        .await??;

        self.runtime
            .store
            .pause_ingestion_sources(rule.rule.id())
            .await?;

        let now = Instant::now();
        let mut pending = false;
        let mut seen = HashSet::new();
        for (item, fingerprint) in candidates {
            if item.source_root().starts_with(&rule.destination) {
                continue;
            }
            let key = ObservationKey {
                rule_id: rule.rule.id(),
                source_root: item.source_root().to_path_buf(),
            };
            seen.insert(key.clone());
            let stable = match self.observations.get_mut(&key) {
                Some(observation) if observation.fingerprint == fingerprint => {
                    now.duration_since(observation.first_seen) >= self.config.debounce
                }
                Some(observation) => {
                    observation.fingerprint = fingerprint.clone();
                    observation.first_seen = now;
                    false
                }
                None => {
                    self.observations.insert(
                        key,
                        Observation {
                            fingerprint: fingerprint.clone(),
                            first_seen: now,
                        },
                    );
                    false
                }
            };
            if stable {
                self.process_item(rule, item, fingerprint).await?;
            } else {
                pending = true;
            }
        }
        self.observations
            .retain(|key, _| key.rule_id != rule.rule.id() || seen.contains(key));
        Ok(pending)
    }

    async fn process_item(
        &self,
        rule: &ResolvedRule,
        item: DiscoveredItem,
        fingerprint: SourceFingerprint,
    ) -> Result<(), SupervisorError> {
        let reservation = self
            .runtime
            .store
            .reserve_source(rule.rule.id(), fingerprint)
            .await?;
        let source = reservation.source();
        match item.outcome() {
            DiscoveryOutcome::Ignored => {
                self.runtime
                    .store
                    .update_source_status(source.id(), RuleStatus::Paused)
                    .await?;
            }
            DiscoveryOutcome::NeedsReview { media_kinds } => {
                self.runtime
                    .store
                    .update_source_review(source.id(), media_kinds)
                    .await?;
            }
            DiscoveryOutcome::Ready(_) if source.job_id().is_some() => {
                self.runtime
                    .store
                    .update_source_status(source.id(), RuleStatus::Processing)
                    .await?;
            }
            DiscoveryOutcome::Ready(_) => {
                let media_kind = item.media_kind().ok_or(SupervisorError::MissingMediaKind)?;
                let organization = JobOrganizationDto {
                    destination_path: rule.destination.to_string_lossy().into_owned(),
                    placement: rule.rule.placement(),
                    path_template: rule.rule.path_template_override().map(str::to_owned),
                    origin_rule_id: Some(rule.rule.id().get()),
                    auto_execute: true,
                };
                let input = JobInputDto::new(
                    media_kind,
                    item.source_root().to_string_lossy().into_owned(),
                    true,
                )
                .with_organization(organization);
                match self
                    .runtime
                    .jobs
                    .create_for_source(source.id(), input)
                    .await
                {
                    Ok(_) => {}
                    Err(error) => {
                        self.runtime
                            .store
                            .update_source_status(source.id(), RuleStatus::Error)
                            .await?;
                        return Err(error.into());
                    }
                }
            }
        }
        Ok(())
    }

    async fn disable_rule(&self, id: IngestionRuleId, message: &'static str) {
        if let Err(error) = self.runtime.store.disable_ingestion_rule(id, message).await {
            tracing::warn!(rule_id = id.get(), %error, "could not disable ingestion rule");
        }
    }

    async fn record_error(&self, id: IngestionRuleId, message: &'static str) {
        if let Err(error) = self
            .runtime
            .store
            .set_ingestion_rule_error(id, Some(message))
            .await
        {
            tracing::warn!(rule_id = id.get(), %error, "could not persist ingestion rule error");
        }
    }

    async fn clear_error(&self, id: IngestionRuleId) {
        if let Err(error) = self.runtime.store.set_ingestion_rule_error(id, None).await {
            tracing::warn!(rule_id = id.get(), %error, "could not clear ingestion rule error");
        }
    }
}

async fn run_supervisor(
    runtime: IngestionRuntime,
    config: IngestionSupervisorConfig,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut notifications = runtime.notifications.subscribe();
    // A single pending signal is enough: every change rescans all active source rules.
    let (event_sender, mut events) = mpsc::channel(1);
    let watcher = config.poll_interval.map_or_else(
        || {
            let callback_sender = event_sender.clone();
            notify::recommended_watcher(move |event| {
                forward_watcher_event(&callback_sender, event);
            })
            .map(|watcher| Box::new(watcher) as Box<dyn Watcher + Send>)
        },
        |interval| {
            let callback_sender = event_sender.clone();
            PollWatcher::new(
                move |event| forward_watcher_event(&callback_sender, event),
                NotifyConfig::default().with_poll_interval(interval),
            )
            .map(|watcher| Box::new(watcher) as Box<dyn Watcher + Send>)
        },
    );
    let mut watcher = match watcher {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            tracing::warn!(%error, "filesystem watcher initialization failed; using periodic reconciliation");
            None
        }
    };
    let mut engine = Engine::new(runtime, config);
    engine.reload(watcher.as_mut()).await;
    engine.process_dirty().await;

    let mut debounce = tokio::time::interval(config.debounce);
    debounce.set_missed_tick_behavior(MissedTickBehavior::Delay);
    debounce.tick().await;
    let mut periodic = tokio::time::interval(config.reconciliation);
    periodic.set_missed_tick_behavior(MissedTickBehavior::Delay);
    periodic.tick().await;

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            notification = notifications.recv() => {
                match notification {
                    Ok(IngestionNotification::Reload) | Err(broadcast::error::RecvError::Lagged(_)) => {
                        engine.reload(watcher.as_mut()).await;
                    }
                    Ok(IngestionNotification::Rescan(id)) => {
                        engine.reload(watcher.as_mut()).await;
                        engine.mark_rule(id);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            event = events.recv() => {
                match event {
                    Some(Ok(())) => engine.mark_event(),
                    Some(Err(error)) => {
                        tracing::warn!(%error, "filesystem watcher reported an error");
                        for rule in engine.active.clone() {
                            engine.record_error(rule.rule.id(), WATCHER_ERROR).await;
                        }
                    }
                    None => {}
                }
            }
            _ = debounce.tick() => engine.process_dirty().await,
            _ = periodic.tick() => {
                engine.reload(watcher.as_mut()).await;
                engine.process_dirty().await;
            }
        }
    }
}

fn forward_watcher_event(
    sender: &mpsc::Sender<notify::Result<()>>,
    event: notify::Result<notify::Event>,
) {
    match event {
        Ok(event) if event.paths.iter().any(|path| !is_temporary_path(path)) => {
            let _ = sender.try_send(Ok(()));
        }
        Ok(_) => {}
        Err(error) => {
            let _ = sender.try_send(Err(error));
        }
    }
}

fn resolve_rule(
    runtime: &IngestionRuntime,
    rule: IngestionRule,
) -> Result<ResolvedRule, WorkspaceStateError> {
    let source = runtime.workspace.resolve_directory(&DirectoryRef {
        root_id: rule.source().root_id().to_owned(),
        path: rule.source().relative_path().to_owned(),
    })?;
    let destination = runtime.workspace.resolve_directory(&DirectoryRef {
        root_id: rule.destination().root_id().to_owned(),
        path: rule.destination().relative_path().to_owned(),
    })?;
    Ok(ResolvedRule {
        rule,
        source: source.canonical_path,
        destination: destination.canonical_path,
    })
}

pub(crate) fn fingerprint(
    source: &Path,
    item: &Path,
) -> Result<SourceFingerprint, SupervisorError> {
    let relative = item
        .strip_prefix(source)
        .map_err(|_| SupervisorError::EscapedSourceRoot)?;
    let relative = if relative.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    };
    let (size, modified_at_ms) = path_signature(item)?;
    Ok(SourceFingerprint::new(relative, size, modified_at_ms)?)
}

fn path_signature(root: &Path) -> Result<(u64, i64), SupervisorError> {
    let mut size = 0_u64;
    let mut modified_at_ms = 0_i64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        let timestamp = system_time_ms(metadata.modified()?)?;
        modified_at_ms = modified_at_ms.max(timestamp);
        if metadata.file_type().is_symlink() {
            size = size.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
            entries.sort_by_key(fs::DirEntry::file_name);
            pending.extend(entries.into_iter().map(|entry| entry.path()));
        } else if metadata.is_file() {
            size = size.saturating_add(metadata.len());
        }
    }
    Ok((size, modified_at_ms))
}

fn system_time_ms(time: SystemTime) -> Result<i64, SupervisorError> {
    let milliseconds = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SupervisorError::InvalidModifiedTime)?
        .as_millis();
    i64::try_from(milliseconds).map_err(|_| SupervisorError::InvalidModifiedTime)
}

fn is_temporary_path(path: &Path) -> bool {
    let temporary_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("tmp") || extension.eq_ignore_ascii_case("part")
        });
    temporary_extension
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".fixer-") || name.ends_with('~'))
}

#[derive(Debug, Error)]
pub(crate) enum SupervisorError {
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Model(#[from] super::model::IngestionModelError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("blocking discovery task failed")]
    Join(#[from] tokio::task::JoinError),
    #[error("discovered source root escaped its configured directory")]
    EscapedSourceRoot,
    #[error("source modification time is outside the supported range")]
    InvalidModifiedTime,
    #[error("ready discovery item has no media kind")]
    MissingMediaKind,
}
