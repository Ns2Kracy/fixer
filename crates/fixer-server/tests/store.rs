use std::{collections::BTreeMap, str::FromStr};

use fixer_server::{
    ingestion::model::{
        IngestionRuleInput, MediaKindMode, RuleDirectory, RulePlacement, RuleStatus,
        SourceFingerprint, SourceReservation,
    },
    jobs::model::{
        ExecutionSummary, JobInputDto, JobMediaKind, JobState, PlanSummary, ProgressSummary,
        ReviewDecisionDto, ReviewSummary,
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
#[allow(
    clippy::too_many_lines,
    reason = "the reservation test keeps one complete concurrency and restart scenario together"
)]
async fn execution_reservation_is_atomic_and_idempotent_per_job() {
    let (root, store) = store().await;
    let job = store
        .create_job(JobInputDto::new(JobMediaKind::Movie, "/media/a.mkv", true))
        .await
        .unwrap();
    let job = store
        .transition(
            job.id(),
            JobState::Queued,
            JobState::Scanning,
            JobUpdate::default(),
        )
        .await
        .unwrap();
    let job = store
        .transition(
            job.id(),
            JobState::Scanning,
            JobState::Searching,
            JobUpdate::default(),
        )
        .await
        .unwrap();
    let job = store
        .transition(
            job.id(),
            JobState::Searching,
            JobState::Resolving,
            JobUpdate::default(),
        )
        .await
        .unwrap();
    let job = store
        .transition(
            job.id(),
            JobState::Resolving,
            JobState::AwaitingConfirmation,
            JobUpdate::default(),
        )
        .await
        .unwrap();
    let job = store
        .transition(
            job.id(),
            JobState::AwaitingConfirmation,
            JobState::Planning,
            JobUpdate::default()
                .with_review_decision(ReviewDecisionDto::new(0, vec![]))
                .with_plan(PlanSummary::new(1, true)),
        )
        .await
        .unwrap();

    assert!(matches!(
        store
            .transition(
                job.id(),
                JobState::Planning,
                JobState::Writing,
                JobUpdate::default(),
            )
            .await,
        Err(StoreError::ExecutionReservationRequired { .. })
    ));

    let first = store.clone();
    let second = store.clone();
    let id = job.id();
    let (left, right) = tokio::join!(
        first.reserve_execution(id, "request-a", "approved-v1"),
        second.reserve_execution(id, "request-a", "approved-v1")
    );
    let reservations = [left.unwrap(), right.unwrap()];
    assert_eq!(
        reservations
            .iter()
            .filter(|result| matches!(
                result,
                fixer_server::store::ExecutionReservation::Reserved(_)
            ))
            .count(),
        1
    );
    assert_eq!(
        reservations
            .iter()
            .filter(|result| matches!(
                result,
                fixer_server::store::ExecutionReservation::Existing(_)
            ))
            .count(),
        1
    );
    assert!(
        reservations
            .iter()
            .all(|result| result.job().state() == JobState::Writing)
    );

    let replay = store
        .reserve_execution(id, "request-a", "approved-v1")
        .await
        .unwrap();
    assert!(matches!(
        replay,
        fixer_server::store::ExecutionReservation::Existing(_)
    ));
    assert!(matches!(
        store
            .reserve_execution(id, "request-b", "approved-v1")
            .await,
        Err(StoreError::IdempotencyConflict { .. })
    ));

    drop(first);
    drop(second);
    drop(store);
    let database = root.path().join("jobs.sqlite3");
    let reopened = SqliteJobStore::open(database).await.unwrap();
    let interrupted = reopened.get_job(id).await.unwrap();
    assert_eq!(interrupted.state(), JobState::Interrupted);
    assert!(matches!(
        reopened
            .reserve_execution(id, "request-a", "approved-v1")
            .await
            .unwrap(),
        fixer_server::store::ExecutionReservation::Existing(job)
            if job.state() == JobState::Interrupted
    ));
    assert!(matches!(
        reopened
            .reserve_execution(id, "request-b", "approved-v1")
            .await,
        Err(StoreError::IdempotencyConflict { .. })
    ));
    assert!(matches!(
        reopened
            .transition(
                id,
                JobState::Interrupted,
                JobState::Queued,
                JobUpdate::default(),
            )
            .await,
        Err(StoreError::ReservedExecutionRetry { .. })
    ));
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
    let outcomes: [_; 2] = tokio::join!(
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
    )
    .into();
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
        let Err(error) = SqliteJobStore::open(&alias).await else {
            panic!("second store unexpectedly acquired the database lease through {alias:?}")
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
    let Err(error) = SqliteJobStore::open(&incompatible).await else {
        panic!("incompatible schema unexpectedly passed migration")
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
            "updated_at_ms",
            "review_decision_json"
        ]
    );
    assert!(columns.iter().all(|column| {
        !column.contains("secret")
            && !column.contains("token")
            && !column.contains("binary")
            && !column.contains("snapshot")
    }));
    let execution_columns = sqlx::query("PRAGMA table_info(job_executions)")
        .fetch_all(&pool)
        .await
        .unwrap()
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    assert_eq!(
        execution_columns,
        [
            "job_id",
            "idempotency_key",
            "request_fingerprint",
            "created_at_ms"
        ]
    );
    let migration_two: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 2")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(migration_two, 1);

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

fn ingestion_rule_input(mode: MediaKindMode, placement: RulePlacement) -> IngestionRuleInput {
    IngestionRuleInput::new(
        "Incoming media",
        RuleDirectory::new("root-source", "incoming").unwrap(),
        RuleDirectory::new("root-destination", "library").unwrap(),
        mode,
        placement,
    )
    .unwrap()
}

#[tokio::test]
async fn ingestion_rule_crud_preserves_required_placement_modes_and_optional_fields() {
    let (_root, store) = store().await;
    let created = store
        .create_ingestion_rule(ingestion_rule_input(
            MediaKindMode::Auto,
            RulePlacement::Move,
        ))
        .await
        .unwrap();

    assert_eq!(created.name(), "Incoming media");
    assert_eq!(created.source().root_id(), "root-source");
    assert_eq!(created.source().relative_path(), "incoming");
    assert_eq!(created.destination().root_id(), "root-destination");
    assert_eq!(created.destination().relative_path(), "library");
    assert_eq!(created.media_kind_mode(), MediaKindMode::Auto);
    assert_eq!(created.placement(), RulePlacement::Move);
    assert_eq!(created.path_template_override(), None);
    assert!(created.enabled());
    assert_eq!(created.last_error(), None);
    assert!(created.id().get() > 0);
    assert_eq!(created.created_at_ms(), created.updated_at_ms());

    let replacement = ingestion_rule_input(
        MediaKindMode::Fixed(JobMediaKind::Movie),
        RulePlacement::Hardlink,
    )
    .with_path_template_override("{title} ({year})/{title}")
    .unwrap()
    .with_enabled(false)
    .with_last_error("source root is unavailable")
    .unwrap();
    let updated = store
        .update_ingestion_rule(created.id(), replacement)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        updated.media_kind_mode(),
        MediaKindMode::Fixed(JobMediaKind::Movie)
    );
    assert_eq!(updated.placement(), RulePlacement::Hardlink);
    assert_eq!(
        updated.path_template_override(),
        Some("{title} ({year})/{title}")
    );
    assert!(!updated.enabled());
    assert_eq!(updated.last_error(), Some("source root is unavailable"));
    assert!(updated.updated_at_ms() >= updated.created_at_ms());
    assert_eq!(
        store.get_ingestion_rule(created.id()).await.unwrap(),
        Some(updated.clone())
    );
    assert_eq!(store.list_ingestion_rules(10).await.unwrap(), vec![updated]);
    assert!(matches!(
        store.list_ingestion_rules(101).await,
        Err(StoreError::CorruptRecord(_))
    ));

    assert!(store.delete_ingestion_rule(created.id()).await.unwrap());
    assert!(
        store
            .get_ingestion_rule(created.id())
            .await
            .unwrap()
            .is_none()
    );
    assert!(!store.delete_ingestion_rule(created.id()).await.unwrap());
}

