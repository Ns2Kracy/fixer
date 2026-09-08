use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
};

use crate::{
    auth::{
        IssuedApiToken, IssuedSession,
        password::{PasswordHashValue, verify_password},
        session::issue_session_secrets,
        token::{digest, issue_secret},
    },
    ingestion::model::{
        IngestionModelError, IngestionRule, IngestionRuleId, IngestionRuleInput,
        IngestionRuleParts, IngestionSource, IngestionSourceId, IngestionSourceParts,
        IngestionSourceReview, MediaKindMode, RuleDirectory, RulePlacement, RuleStatus,
        SourceFingerprint, SourceReservation,
    },
    jobs::model::{JobInputDto, JobState, ProgressSummary},
    store::{ExecutionReservation, JobId, JobRecord, JobRecordParts, JobUpdate, StoreError},
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
const ACTIVE_STATES: [&str; 5] = ["scanning", "searching", "resolving", "planning", "writing"];
const RECORD_COLUMNS: &str = "id, input_json, state, progress_json, review_json, review_decision_json, plan_json, execution_json, created_at_ms, updated_at_ms";
const INGESTION_RULE_COLUMNS: &str = "id, name, source_root_id, source_relative_path, destination_root_id, destination_relative_path, media_kind_mode, fixed_media_kind, placement, path_template_override, enabled, last_error, created_at_ms, updated_at_ms";
const INGESTION_SOURCE_COLUMNS: &str = "id, rule_id, relative_source_path, size_bytes, modified_at_ms, status, job_id, created_at_ms, updated_at_ms";
const MAX_INGESTION_RULE_LIST_LIMIT: usize = 100;
const MAX_INGESTION_RULES: i64 = 100;

#[derive(Clone)]
pub struct SqliteJobStore {
    pool: SqlitePool,
    _lease: Arc<File>,
}

impl SqliteJobStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        let lease = Arc::new(acquire_lease(path)?);
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        let store = Self {
            pool,
            _lease: lease,
        };
        store.interrupt_active_jobs().await?;
        Ok(store)
    }

    pub async fn has_registered_user(&self) -> Result<bool, StoreError> {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM fixer_users WHERE id = 1 AND username IS NOT NULL)",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(exists != 0)
    }

    pub async fn register_single_user(
        &self,
        username: &str,
        password_hash: &PasswordHashValue,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "INSERT INTO fixer_users (id, username, password_hash, updated_at_ms) \
             VALUES (1, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
               username = excluded.username, \
               password_hash = excluded.password_hash, \
               updated_at_ms = excluded.updated_at_ms \
             WHERE fixer_users.username IS NULL",
        )
        .bind(username)
        .bind(password_hash.as_str())
        .bind(timestamp_ms()?)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn registered_username(&self) -> Result<Option<String>, StoreError> {
        sqlx::query_scalar("SELECT username FROM fixer_users WHERE id = 1 AND username IS NOT NULL")
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn verify_single_user_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> Result<bool, StoreError> {
        let Some((registered_username, encoded)) = sqlx::query_as::<_, (String, String)>(
            "SELECT username, password_hash FROM fixer_users WHERE id = 1 AND username IS NOT NULL",
        )
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(false);
        };
        let encoded = PasswordHashValue::parse(encoded)?;
        let password = password.to_owned();
        let password_valid =
            tokio::task::spawn_blocking(move || verify_password(&password, &encoded)).await??;
        Ok(registered_username == username && password_valid)
    }

    pub async fn create_session(&self, lifetime: Duration) -> Result<IssuedSession, StoreError> {
        let lifetime_ms =
            i64::try_from(lifetime.as_millis()).map_err(|_| StoreError::TimestampOverflow)?;
        if lifetime_ms <= 0 {
            return Err(StoreError::CorruptRecord(
                "session lifetime must be positive".to_owned(),
            ));
        }
        let created_at_ms = timestamp_ms()?;
        let expires_at_ms = created_at_ms
            .checked_add(lifetime_ms)
            .ok_or(StoreError::TimestampOverflow)?;
        let secrets = issue_session_secrets()?;
        sqlx::query(
            "INSERT INTO fixer_sessions (token_digest, csrf_digest, created_at_ms, expires_at_ms) VALUES (?, ?, ?, ?)",
        )
        .bind(secrets.token_digest.as_slice())
        .bind(secrets.csrf_digest.as_slice())
        .bind(created_at_ms)
        .bind(expires_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(IssuedSession::new(
            secrets.token,
            secrets.csrf_token,
            expires_at_ms,
        ))
    }

    pub async fn authenticate_session(
        &self,
        token: &str,
        csrf_token: Option<&str>,
    ) -> Result<bool, StoreError> {
        if !token.starts_with("fixer_session_") {
            return Ok(false);
        }
        let csrf_digest = csrf_token.map(digest);
        let authenticated: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fixer_sessions WHERE token_digest = ? AND expires_at_ms > ? AND (? IS NULL OR csrf_digest = ?))",
        )
        .bind(digest(token).as_slice())
        .bind(timestamp_ms()?)
        .bind(csrf_digest.as_ref().map(<[u8; 32]>::as_slice))
        .bind(csrf_digest.as_ref().map(<[u8; 32]>::as_slice))
        .fetch_one(&self.pool)
        .await?;
        Ok(authenticated == 1)
    }

    pub async fn revoke_session(&self, token: &str) -> Result<bool, StoreError> {
        if !token.starts_with("fixer_session_") {
            return Ok(false);
        }
        let result = sqlx::query("DELETE FROM fixer_sessions WHERE token_digest = ?")
            .bind(digest(token).as_slice())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn issue_api_token(&self, name: &str) -> Result<IssuedApiToken, StoreError> {
        let name = name.trim();
        if name.is_empty() || name.len() > 100 {
            return Err(StoreError::CorruptRecord(
                "API token name must contain between 1 and 100 bytes".to_owned(),
            ));
        }
        let token = issue_secret("fixer_pat_")?;
        let result = sqlx::query(
            "INSERT INTO fixer_api_tokens (name, token_digest, created_at_ms) VALUES (?, ?, ?)",
        )
        .bind(name)
        .bind(digest(&token).as_slice())
        .bind(timestamp_ms()?)
        .execute(&self.pool)
        .await?;
        Ok(IssuedApiToken::new(result.last_insert_rowid(), token))
    }

    pub async fn authenticate_api_token(&self, token: &str) -> Result<Option<i64>, StoreError> {
        if !token.starts_with("fixer_pat_") {
            return Ok(None);
        }
        sqlx::query_scalar(
            "SELECT id FROM fixer_api_tokens WHERE token_digest = ? AND revoked_at_ms IS NULL",
        )
        .bind(digest(token).as_slice())
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn revoke_api_token(&self, id: i64) -> Result<bool, StoreError> {
        if id <= 0 {
            return Ok(false);
        }
        let result = sqlx::query(
            "UPDATE fixer_api_tokens SET revoked_at_ms = ? WHERE id = ? AND revoked_at_ms IS NULL",
        )
        .bind(timestamp_ms()?)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn create_ingestion_rule(
        &self,
        input: IngestionRuleInput,
    ) -> Result<IngestionRule, StoreError> {
        let now = timestamp_ms()?;
        let (mode, fixed_kind) = input.media_kind_mode().storage_parts();
        let mut transaction = self.pool.begin().await?;
        let rule_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ingestion_rules")
            .fetch_one(&mut *transaction)
            .await?;
        if rule_count >= MAX_INGESTION_RULES {
            return Err(StoreError::IngestionRuleLimit {
                limit: MAX_INGESTION_RULES,
            });
        }
        let result = sqlx::query(
            "INSERT INTO ingestion_rules (name, source_root_id, source_relative_path, destination_root_id, destination_relative_path, media_kind_mode, fixed_media_kind, placement, path_template_override, enabled, last_error, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.name())
        .bind(input.source().root_id())
        .bind(input.source().relative_path())
        .bind(input.destination().root_id())
        .bind(input.destination().relative_path())
        .bind(mode)
        .bind(fixed_kind)
        .bind(input.placement().as_str())
        .bind(input.path_template_override())
        .bind(i64::from(input.enabled()))
        .bind(input.last_error())
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let id = IngestionRuleId::from_database(result.last_insert_rowid())
            .map_err(corrupt_ingestion_record)?;
        let sql = format!("SELECT {INGESTION_RULE_COLUMNS} FROM ingestion_rules WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id.get())
            .fetch_one(&mut *transaction)
            .await?;
        let rule = decode_ingestion_rule(&row)?;
        transaction.commit().await?;
        Ok(rule)
    }

    pub async fn list_ingestion_rules(
        &self,
        limit: usize,
    ) -> Result<Vec<IngestionRule>, StoreError> {
        if limit > MAX_INGESTION_RULE_LIST_LIMIT {
            return Err(StoreError::CorruptRecord(format!(
                "ingestion rule list limit must not exceed {MAX_INGESTION_RULE_LIST_LIMIT}"
            )));
        }
        let limit = i64::try_from(limit).map_err(|_| {
            StoreError::CorruptRecord("ingestion rule list limit exceeds SQLite range".to_owned())
        })?;
        let sql = format!(
            "SELECT {INGESTION_RULE_COLUMNS} FROM ingestion_rules ORDER BY id DESC LIMIT ?"
        );
        let rows = sqlx::query(&sql).bind(limit).fetch_all(&self.pool).await?;
        rows.iter().map(decode_ingestion_rule).collect()
    }

    pub async fn get_ingestion_rule(
        &self,
        id: IngestionRuleId,
    ) -> Result<Option<IngestionRule>, StoreError> {
        let sql = format!("SELECT {INGESTION_RULE_COLUMNS} FROM ingestion_rules WHERE id = ?");
        sqlx::query(&sql)
            .bind(id.get())
            .fetch_optional(&self.pool)
            .await?
            .map(|row| decode_ingestion_rule(&row))
            .transpose()
    }

    pub(crate) async fn ingestion_rule_activity_status(
        &self,
        id: IngestionRuleId,
    ) -> Result<RuleStatus, StoreError> {
        let (has_error, needs_review, processing) = sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT \
             COALESCE(MAX(CASE WHEN jobs.state IN ('failed', 'interrupted', 'cancelled') THEN 1 ELSE 0 END), 0), \
             COALESCE(MAX(CASE WHEN ingestion_sources.status = 'needs_review' OR jobs.state IN ('awaiting_review', 'awaiting_confirmation') THEN 1 ELSE 0 END), 0), \
             COALESCE(MAX(CASE WHEN ingestion_sources.status = 'processing' AND (ingestion_sources.job_id IS NULL OR jobs.state IN ('queued', 'scanning', 'searching', 'resolving', 'planning', 'writing')) THEN 1 ELSE 0 END), 0) \
             FROM ingestion_sources LEFT JOIN jobs ON jobs.id = ingestion_sources.job_id \
             WHERE ingestion_sources.rule_id = ? AND ingestion_sources.status != 'paused'",
        )
        .bind(id.get())
        .fetch_one(&self.pool)
        .await?;
        Ok(if has_error != 0 {
            RuleStatus::Error
        } else if needs_review != 0 {
            RuleStatus::NeedsReview
        } else if processing != 0 {
            RuleStatus::Processing
        } else {
            RuleStatus::Watching
        })
    }

    pub async fn update_ingestion_rule(
        &self,
        id: IngestionRuleId,
        input: IngestionRuleInput,
    ) -> Result<Option<IngestionRule>, StoreError> {
        let (mode, fixed_kind) = input.media_kind_mode().storage_parts();
        let sql = format!(
            "UPDATE ingestion_rules SET name = ?, source_root_id = ?, source_relative_path = ?, destination_root_id = ?, destination_relative_path = ?, media_kind_mode = ?, fixed_media_kind = ?, placement = ?, path_template_override = ?, enabled = ?, last_error = ?, updated_at_ms = ? WHERE id = ? RETURNING {INGESTION_RULE_COLUMNS}"
        );
        sqlx::query(&sql)
            .bind(input.name())
            .bind(input.source().root_id())
            .bind(input.source().relative_path())
            .bind(input.destination().root_id())
            .bind(input.destination().relative_path())
            .bind(mode)
            .bind(fixed_kind)
            .bind(input.placement().as_str())
            .bind(input.path_template_override())
            .bind(i64::from(input.enabled()))
            .bind(input.last_error())
            .bind(timestamp_ms()?)
            .bind(id.get())
            .fetch_optional(&self.pool)
            .await?
            .map(|row| decode_ingestion_rule(&row))
            .transpose()
    }

    pub async fn disable_ingestion_rule(
        &self,
        id: IngestionRuleId,
        error: &str,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE ingestion_rules SET enabled = 0, last_error = ?, updated_at_ms = ? WHERE id = ?",
        )
        .bind(error)
        .bind(timestamp_ms()?)
        .bind(id.get())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn set_ingestion_rule_error(
        &self,
        id: IngestionRuleId,
        error: Option<&str>,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE ingestion_rules SET last_error = ?, updated_at_ms = ? WHERE id = ?",
        )
        .bind(error)
        .bind(timestamp_ms()?)
        .bind(id.get())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn delete_ingestion_rule(&self, id: IngestionRuleId) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM ingestion_rules WHERE id = ?")
            .bind(id.get())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn reserve_source(
        &self,
        rule_id: IngestionRuleId,
        fingerprint: SourceFingerprint,
    ) -> Result<SourceReservation, StoreError> {
        let now = timestamp_ms()?;
        let mut transaction = self.pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO ingestion_sources (rule_id, relative_source_path, size_bytes, modified_at_ms, status, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, 'processing', ?, ?) ON CONFLICT(rule_id, relative_source_path, size_bytes, modified_at_ms) DO NOTHING",
        )
        .bind(rule_id.get())
        .bind(fingerprint.relative_source_path())
        .bind(fingerprint.size_for_database())
        .bind(fingerprint.modified_at_ms())
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let sql = format!(
            "SELECT {INGESTION_SOURCE_COLUMNS} FROM ingestion_sources WHERE rule_id = ? AND relative_source_path = ? AND size_bytes = ? AND modified_at_ms = ?"
        );
        let row = sqlx::query(&sql)
            .bind(rule_id.get())
            .bind(fingerprint.relative_source_path())
            .bind(fingerprint.size_for_database())
            .bind(fingerprint.modified_at_ms())
            .fetch_one(&mut *transaction)
            .await?;
        let source = decode_ingestion_source(&row)?;
        transaction.commit().await?;
        if inserted == 1 {
            Ok(SourceReservation::Reserved(source))
        } else {
            Ok(SourceReservation::Existing(source))
        }
    }

    pub async fn associate_source_job(
        &self,
        source_id: IngestionSourceId,
        job_id: JobId,
    ) -> Result<IngestionSource, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let sql = format!(
            "UPDATE ingestion_sources SET job_id = ?, updated_at_ms = ? WHERE id = ? AND job_id IS NULL RETURNING {INGESTION_SOURCE_COLUMNS}"
        );
        if let Some(row) = sqlx::query(&sql)
            .bind(job_id.get())
            .bind(timestamp_ms()?)
            .bind(source_id.get())
            .fetch_optional(&mut *transaction)
            .await?
        {
            let source = decode_ingestion_source(&row)?;
            transaction.commit().await?;
            return Ok(source);
        }

        let sql = format!("SELECT {INGESTION_SOURCE_COLUMNS} FROM ingestion_sources WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(source_id.get())
            .fetch_optional(&mut *transaction)
            .await?;
        if let Some(row) = row {
            let source = decode_ingestion_source(&row)?;
            transaction.commit().await?;
            if source.job_id() == Some(job_id) {
                Ok(source)
            } else {
                Err(StoreError::IngestionSourceJobConflict {
                    id: source_id.get(),
                })
            }
        } else {
            transaction.rollback().await?;
            Err(StoreError::IngestionSourceNotFound {
                id: source_id.get(),
            })
        }
    }

    pub async fn pause_ingestion_sources(
        &self,
        rule_id: IngestionRuleId,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE ingestion_sources SET status = 'paused', review_kinds_json = NULL, updated_at_ms = ? WHERE rule_id = ? AND status != 'paused'",
        )
        .bind(timestamp_ms()?)
        .bind(rule_id.get())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_source_review(
        &self,
        source_id: IngestionSourceId,
        media_kinds: &[crate::jobs::model::JobMediaKind],
    ) -> Result<IngestionSource, StoreError> {
        let review_json = serde_json::to_string(media_kinds)?;
        let sql = format!(
            "UPDATE ingestion_sources SET status = 'needs_review', review_kinds_json = ?, updated_at_ms = ? WHERE id = ? RETURNING {INGESTION_SOURCE_COLUMNS}"
        );
        let row = sqlx::query(&sql)
            .bind(review_json)
            .bind(timestamp_ms()?)
            .bind(source_id.get())
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::IngestionSourceNotFound {
                id: source_id.get(),
            })?;
        decode_ingestion_source(&row)
    }

    pub async fn source_review_count(&self, rule_id: IngestionRuleId) -> Result<u64, StoreError> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ingestion_sources WHERE rule_id = ? AND status = 'needs_review' AND job_id IS NULL",
        )
        .bind(rule_id.get())
        .fetch_one(&self.pool)
        .await?;
        u64::try_from(count).map_err(|_| {
            StoreError::CorruptRecord("ingestion review count must not be negative".to_owned())
        })
    }

    pub async fn list_source_reviews(
        &self,
        rule_id: IngestionRuleId,
    ) -> Result<Vec<IngestionSourceReview>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, rule_id, relative_source_path, review_kinds_json FROM ingestion_sources WHERE rule_id = ? AND status = 'needs_review' AND job_id IS NULL ORDER BY id LIMIT 100",
        )
        .bind(rule_id.get())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(decode_source_review).collect()
    }

    pub async fn get_source_review(
        &self,
        source_id: IngestionSourceId,
    ) -> Result<Option<IngestionSourceReview>, StoreError> {
        sqlx::query(
            "SELECT id, rule_id, relative_source_path, review_kinds_json FROM ingestion_sources WHERE id = ? AND status = 'needs_review' AND job_id IS NULL",
        )
        .bind(source_id.get())
        .fetch_optional(&self.pool)
        .await?
        .map(|row| decode_source_review(&row))
        .transpose()
    }

    pub async fn update_source_status(
        &self,
        source_id: IngestionSourceId,
        status: RuleStatus,
    ) -> Result<IngestionSource, StoreError> {
        let sql = format!(
            "UPDATE ingestion_sources SET status = ?, updated_at_ms = ? WHERE id = ? RETURNING {INGESTION_SOURCE_COLUMNS}"
        );
        let row = sqlx::query(&sql)
            .bind(status.as_str())
            .bind(timestamp_ms()?)
            .bind(source_id.get())
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::IngestionSourceNotFound {
                id: source_id.get(),
            })?;
        decode_ingestion_source(&row)
    }

    pub async fn get_source_job(
        &self,
        source_id: IngestionSourceId,
    ) -> Result<Option<JobRecord>, StoreError> {
        let job_id = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT job_id FROM ingestion_sources WHERE id = ?",
        )
        .bind(source_id.get())
        .fetch_optional(&self.pool)
        .await?
        .flatten();
        let Some(job_id) = job_id else {
            return Ok(None);
        };
        let job_id = JobId::from_database(job_id)?;
        self.get_job(job_id).await.map(Some)
    }

    pub async fn create_job_for_source(
        &self,
        source_id: IngestionSourceId,
        input: JobInputDto,
    ) -> Result<JobRecord, StoreError> {
        let now = timestamp_ms()?;
        let input_json = serde_json::to_string(&input)?;
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query(
            "INSERT INTO jobs (input_json, state, created_at_ms, updated_at_ms) VALUES (?, 'queued', ?, ?)",
        )
        .bind(input_json)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let job_id = JobId::from_database(result.last_insert_rowid())?;
        let associated = sqlx::query(
            "UPDATE ingestion_sources SET job_id = ?, status = 'processing', review_kinds_json = NULL, updated_at_ms = ? WHERE id = ? AND job_id IS NULL",
        )
        .bind(job_id.get())
        .bind(now)
        .bind(source_id.get())
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        if associated != 1 {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(SELECT 1 FROM ingestion_sources WHERE id = ?)",
            )
            .bind(source_id.get())
            .fetch_one(&mut *transaction)
            .await?;
            transaction.rollback().await?;
            return Err(if exists == 0 {
                StoreError::IngestionSourceNotFound {
                    id: source_id.get(),
                }
            } else {
                StoreError::IngestionSourceJobConflict {
                    id: source_id.get(),
                }
            });
        }
        let sql = format!("SELECT {RECORD_COLUMNS} FROM jobs WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(job_id.get())
            .fetch_one(&mut *transaction)
            .await?;
        let job = decode_record(&row)?;
        transaction.commit().await?;
        Ok(job)
    }

    pub async fn create_job(&self, input: JobInputDto) -> Result<JobRecord, StoreError> {
        let now = timestamp_ms()?;
        let input_json = serde_json::to_string(&input)?;
        let result = sqlx::query(
            "INSERT INTO jobs (input_json, state, created_at_ms, updated_at_ms) VALUES (?, 'queued', ?, ?)",
        )
        .bind(input_json)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        let id = JobId::from_database(result.last_insert_rowid())?;
        self.get_job(id).await
    }

    pub(crate) async fn claim_next_queued(
        &self,
        progress: ProgressSummary,
    ) -> Result<Option<JobRecord>, StoreError> {
        let progress_json = serde_json::to_string(&progress)?;
        let updated_at_ms = timestamp_ms()?;
        let sql = format!(
            "UPDATE jobs SET state = 'scanning', progress_json = ?, updated_at_ms = ? WHERE id = (SELECT id FROM jobs WHERE state = 'queued' ORDER BY id LIMIT 1) AND state = 'queued' RETURNING {RECORD_COLUMNS}"
        );
        sqlx::query(&sql)
            .bind(progress_json)
            .bind(updated_at_ms)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| decode_record(&row))
            .transpose()
    }

    pub async fn get_job(&self, id: JobId) -> Result<JobRecord, StoreError> {
        let sql = format!("SELECT {RECORD_COLUMNS} FROM jobs WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id.get())
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::NotFound { id: id.get() })?;
        decode_record(&row)
    }

    pub async fn list_jobs(
        &self,
        limit: usize,
        state: Option<JobState>,
    ) -> Result<Vec<JobRecord>, StoreError> {
        let limit = i64::try_from(limit).map_err(|_| {
            StoreError::CorruptRecord("job list limit exceeds SQLite range".to_owned())
        })?;
        let rows = if let Some(state) = state {
            let sql = format!(
                "SELECT {RECORD_COLUMNS} FROM jobs WHERE state = ? ORDER BY id DESC LIMIT ?"
            );
            sqlx::query(&sql)
                .bind(state.to_string())
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
        } else {
            let sql = format!("SELECT {RECORD_COLUMNS} FROM jobs ORDER BY id DESC LIMIT ?");
            sqlx::query(&sql).bind(limit).fetch_all(&self.pool).await?
        };
        rows.iter().map(decode_record).collect()
    }

    pub async fn transition(
        &self,
        id: JobId,
        expected: JobState,
        next: JobState,
        update: JobUpdate,
    ) -> Result<JobRecord, StoreError> {
        if !expected.can_transition_to(next) {
            return Err(StoreError::InvalidTransition {
                from: expected,
                to: next,
            });
        }
        if expected == JobState::Planning && next == JobState::Writing {
            return Err(StoreError::ExecutionReservationRequired { id: id.get() });
        }
        if expected == JobState::Interrupted
            && next == JobState::Queued
            && self.has_execution_reservation(id).await?
        {
            return Err(StoreError::ReservedExecutionRetry { id: id.get() });
        }

        let progress = serialize_optional(update.progress.as_ref())?;
        let review = serialize_optional(update.review.as_ref())?;
        let review_decision = serialize_optional(update.review_decision.as_ref())?;
        let plan = serialize_optional(update.plan.as_ref())?;
        let execution = serialize_optional(update.execution.as_ref())?;
        let updated_at_ms = timestamp_ms()?;
        let sql = format!(
            "UPDATE jobs SET state = ?, progress_json = COALESCE(?, progress_json), review_json = COALESCE(?, review_json), review_decision_json = COALESCE(?, review_decision_json), plan_json = COALESCE(?, plan_json), execution_json = COALESCE(?, execution_json), updated_at_ms = ? WHERE id = ? AND state = ? RETURNING {RECORD_COLUMNS}"
        );
        let row = sqlx::query(&sql)
            .bind(next.to_string())
            .bind(progress)
            .bind(review)
            .bind(review_decision)
            .bind(plan)
            .bind(execution)
            .bind(updated_at_ms)
            .bind(id.get())
            .bind(expected.to_string())
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => decode_record(&row),
            None => Err(self.transition_conflict(id, expected).await?),
        }
    }

    pub async fn reserve_execution(
        &self,
        id: JobId,
        idempotency_key: &str,
        request_fingerprint: &str,
    ) -> Result<ExecutionReservation, StoreError> {
        let now = timestamp_ms()?;
        let mut transaction = self.pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO job_executions (job_id, idempotency_key, request_fingerprint, created_at_ms) SELECT ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM jobs WHERE id = ? AND state = 'planning') ON CONFLICT(job_id) DO NOTHING",
        )
        .bind(id.get())
        .bind(idempotency_key)
        .bind(request_fingerprint)
        .bind(now)
        .bind(id.get())
        .execute(&mut *transaction)
        .await?
        .rows_affected();

        if inserted == 1 {
            let sql = format!(
                "UPDATE jobs SET state = 'writing', progress_json = ?, updated_at_ms = ? WHERE id = ? AND state = 'planning' RETURNING {RECORD_COLUMNS}"
            );
            let progress = serde_json::to_string(&ProgressSummary::new("writing", 0, None))?;
            let row = sqlx::query(&sql)
                .bind(progress)
                .bind(now)
                .bind(id.get())
                .fetch_one(&mut *transaction)
                .await?;
            let job = decode_record(&row)?;
            transaction.commit().await?;
            return Ok(ExecutionReservation::Reserved(job));
        }

        let existing = sqlx::query(
            "SELECT idempotency_key, request_fingerprint FROM job_executions WHERE job_id = ?",
        )
        .bind(id.get())
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(existing) = existing else {
            transaction.rollback().await?;
            return Err(self.transition_conflict(id, JobState::Planning).await?);
        };
        let existing_key: String = existing.try_get("idempotency_key")?;
        let existing_fingerprint: String = existing.try_get("request_fingerprint")?;
        if existing_key != idempotency_key || existing_fingerprint != request_fingerprint {
            transaction.rollback().await?;
            return Err(StoreError::IdempotencyConflict { id: id.get() });
        }
        let sql = format!("SELECT {RECORD_COLUMNS} FROM jobs WHERE id = ?");
        let row = sqlx::query(&sql)
            .bind(id.get())
            .fetch_one(&mut *transaction)
            .await?;
        let job = decode_record(&row)?;
        transaction.commit().await?;
        Ok(ExecutionReservation::Existing(job))
    }

    async fn has_execution_reservation(&self, id: JobId) -> Result<bool, StoreError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_executions WHERE job_id = ?")
            .bind(id.get())
            .fetch_one(&self.pool)
            .await?;
        Ok(count != 0)
    }

    async fn transition_conflict(
        &self,
        id: JobId,
        expected: JobState,
    ) -> Result<StoreError, StoreError> {
        let actual = sqlx::query_scalar::<_, String>("SELECT state FROM jobs WHERE id = ?")
            .bind(id.get())
            .fetch_optional(&self.pool)
            .await?;
        match actual {
            Some(actual) => Ok(StoreError::StateConflict {
                id: id.get(),
                expected,
                actual: parse_state(&actual)?,
            }),
            None => Ok(StoreError::NotFound { id: id.get() }),
        }
    }

    async fn interrupt_active_jobs(&self) -> Result<(), StoreError> {
        let now = timestamp_ms()?;
        sqlx::query(
            "UPDATE jobs SET state = 'interrupted', updated_at_ms = ? WHERE state IN (?, ?, ?, ?, ?)",
        )
        .bind(now)
        .bind(ACTIVE_STATES[0])
        .bind(ACTIVE_STATES[1])
        .bind(ACTIVE_STATES[2])
        .bind(ACTIVE_STATES[3])
        .bind(ACTIVE_STATES[4])
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn acquire_lease(database: &Path) -> Result<File, StoreError> {
    let database_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(database)?;
    let lease_path = lease_path(database_file)?;
    let lease = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lease_path)?;
    fs2::FileExt::try_lock_exclusive(&lease).map_err(|source| {
        if source.kind() == std::io::ErrorKind::WouldBlock {
            StoreError::AlreadyOpen {
                path: database.to_owned(),
            }
        } else {
            StoreError::Io(source)
        }
    })?;
    Ok(lease)
}

