use std::{collections::BTreeMap, str::FromStr};

use fixer_server::{
    jobs::model::{
        ExecutionSummary, JobInputDto, JobMediaKind, JobState, PlanSummary, ProgressSummary,
        ReviewSummary,
    },
    store::{JobUpdate, SqliteJobStore, StoreError},
};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};

async fn store() -> (tempfile::TempDir, SqliteJobStore) {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("jobs.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();
    (root, store)
}

async fn raw_pool(path: &std::path::Path) -> SqlitePool {
    let options = SqliteConnectOptions::from_str(path.to_str().unwrap())
        .unwrap()
        .create_if_missing(true);
    SqlitePool::connect_with(options).await.unwrap()
}

#[tokio::test]
async fn migration_and_job_round_trip_persist_versioned_dtos_and_timestamps() {
    let (_root, store) = store().await;
    let input = JobInputDto::new(JobMediaKind::Movie, "/media/Arrival.mkv", false);
    let created = store.create_job(input.clone()).await.unwrap();

    assert!(created.id().get() > 0);
    assert_eq!(created.input(), &input);
    assert_eq!(created.state(), JobState::Queued);
    assert_eq!(created.created_at_ms(), created.updated_at_ms());
    assert!(created.progress().is_none());

    let scanning = store
        .transition(
            created.id(),
            JobState::Queued,
            JobState::Scanning,
            JobUpdate::default().with_progress(ProgressSummary::new("scanning", 1, Some(1))),
        )
        .await
        .unwrap();
    assert_eq!(scanning.state(), JobState::Scanning);
    assert!(scanning.updated_at_ms() >= scanning.created_at_ms());
    assert!(scanning.progress().is_some());

    let searching = store
        .transition(
            created.id(),
            JobState::Scanning,
            JobState::Searching,
            JobUpdate::default()
                .with_review(ReviewSummary::new(3, 1))
                .with_plan(PlanSummary::new(4, true))
                .with_execution(ExecutionSummary::new(3, 1)),
        )
        .await
        .unwrap();
    let loaded = store.get_job(created.id()).await.unwrap();
    assert_eq!(loaded, searching);
    assert_eq!(loaded.review(), Some(&ReviewSummary::new(3, 1)));
    assert_eq!(loaded.plan(), Some(&PlanSummary::new(4, true)));
    assert_eq!(loaded.execution(), Some(&ExecutionSummary::new(3, 1)));
}

#[tokio::test]
async fn concurrent_transitions_compare_and_set_and_return_their_own_row() {
    let (_root, store) = store().await;
    let job = store
        .create_job(JobInputDto::new(JobMediaKind::Book, "/books/a.epub", false))
        .await
        .unwrap();

    let invalid = store
        .transition(
            job.id(),
            JobState::Queued,
            JobState::Completed,
            JobUpdate::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(invalid, StoreError::InvalidTransition { .. }));

    let first = store.clone();
    let second = store.clone();
    let id = job.id();
    let (left, right) = tokio::join!(
        first.transition(
            id,
            JobState::Queued,
            JobState::Scanning,
            JobUpdate::default().with_progress(ProgressSummary::new("left", 1, Some(1))),
        ),
        second.transition(
            id,
            JobState::Queued,
            JobState::Scanning,
            JobUpdate::default().with_progress(ProgressSummary::new("right", 1, Some(1))),
        )
    );

    let outcomes = [left, right];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| matches!(result, Err(StoreError::StateConflict { .. })))
            .count(),
        1
    );
    assert_eq!(
        outcomes.into_iter().find_map(Result::ok).unwrap().state(),
        JobState::Scanning
    );
}