#[tokio::test]
async fn ingestion_rule_creation_is_bounded_to_list_capacity() {
    let (_root, store) = store().await;
    for _ in 0..100 {
        store
            .create_ingestion_rule(ingestion_rule_input(
                MediaKindMode::Auto,
                RulePlacement::Copy,
            ))
            .await
            .unwrap();
    }

    assert!(matches!(
        store
            .create_ingestion_rule(ingestion_rule_input(
                MediaKindMode::Auto,
                RulePlacement::Copy,
            ))
            .await,
        Err(StoreError::IngestionRuleLimit { limit: 100 })
    ));
    assert_eq!(store.list_ingestion_rules(100).await.unwrap().len(), 100);
}

#[tokio::test]
async fn ingestion_job_and_source_association_commit_atomically() {
    let (_root, store) = store().await;
    let rule = store
        .create_ingestion_rule(ingestion_rule_input(
            MediaKindMode::Fixed(JobMediaKind::Movie),
            RulePlacement::Copy,
        ))
        .await
        .unwrap();
    let fingerprint = SourceFingerprint::new("Arrival.mkv", 5, 10).unwrap();
    let source = store
        .reserve_source(rule.id(), fingerprint.clone())
        .await
        .unwrap();
    let job = store
        .create_job_for_source(
            source.source().id(),
            JobInputDto::new(JobMediaKind::Movie, "/media/Arrival.mkv", true),
        )
        .await
        .unwrap();
    let associated = store.reserve_source(rule.id(), fingerprint).await.unwrap();
    assert_eq!(associated.source().job_id(), Some(job.id()));

    assert!(matches!(
        store
            .create_job_for_source(
                source.source().id(),
                JobInputDto::new(JobMediaKind::Movie, "/media/Arrival.mkv", true),
            )
            .await,
        Err(StoreError::IngestionSourceJobConflict { .. })
    ));
    assert_eq!(store.list_jobs(10, None).await.unwrap().len(), 1);
}