fn lease_path(database_file: File) -> Result<PathBuf, StoreError> {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    let identity = same_file::Handle::from_file(database_file)?;
    let mut hasher = DefaultHasher::new();
    identity.hash(&mut hasher);
    let directory = std::env::temp_dir().join("fixer-server-store-leases");
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join(format!("sqlite-{:016x}.lock", hasher.finish())))
}

fn decode_ingestion_rule(row: &SqliteRow) -> Result<IngestionRule, StoreError> {
    let id =
        IngestionRuleId::from_database(row.try_get("id")?).map_err(corrupt_ingestion_record)?;
    let name: String = row.try_get("name")?;
    let source_root_id: String = row.try_get("source_root_id")?;
    let source_relative_path: String = row.try_get("source_relative_path")?;
    let destination_root_id: String = row.try_get("destination_root_id")?;
    let destination_relative_path: String = row.try_get("destination_relative_path")?;
    let media_kind_mode: String = row.try_get("media_kind_mode")?;
    let fixed_media_kind: Option<String> = row.try_get("fixed_media_kind")?;
    let placement: String = row.try_get("placement")?;
    let path_template_override: Option<String> = row.try_get("path_template_override")?;
    let enabled: i64 = row.try_get("enabled")?;
    let last_error: Option<String> = row.try_get("last_error")?;
    let created_at_ms: i64 = row.try_get("created_at_ms")?;
    let updated_at_ms: i64 = row.try_get("updated_at_ms")?;
    validate_timestamps(created_at_ms, updated_at_ms)?;

    let source = RuleDirectory::new(source_root_id, source_relative_path)
        .map_err(corrupt_ingestion_record)?;
    let destination = RuleDirectory::new(destination_root_id, destination_relative_path)
        .map_err(corrupt_ingestion_record)?;
    let mode = MediaKindMode::from_storage(&media_kind_mode, fixed_media_kind.as_deref())
        .map_err(corrupt_ingestion_record)?;
    let placement = RulePlacement::from_storage(&placement).map_err(corrupt_ingestion_record)?;
    let enabled = match enabled {
        0 => false,
        1 => true,
        _ => {
            return Err(StoreError::CorruptRecord(
                "ingestion rule enabled flag must be zero or one".to_owned(),
            ));
        }
    };
    let mut input = IngestionRuleInput::new(name, source, destination, mode, placement)
        .map_err(corrupt_ingestion_record)?
        .with_enabled(enabled);
    if let Some(template) = path_template_override {
        input = input
            .with_path_template_override(template)
            .map_err(corrupt_ingestion_record)?;
    }
    if let Some(error) = last_error {
        input = input
            .with_last_error(error)
            .map_err(corrupt_ingestion_record)?;
    }

    Ok(IngestionRule::from_parts(IngestionRuleParts {
        id,
        input,
        created_at_ms,
        updated_at_ms,
    }))
}

