pub mod discovery;
pub mod model;
pub mod watcher;

use std::path::Path;

use tokio::sync::broadcast;

use crate::{
    FsPolicy, FsPolicyError, JobRuntime, SqliteJobStore, WorkspaceState, WorkspaceStateError,
    ingestion::{
        discovery::{DiscoveryError, DiscoveryOutcome, discover},
        model::{
            IngestionRule, IngestionRuleId, IngestionRuleInput, IngestionSourceId,
            IngestionSourceReview, MediaKindMode, RuleDirectory, RuleStatus,
        },
        watcher::{SupervisorError, fingerprint},
    },
    jobs::{
        RuntimeError,
        model::{JobInputDto, JobMediaKind, JobOrganizationDto},
    },
    store::{JobRecord, StoreError},
    workspace::DirectoryRef,
};

const NOTIFICATION_CAPACITY: usize = 32;
const RULE_LIST_LIMIT: usize = 100;

/// A successful rule mutation that the ingestion supervisor must observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestionNotification {
    Reload,
    Rescan(IngestionRuleId),
}

/// Lightweight fan-out handle for rule reload and explicit rescan requests.
#[derive(Debug, Clone)]
pub struct IngestionNotifications {
    sender: broadcast::Sender<IngestionNotification>,
}

impl IngestionNotifications {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(NOTIFICATION_CAPACITY);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<IngestionNotification> {
        self.sender.subscribe()
    }

    fn reload(&self) {
        let _ = self.sender.send(IngestionNotification::Reload);
    }

    fn rescan(&self, rule_id: IngestionRuleId) {
        let _ = self.sender.send(IngestionNotification::Rescan(rule_id));
    }
}

impl Default for IngestionNotifications {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared persistence, job, workspace, and notification state for ingestion APIs.
#[derive(Clone)]
pub struct IngestionRuntime {
    store: SqliteJobStore,
    jobs: JobRuntime,
    workspace: WorkspaceState,
    notifications: IngestionNotifications,
}

impl IngestionRuntime {
    pub const fn new(
        store: SqliteJobStore,
        jobs: JobRuntime,
        workspace: WorkspaceState,
        notifications: IngestionNotifications,
    ) -> Self {
        Self {
            store,
            jobs,
            workspace,
            notifications,
        }
    }

    pub async fn list_rules(&self) -> Result<Vec<IngestionRule>, IngestionRuntimeError> {
        self.store
            .list_ingestion_rules(RULE_LIST_LIMIT)
            .await
            .map_err(IngestionRuntimeError::Store)
    }

    pub async fn rule_status(
        &self,
        rule: &IngestionRule,
    ) -> Result<RuleStatus, IngestionRuntimeError> {
        if rule.last_error().is_some() {
            return Ok(RuleStatus::Error);
        }
        if !rule.enabled() {
            return Ok(RuleStatus::Paused);
        }
        self.store
            .ingestion_rule_activity_status(rule.id())
            .await
            .map_err(IngestionRuntimeError::Store)
    }

    pub async fn create_rule(
        &self,
        input: IngestionRuleInput,
    ) -> Result<IngestionRule, IngestionRuntimeError> {
        let rule = self
            .store
            .create_ingestion_rule(input)
            .await
            .map_err(IngestionRuntimeError::Store)?;
        self.notifications.reload();
        Ok(rule)
    }

    pub async fn update_rule(
        &self,
        id: IngestionRuleId,
        input: IngestionRuleInput,
    ) -> Result<Option<IngestionRule>, IngestionRuntimeError> {
        let rule = self
            .store
            .update_ingestion_rule(id, input)
            .await
            .map_err(IngestionRuntimeError::Store)?;
        if rule.is_some() {
            self.notifications.reload();
        }
        Ok(rule)
    }

    pub async fn delete_rule(&self, id: IngestionRuleId) -> Result<bool, IngestionRuntimeError> {
        let deleted = self
            .store
            .delete_ingestion_rule(id)
            .await
            .map_err(IngestionRuntimeError::Store)?;
        if deleted {
            self.notifications.reload();
        }
        Ok(deleted)
    }

    pub async fn review_count(&self, id: IngestionRuleId) -> Result<u64, IngestionRuntimeError> {
        self.store
            .source_review_count(id)
            .await
            .map_err(IngestionRuntimeError::Store)
    }