#[tokio::test]
async fn ingestion_source_fingerprint_reservation_and_job_association_are_idempotent() {
    let (_root, store) = store().await;
    let rule = store
        .create_ingestion_rule(ingestion_rule_input(
            MediaKindMode::Fixed(JobMediaKind::Television),
            RulePlacement::Symlink,
        ))
        .await
        .unwrap();
    let fingerprint = SourceFingerprint::new("Show/Season 01", 1_024, 1_725_000_000_000).unwrap();

    let first = store
        .reserve_source(rule.id(), fingerprint.clone())
        .await
        .unwrap();
    assert!(first.is_reserved());
    assert_eq!(first.source().rule_id(), rule.id());
    assert_eq!(first.source().fingerprint(), &fingerprint);
    assert_eq!(first.source().status(), RuleStatus::Processing);
    assert_eq!(first.source().job_id(), None);

    let duplicate = store.reserve_source(rule.id(), fingerprint).await.unwrap();
    assert!(!duplicate.is_reserved());
    assert_eq!(duplicate.source(), first.source());

    let job = store
        .create_job(JobInputDto::new(
            JobMediaKind::Television,
            "/media/Show/Season 01",
            false,
        ))
        .await
        .unwrap();
    let associated = store
        .associate_source_job(first.source().id(), job.id())
        .await
        .unwrap();
    assert_eq!(associated.job_id(), Some(job.id()));
    assert_eq!(
        store
            .associate_source_job(first.source().id(), job.id())
            .await
            .unwrap(),
        associated
    );
    let reviewed = store
        .update_source_status(first.source().id(), RuleStatus::NeedsReview)
        .await
        .unwrap();
    assert_eq!(reviewed.status(), RuleStatus::NeedsReview);
    assert_eq!(reviewed.job_id(), Some(job.id()));
}

#[tokio::test]
async fn ingestion_rules_and_observations_survive_reopen() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("jobs.sqlite3");
    let store = SqliteJobStore::open(&database).await.unwrap();
    let rule = store
        .create_ingestion_rule(
            ingestion_rule_input(MediaKindMode::Auto, RulePlacement::Reflink)
                .with_path_template_override("{kind}/{title}")
                .unwrap(),
        )
        .await
        .unwrap();
    let fingerprint = SourceFingerprint::new("Arrival.mkv", 2_048, 1_725_000_000_123).unwrap();
    let reserved = store
        .reserve_source(rule.id(), fingerprint.clone())
        .await
        .unwrap();
    assert!(matches!(reserved, SourceReservation::Reserved(_)));
    let source = reserved.source().clone();
    drop(store);

    let reopened = SqliteJobStore::open(&database).await.unwrap();
    assert_eq!(
        reopened.get_ingestion_rule(rule.id()).await.unwrap(),
        Some(rule)
    );
    let existing = reopened
        .reserve_source(source.rule_id(), fingerprint)
        .await
        .unwrap();
    assert!(matches!(existing, SourceReservation::Existing(_)));
    assert_eq!(existing.source(), &source);
}

#[test]
fn ingestion_model_rejects_unbounded_persistence_inputs() {
    assert!(RuleDirectory::new("", "incoming").is_err());
    assert!(RuleDirectory::new("root", "x".repeat(4_097)).is_err());
    assert!(SourceFingerprint::new("", 1, 1).is_err());
    assert!(SourceFingerprint::new("movie.mkv", u64::MAX, 1).is_err());
    assert!(SourceFingerprint::new("movie.mkv", 1, -1).is_err());
    assert!(
        IngestionRuleInput::new(
            "x".repeat(101),
            RuleDirectory::new("source", "").unwrap(),
            RuleDirectory::new("destination", "").unwrap(),
            MediaKindMode::Auto,
            RulePlacement::Copy,
        )
        .is_err()
    );
    assert!(
        ingestion_rule_input(MediaKindMode::Auto, RulePlacement::Copy)
            .with_path_template_override("x".repeat(4_097))
            .is_err()
    );
}