fn decode_ingestion_source(row: &SqliteRow) -> Result<IngestionSource, StoreError> {
    let id =
        IngestionSourceId::from_database(row.try_get("id")?).map_err(corrupt_ingestion_record)?;
    let rule_id = IngestionRuleId::from_database(row.try_get("rule_id")?)
        .map_err(corrupt_ingestion_record)?;
    let relative_source_path: String = row.try_get("relative_source_path")?;
    let size_bytes: i64 = row.try_get("size_bytes")?;
    let modified_at_ms: i64 = row.try_get("modified_at_ms")?;
    let status: String = row.try_get("status")?;
    let job_id: Option<i64> = row.try_get("job_id")?;
    let created_at_ms: i64 = row.try_get("created_at_ms")?;
    let updated_at_ms: i64 = row.try_get("updated_at_ms")?;
    validate_timestamps(created_at_ms, updated_at_ms)?;

    let size_bytes = u64::try_from(size_bytes).map_err(|_| {
        StoreError::CorruptRecord("ingestion source size must not be negative".to_owned())
    })?;
    let fingerprint = SourceFingerprint::new(relative_source_path, size_bytes, modified_at_ms)
        .map_err(corrupt_ingestion_record)?;
    let status = RuleStatus::from_storage(&status).map_err(corrupt_ingestion_record)?;
    let job_id = job_id.map(JobId::from_database).transpose()?;
    Ok(IngestionSource::from_parts(IngestionSourceParts {
        id,
        rule_id,
        fingerprint,
        status,
        job_id,
        created_at_ms,
        updated_at_ms,
    }))
}