#[tokio::test]
async fn startup_recovery_interrupts_all_active_states_and_preserves_other_states() {
    use JobState::{
        AwaitingConfirmation, Cancelled, Completed, Failed, Interrupted, Planning, Queued,
        Resolving, Scanning, Searching, Writing,
    };

    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("jobs.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();
    let mut jobs = BTreeMap::new();
    for state in JobState::ALL {
        let record = store
            .create_job(JobInputDto::new(
                JobMediaKind::Movie,
                format!("/media/{state}"),
                false,
            ))
            .await
            .unwrap();
        jobs.insert(state.to_string(), record.id());
    }
    drop(store);

    let pool = raw_pool(&database).await;
    for state in JobState::ALL {
        sqlx::query("UPDATE jobs SET state = ? WHERE id = ?")
            .bind(state.to_string())
            .bind(jobs[&state.to_string()].get())
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;

    let reopened = SqliteJobStore::open(&database).await.unwrap();
    let expected = [
        (Queued, Queued),
        (Scanning, Interrupted),
        (Searching, Interrupted),
        (Resolving, Interrupted),
        (AwaitingConfirmation, AwaitingConfirmation),
        (Planning, Interrupted),
        (Writing, Interrupted),
        (Completed, Completed),
        (Failed, Failed),
        (Cancelled, Cancelled),
        (Interrupted, Interrupted),
    ];
    for (before, after) in expected {
        assert_eq!(
            reopened
                .get_job(jobs[&before.to_string()])
                .await
                .unwrap()
                .state(),
            after,
            "recovery decision for {before}"
        );
    }
}

#[tokio::test]
async fn a_second_open_is_rejected_without_interrupting_live_jobs() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("jobs.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();
    let job = store
        .create_job(JobInputDto::new(JobMediaKind::Music, "/music/live", false))
        .await
        .unwrap();
    store
        .transition(
            job.id(),
            JobState::Queued,
            JobState::Scanning,
            JobUpdate::default(),
        )
        .await
        .unwrap();

    let mut aliases = vec![database.clone()];
    #[cfg(unix)]
    {
        let symlink = root.path().join("jobs-symlink.sqlite3");
        let hardlink = root.path().join("jobs-hardlink.sqlite3");
        std::os::unix::fs::symlink(&database, &symlink).unwrap();
        std::fs::hard_link(&database, &hardlink).unwrap();
        aliases.extend([symlink, hardlink]);
    }

    for alias in aliases {
        let error = match SqliteJobStore::open(&alias).await {
            Ok(_) => {
                panic!("second store unexpectedly acquired the database lease through {alias:?}")
            }
            Err(error) => error,
        };
        assert!(matches!(error, StoreError::AlreadyOpen { .. }));
    }
    assert_eq!(
        store.get_job(job.id()).await.unwrap().state(),
        JobState::Scanning
    );

    drop(store);
    let reopened = SqliteJobStore::open(&database).await.unwrap();
    assert_eq!(
        reopened.get_job(job.id()).await.unwrap().state(),
        JobState::Interrupted
    );
}

#[tokio::test]
async fn tracked_migration_reruns_and_rejects_an_incompatible_existing_schema() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("jobs.sqlite3");
    drop(SqliteJobStore::open(&database).await.unwrap());
    drop(SqliteJobStore::open(&database).await.unwrap());

    let pool = raw_pool(&database).await;
    let applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(applied, 1);
    pool.close().await;

    let incompatible = root.path().join("incompatible.sqlite3");
    let pool = raw_pool(&incompatible).await;
    sqlx::query("CREATE TABLE jobs (id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let error = match SqliteJobStore::open(&incompatible).await {
        Ok(_) => panic!("incompatible schema unexpectedly passed migration"),
        Err(error) => error,
    };
    assert!(matches!(error, StoreError::Migration(_)));

    let pool = raw_pool(&incompatible).await;
    let applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let partial_index: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'index' AND name = 'jobs_state_id_idx'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(applied, 0);
    assert_eq!(partial_index, 0);
}

#[tokio::test]
async fn schema_has_only_bounded_dto_state_and_timestamp_columns() {
    let (root, store) = store().await;
    drop(store);
    let pool = raw_pool(&root.path().join("jobs.sqlite3")).await;
    let rows = sqlx::query("PRAGMA table_info(jobs)")
        .fetch_all(&pool)
        .await
        .unwrap();
    let columns = rows
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    assert_eq!(
        columns,
        [
            "id",
            "input_json",
            "state",
            "progress_json",
            "review_json",
            "plan_json",
            "execution_json",
            "created_at_ms",
            "updated_at_ms"
        ]
    );
    assert!(columns.iter().all(|column| {
        !column.contains("secret")
            && !column.contains("token")
            && !column.contains("binary")
            && !column.contains("snapshot")
    }));
    let index_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'index' AND name = 'jobs_state_id_idx'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(index_exists, 1);

    for statement in [
        "INSERT INTO jobs (id, input_json, state, created_at_ms, updated_at_ms) VALUES (-1, '{}', 'queued', 0, 0)",
        "INSERT INTO jobs (input_json, state, created_at_ms, updated_at_ms) VALUES ('not-json', 'queued', 0, 0)",
        "INSERT INTO jobs (input_json, state, created_at_ms, updated_at_ms) VALUES ('{}', 'unknown', 0, 0)",
        "INSERT INTO jobs (input_json, state, created_at_ms, updated_at_ms) VALUES ('{}', 'queued', -1, 0)",
        "INSERT INTO jobs (input_json, state, created_at_ms, updated_at_ms) VALUES ('{}', 'queued', 2, 1)",
    ] {
        assert!(
            sqlx::query(statement).execute(&pool).await.is_err(),
            "constraint accepted: {statement}"
        );
    }
}

#[tokio::test]
async fn unsupported_versions_in_every_persisted_dto_column_are_rejected_on_read() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("jobs.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();
    let mut jobs = Vec::new();
    for index in 0..5 {
        jobs.push(
            store
                .create_job(JobInputDto::new(
                    JobMediaKind::Anime,
                    format!("/anime/{index}"),
                    false,
                ))
                .await
                .unwrap()
                .id(),
        );
    }
    drop(store);

    let pool = raw_pool(&database).await;
    let corruptions = [
        (
            "input_json",
            r#"{"schema_version":2,"media_kind":"anime","input_path":"/anime/0","apply":false}"#,
        ),
        (
            "progress_json",
            r#"{"schema_version":2,"stage":"scanning","completed_items":0,"total_items":null}"#,
        ),
        (
            "review_json",
            r#"{"schema_version":2,"candidate_count":0,"conflict_count":0}"#,
        ),
        (
            "plan_json",
            r#"{"schema_version":2,"operation_count":0,"requires_confirmation":false}"#,
        ),
        (
            "execution_json",
            r#"{"schema_version":2,"completed_operations":0,"failed_operations":0}"#,
        ),
    ];
    for ((column, value), id) in corruptions.into_iter().zip(&jobs) {
        let statement = format!("UPDATE jobs SET {column} = ? WHERE id = ?");
        sqlx::query(&statement)
            .bind(value)
            .bind(id.get())
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;

    let reopened = SqliteJobStore::open(&database).await.unwrap();
    for id in jobs {
        assert!(matches!(
            reopened.get_job(id).await.unwrap_err(),
            StoreError::Json(_)
        ));
    }
}