    pub async fn list_reviews(
        &self,
        id: IngestionRuleId,
    ) -> Result<Option<Vec<IngestionSourceReview>>, IngestionRuntimeError> {
        if self
            .store
            .get_ingestion_rule(id)
            .await
            .map_err(IngestionRuntimeError::Store)?
            .is_none()
        {
            return Ok(None);
        }
        self.store
            .list_source_reviews(id)
            .await
            .map(Some)
            .map_err(IngestionRuntimeError::Store)
    }

    pub async fn resolve_review(
        &self,
        source_id: IngestionSourceId,
        media_kind: JobMediaKind,
    ) -> Result<Option<JobRecord>, IngestionRuntimeError> {
        let Some(review) = self
            .store
            .get_source_review(source_id)
            .await
            .map_err(IngestionRuntimeError::Store)?
        else {
            return Ok(None);
        };
        if !review.media_kinds().contains(&media_kind) {
            return Err(IngestionRuntimeError::InvalidReviewMediaKind);
        }
        let rule = self
            .store
            .get_ingestion_rule(review.rule_id())
            .await
            .map_err(IngestionRuntimeError::Store)?
            .ok_or(IngestionRuntimeError::ReviewRuleMissing)?;
        let source = self
            .workspace
            .resolve_directory(&DirectoryRef {
                root_id: rule.source().root_id().to_owned(),
                path: rule.source().relative_path().to_owned(),
            })
            .map_err(IngestionRuntimeError::Workspace)?;
        let source_path = if review.relative_source_path() == "." {
            source.canonical_path
        } else {
            source
                .canonical_path
                .join(Path::new(review.relative_source_path()))
        };
        let destination = self
            .workspace
            .resolve_directory(&DirectoryRef {
                root_id: rule.destination().root_id().to_owned(),
                path: rule.destination().relative_path().to_owned(),
            })
            .map_err(IngestionRuntimeError::Workspace)?;
        let organization = JobOrganizationDto {
            destination_path: destination.canonical_path.to_string_lossy().into_owned(),
            placement: rule.placement(),
            path_template: rule.path_template_override().map(str::to_owned),
            origin_rule_id: Some(rule.id().get()),
            auto_execute: true,
        };
        let input = JobInputDto::new(media_kind, source_path.to_string_lossy().into_owned(), true)
            .with_organization(organization);
        self.jobs
            .create_for_source(source_id, input)
            .await
            .map(Some)
            .map_err(|_| IngestionRuntimeError::Jobs)
    }

    pub(crate) async fn create_rule_job(
        &self,
        selected: &DirectoryRef,
        requested_kind: Option<JobMediaKind>,
    ) -> Result<JobRecord, RuleJobError> {
        let selected = self.workspace.resolve_directory(selected)?;
        let mut matching_rule = None::<(IngestionRule, std::path::PathBuf)>;
        for rule in self.store.list_ingestion_rules(RULE_LIST_LIMIT).await? {
            if !rule.enabled() {
                continue;
            }
            let Ok(configured_source) = self.workspace.resolve_directory(&DirectoryRef {
                root_id: rule.source().root_id().to_owned(),
                path: rule.source().relative_path().to_owned(),
            }) else {
                continue;
            };
            if !selected
                .canonical_path
                .starts_with(&configured_source.canonical_path)
            {
                continue;
            }
            let depth = configured_source.canonical_path.components().count();
            let is_deeper = matching_rule
                .as_ref()
                .is_none_or(|(_, path)| depth > path.components().count());
            if is_deeper {
                matching_rule = Some((rule, configured_source.canonical_path));
            }
        }
        let (rule, configured_source) = matching_rule.ok_or(RuleJobError::NoMatchingRule)?;

        let mode = match (rule.media_kind_mode(), requested_kind) {
            (MediaKindMode::Auto, Some(kind)) => MediaKindMode::Fixed(kind),
            (mode, None) => mode,
            (MediaKindMode::Fixed(configured), Some(requested)) if configured == requested => {
                MediaKindMode::Fixed(configured)
            }
            (MediaKindMode::Fixed(_), Some(_)) => {
                return Err(RuleJobError::InvalidMediaKindOverride);
            }
        };
        let destination = self.workspace.resolve_directory(&DirectoryRef {
            root_id: rule.destination().root_id().to_owned(),
            path: rule.destination().relative_path().to_owned(),
        })?;
        let policy = FsPolicy::new([&selected.canonical_path, &destination.canonical_path])?;
        let (selected_path, destination_path) = policy
            .validate_directory_pair(&selected.canonical_path, &destination.canonical_path)?;

        let scan_path = selected_path.clone();
        let mut items = tokio::task::spawn_blocking(move || discover(scan_path, mode)).await??;
        items.retain(|item| !matches!(item.outcome(), DiscoveryOutcome::Ignored));
        match items.len() {
            0 => return Err(RuleJobError::NoWork),
            1 => {}
            _ => return Err(RuleJobError::MultipleWorks),
        }
        let item = items.pop().ok_or(RuleJobError::NoWork)?;
        let media_kind = item.media_kind().ok_or(RuleJobError::AmbiguousMedia)?;
        let input_path = item.source_root().to_path_buf();

        let fingerprint_path = input_path.clone();
        let source_fingerprint =
            tokio::task::spawn_blocking(move || fingerprint(&configured_source, &fingerprint_path))
                .await??;
        let reservation = self
            .store
            .reserve_source(rule.id(), source_fingerprint)
            .await?;
        let source = reservation.source();
        if let Some(job_id) = source.job_id() {
            return self.jobs.get(job_id).await.map_err(Into::into);
        }

        let organization = JobOrganizationDto {
            destination_path: destination_path.to_string_lossy().into_owned(),
            placement: rule.placement(),
            path_template: rule.path_template_override().map(str::to_owned),
            origin_rule_id: Some(rule.id().get()),
            auto_execute: false,
        };
        let input = JobInputDto::new(media_kind, input_path.to_string_lossy().into_owned(), true)
            .with_organization(organization);
        self.jobs
            .create_for_source(source.id(), input)
            .await
            .map_err(Into::into)
    }