fn corrupt_ingestion_record(error: IngestionModelError) -> StoreError {
    StoreError::CorruptRecord(error.to_string())
}

fn decode_source_review(row: &SqliteRow) -> Result<IngestionSourceReview, StoreError> {
    let source_id =
        IngestionSourceId::from_database(row.try_get("id")?).map_err(corrupt_ingestion_record)?;
    let rule_id = IngestionRuleId::from_database(row.try_get("rule_id")?)
        .map_err(corrupt_ingestion_record)?;
    let relative_source_path = row.try_get("relative_source_path")?;
    let review_json = row
        .try_get::<Option<String>, _>("review_kinds_json")?
        .ok_or_else(|| {
            StoreError::CorruptRecord("review source has no media kind options".to_owned())
        })?;
    let media_kinds = serde_json::from_str(&review_json)?;
    Ok(IngestionSourceReview::new(
        source_id,
        rule_id,
        relative_source_path,
        media_kinds,
    ))
}

fn decode_record(row: &SqliteRow) -> Result<JobRecord, StoreError> {
    let id = JobId::from_database(row.try_get("id")?)?;
    let input_json: String = row.try_get("input_json")?;
    let state: String = row.try_get("state")?;
    let progress_json: Option<String> = row.try_get("progress_json")?;
    let review_json: Option<String> = row.try_get("review_json")?;
    let review_decision_json: Option<String> = row.try_get("review_decision_json")?;
    let plan_json: Option<String> = row.try_get("plan_json")?;
    let execution_json: Option<String> = row.try_get("execution_json")?;
    let created_at_ms: i64 = row.try_get("created_at_ms")?;
    let updated_at_ms: i64 = row.try_get("updated_at_ms")?;
    validate_timestamps(created_at_ms, updated_at_ms)?;

    Ok(JobRecord::from_parts(JobRecordParts {
        id,
        input: serde_json::from_str(&input_json)?,
        state: parse_state(&state)?,
        progress: deserialize_optional(progress_json)?,
        review: deserialize_optional(review_json)?,
        review_decision: deserialize_optional(review_decision_json)?,
        plan: deserialize_optional(plan_json)?,
        execution: deserialize_optional(execution_json)?,
        created_at_ms,
        updated_at_ms,
    }))
}

