use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
};

use crate::{
    jobs::model::{JobInputDto, JobState, ProgressSummary},
    store::{JobId, JobRecord, JobRecordParts, JobUpdate, StoreError},
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
const ACTIVE_STATES: [&str; 5] = ["scanning", "searching", "resolving", "planning", "writing"];
const RECORD_COLUMNS: &str = "id, input_json, state, progress_json, review_json, plan_json, execution_json, created_at_ms, updated_at_ms";

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
            .foreign_keys(true);
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

        let progress = serialize_optional(update.progress.as_ref())?;
        let review = serialize_optional(update.review.as_ref())?;
        let plan = serialize_optional(update.plan.as_ref())?;
        let execution = serialize_optional(update.execution.as_ref())?;
        let updated_at_ms = timestamp_ms()?;
        let sql = format!(
            "UPDATE jobs SET state = ?, progress_json = COALESCE(?, progress_json), review_json = COALESCE(?, review_json), plan_json = COALESCE(?, plan_json), execution_json = COALESCE(?, execution_json), updated_at_ms = ? WHERE id = ? AND state = ? RETURNING {RECORD_COLUMNS}"
        );
        let row = sqlx::query(&sql)
            .bind(next.to_string())
            .bind(progress)
            .bind(review)
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

fn decode_record(row: &SqliteRow) -> Result<JobRecord, StoreError> {
    let id = JobId::from_database(row.try_get("id")?)?;
    let input_json: String = row.try_get("input_json")?;
    let state: String = row.try_get("state")?;
    let progress_json: Option<String> = row.try_get("progress_json")?;
    let review_json: Option<String> = row.try_get("review_json")?;
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
