pub mod model;

use tokio::sync::broadcast;

use crate::{
    FsPolicy, FsPolicyError, JobRuntime, SqliteJobStore, WorkspaceState, WorkspaceStateError,
    ingestion::model::{IngestionRule, IngestionRuleId, IngestionRuleInput, RuleDirectory},
    store::StoreError,
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
    _jobs: JobRuntime,
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
            _jobs: jobs,
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
pub enum IngestionRuntimeError {
    #[error("persistent ingestion operation failed")]
    Store(#[source] StoreError),
    #[error("ingestion directory reference is invalid")]
    Workspace(#[source] WorkspaceStateError),
    #[error("ingestion directory pair is invalid")]
    FilesystemPolicy(#[source] FsPolicyError),
    #[error("ingestion rule fields are invalid")]
    Model(#[source] model::IngestionModelError),
}