fn validate_timestamps(created_at_ms: i64, updated_at_ms: i64) -> Result<(), StoreError> {
    if created_at_ms < 0 || updated_at_ms < created_at_ms {
        return Err(StoreError::CorruptRecord(format!(
            "invalid timestamps: created_at_ms={created_at_ms}, updated_at_ms={updated_at_ms}"
        )));
    }
    Ok(())
}

fn parse_state(value: &str) -> Result<JobState, StoreError> {
    JobState::from_str(value).map_err(|error| StoreError::CorruptRecord(error.to_string()))
}

fn serialize_optional<T: serde::Serialize>(
    value: Option<&T>,
) -> Result<Option<String>, StoreError> {
    value
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn deserialize_optional<T: serde::de::DeserializeOwned>(
    value: Option<String>,
) -> Result<Option<T>, StoreError> {
    value
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(Into::into)
}

fn timestamp_ms() -> Result<i64, StoreError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::ClockBeforeEpoch)?
        .as_millis();
    i64::try_from(millis).map_err(|_| StoreError::TimestampOverflow)
}

#[cfg(test)]
mod tests {
    use super::{parse_state, validate_timestamps};
    use crate::store::{JobId, StoreError};

    #[test]
    fn corrupt_ids_timestamps_and_states_are_rejected() {
        for value in [i64::MIN, -1, 0] {
            assert!(matches!(
                JobId::from_database(value),
                Err(StoreError::CorruptRecord(_))
            ));
        }
        assert!(JobId::from_database(1).is_ok());

        assert!(matches!(
            validate_timestamps(-1, 0),
            Err(StoreError::CorruptRecord(_))
        ));
        assert!(matches!(
            validate_timestamps(2, 1),
            Err(StoreError::CorruptRecord(_))
        ));
        assert!(validate_timestamps(1, 1).is_ok());
        assert!(matches!(
            parse_state("unknown"),
            Err(StoreError::CorruptRecord(_))
        ));
    }
}
