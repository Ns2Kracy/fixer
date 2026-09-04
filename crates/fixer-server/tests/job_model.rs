use std::fmt::Debug;

use fixer_server::{
    ingestion::model::RulePlacement,
    jobs::model::{
        ExecutionFailureSummary, ExecutionSummary, JobInputDto, JobMediaKind, JobOrganizationDto,
        JobState, PlanSummary, ProgressSummary, ReviewDecisionDto, ReviewSummary,
    },
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn assert_round_trip<T>(value: &T)
where
    T: Debug + PartialEq + Serialize + DeserializeOwned,
{
    let json = serde_json::to_string(value).unwrap();
    let decoded = serde_json::from_str::<T>(&json).unwrap();
    assert_eq!(&decoded, value);
}

fn assert_unsupported_version<T>(record: Value)
where
    T: DeserializeOwned,
{
    assert!(serde_json::from_value::<T>(record).is_err());
}

#[test]
fn job_input_and_summaries_are_stable_server_owned_dtos() {
    let input = JobInputDto::new(JobMediaKind::Movie, "/media/Arrival.mkv", false);
    assert_eq!(
        serde_json::to_value(&input).unwrap(),
        json!({
            "schema_version": 1,
            "media_kind": "movie",
            "input_path": "/media/Arrival.mkv",
            "apply": false
        })
    );
    assert_round_trip(&input);

    let progress = ProgressSummary::new("searching", 2, Some(6));
    assert_eq!(
        serde_json::to_value(&progress).unwrap(),
        json!({"schema_version": 1, "stage": "searching", "completed_items": 2, "total_items": 6})
    );
    assert_round_trip(&progress);

    let review = ReviewSummary::new(3, 2);
    assert_eq!(
        serde_json::to_value(review).unwrap(),
        json!({"schema_version": 1, "candidate_count": 3, "conflict_count": 2})
    );
    assert_round_trip(&review);

    let decision = ReviewDecisionDto::new(2, vec![0, 1]);
    assert_eq!(
        serde_json::to_value(&decision).unwrap(),
        json!({"schema_version": 1, "candidate_index": 2, "accepted_conflict_indexes": [0, 1]})
    );
    assert_round_trip(&decision);

    let plan = PlanSummary::new(5, true);
    assert_eq!(
        serde_json::to_value(&plan).unwrap(),
        json!({"schema_version": 1, "operation_count": 5, "requires_confirmation": true})
    );
    assert_round_trip(&plan);

    let execution = ExecutionSummary::new(4, 1);
    assert_eq!(
        serde_json::to_value(&execution).unwrap(),
        json!({"schema_version": 1, "completed_operations": 4, "failed_operations": 1})
    );
    assert_round_trip(&execution);

    let failed = ExecutionSummary::new(0, 1).with_failure(ExecutionFailureSummary::new(
        Some(2),
        "target_exists",
        "An output target already exists",
    ));
    assert_eq!(
        serde_json::to_value(&failed).unwrap(),
        json!({
            "schema_version": 1,
            "completed_operations": 0,
            "failed_operations": 1,
            "failure": {
                "schema_version": 1,
                "operation_index": 2,
                "code": "target_exists",
                "message": "An output target already exists"
            }
        })
    );
    assert_round_trip(&failed);
}

#[test]
fn organization_snapshot_is_optional_and_backward_compatible() {
    let legacy = json!({
        "schema_version": 1,
        "media_kind": "movie",
        "input_path": "/media/Arrival.mkv",
        "apply": false
    });
    let decoded: JobInputDto = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(decoded.organization(), None);
    assert_eq!(serde_json::to_value(decoded).unwrap(), legacy);

    let organization = JobOrganizationDto {
        destination_path: "/library".to_owned(),
        placement: RulePlacement::Hardlink,
        path_template: Some("Classics/{{ title | sanitize }}".to_owned()),
        origin_rule_id: Some(42),
        auto_execute: true,
    };
    let input = JobInputDto::new(JobMediaKind::Movie, "/media/Arrival.mkv", false)
        .with_organization(organization.clone());
    assert_eq!(input.organization(), Some(&organization));
    assert_eq!(
        serde_json::to_value(&input).unwrap(),
        json!({
            "schema_version": 1,
            "media_kind": "movie",
            "input_path": "/media/Arrival.mkv",
            "apply": false,
            "organization": {
                "destination_path": "/library",
                "placement": "hardlink",
                "path_template": "Classics/{{ title | sanitize }}",
                "origin_rule_id": 42,
                "auto_execute": true
            }
        })
    );
    assert_round_trip(&input);
}

#[test]
fn unsupported_persisted_schema_versions_and_states_are_rejected() {
    assert_unsupported_version::<JobInputDto>(json!({
        "schema_version": 2,
        "media_kind": "movie",
        "input_path": "/media/a.mkv",
        "apply": false
    }));
    assert_unsupported_version::<ProgressSummary>(json!({
        "schema_version": 2,
        "stage": "searching",
        "completed_items": 0,
        "total_items": null
    }));
    assert_unsupported_version::<ReviewSummary>(json!({
        "schema_version": 2,
        "candidate_count": 0,
        "conflict_count": 0
    }));
    assert_unsupported_version::<ReviewDecisionDto>(json!({
        "schema_version": 2,
        "candidate_index": 0,
        "accepted_conflict_indexes": []
    }));
    assert_unsupported_version::<PlanSummary>(json!({
        "schema_version": 2,
        "operation_count": 0,
        "requires_confirmation": false
    }));
    assert_unsupported_version::<ExecutionSummary>(json!({
        "schema_version": 2,
        "completed_operations": 0,
        "failed_operations": 0
    }));
    assert!(serde_json::from_str::<JobState>("\"unknown\"").is_err());
    assert!("unknown".parse::<JobState>().is_err());
}

#[test]
fn job_state_transitions_follow_the_persistent_worker_lifecycle() {
    use JobState::{
        AwaitingConfirmation, Cancelled, Completed, Failed, Interrupted, Planning, Queued,
        Resolving, Scanning, Searching, Writing,
    };

    let allowed = [
        (Queued, Scanning),
        (Queued, Cancelled),
        (Scanning, Searching),
        (Searching, Resolving),
        (Resolving, AwaitingConfirmation),
        (AwaitingConfirmation, Planning),
        (Planning, Writing),
        (Writing, Completed),
        (Scanning, Failed),
        (Searching, Failed),
        (Resolving, Failed),
        (AwaitingConfirmation, Failed),
        (Planning, Failed),
        (Writing, Failed),
        (Scanning, Cancelled),
        (Searching, Cancelled),
        (Resolving, Cancelled),
        (AwaitingConfirmation, Cancelled),
        (Planning, Cancelled),
        (Scanning, Interrupted),
        (Searching, Interrupted),
        (Resolving, Interrupted),
        (Planning, Interrupted),
        (Writing, Interrupted),
        (Interrupted, Queued),
    ];

    for from in JobState::ALL {
        for to in JobState::ALL {
            assert_eq!(
                from.can_transition_to(to),
                allowed.contains(&(from, to)),
                "unexpected transition decision for {from} -> {to}"
            );
        }
    }
}