    pub async fn request_rescan(&self, id: IngestionRuleId) -> Result<bool, IngestionRuntimeError> {
        let exists = self
            .store
            .get_ingestion_rule(id)
            .await
            .map_err(IngestionRuntimeError::Store)?
            .is_some();
        if exists {
            self.notifications.rescan(id);
        }
        Ok(exists)
    }

    pub(crate) fn validate_directories(
        &self,
        source: &DirectoryRef,
        destination: &DirectoryRef,
    ) -> Result<(RuleDirectory, RuleDirectory), IngestionRuntimeError> {
        let source = self
            .workspace
            .resolve_directory(source)
            .map_err(IngestionRuntimeError::Workspace)?;
        let destination = self
            .workspace
            .resolve_directory(destination)
            .map_err(IngestionRuntimeError::Workspace)?;
        let policy = FsPolicy::new([&source.canonical_path, &destination.canonical_path])
            .map_err(IngestionRuntimeError::FilesystemPolicy)?;
        policy
            .validate_directory_pair(&source.canonical_path, &destination.canonical_path)
            .map_err(IngestionRuntimeError::FilesystemPolicy)?;
        let source = RuleDirectory::new(source.root_id, source.relative_path)
            .map_err(IngestionRuntimeError::Model)?;
        let destination = RuleDirectory::new(destination.root_id, destination.relative_path)
            .map_err(IngestionRuntimeError::Model)?;
        Ok((source, destination))
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RuleJobError {
    #[error("no enabled folder rule contains the selected directory")]
    NoMatchingRule,
    #[error("selected directory contains no recognizable media work")]
    NoWork,
    #[error("selected directory contains more than one media work")]
    MultipleWorks,
    #[error("selected media work matches more than one media kind")]
    AmbiguousMedia,
    #[error("media kind cannot override a fixed folder rule")]
    InvalidMediaKindOverride,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceStateError),
    #[error(transparent)]
    FilesystemPolicy(#[from] FsPolicyError),
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error(transparent)]
    Fingerprint(#[from] SupervisorError),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

#[derive(Debug, thiserror::Error)]
pub enum IngestionRuntimeError {
    #[error("persistent ingestion operation failed")]
    Store(#[source] StoreError),
    #[error("ingestion directory reference is invalid")]
    Workspace(#[source] WorkspaceStateError),
    #[error("ingestion directory pair is invalid")]
    FilesystemPolicy(#[source] FsPolicyError),
    #[error("ingestion rule fields are invalid")]
    Model(#[source] model::IngestionModelError),
    #[error("selected media kind is not one of the review options")]
    InvalidReviewMediaKind,
    #[error("the rule associated with this review no longer exists")]
    ReviewRuleMissing,
    #[error("could not create the reviewed ingestion job")]
    Jobs,
}
