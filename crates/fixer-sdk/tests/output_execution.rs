use fixer_core::{
    OperationOutcome, OutputOperation, OutputOperationKind, OutputPlan, PlannedContent,
};
use fixer_sdk::output::{
    ExecutionError, ExecutionPolicy, OutputPlanExt, OverwritePolicy, PlacementMode, ReflinkPolicy,
    plan_media_placement,
};
use std::{fs, path::Path};

fn write_plan(root: &Path, target: &str, content: &[u8]) -> OutputPlan {
    let mut plan = OutputPlan::new(root);
    plan.push(OutputOperation::write_bytes(target, PlannedContent::new(content)).unwrap());
    plan
}

#[test]
fn dry_run_previews_without_touching_the_filesystem() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("movie/movie.json");
    let plan = write_plan(root.path(), "movie/movie.json", b"metadata");
    assert_eq!(plan.preview().unwrap().operations().len(), 1);
    let report = plan.execute(ExecutionPolicy::dry_run()).unwrap();
    assert_eq!(report.operations()[0].outcome(), OperationOutcome::DryRun);
    assert!(!target.exists());
}

#[test]
fn existing_targets_default_to_no_overwrite() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("movie.json"), "existing").unwrap();
    let prepared = write_plan(root.path(), "movie.json", b"replacement")
        .prepare()
        .unwrap();
    let failure = prepared.execute(ExecutionPolicy::default()).unwrap_err();
    assert!(matches!(
        failure.error(),
        ExecutionError::TargetExists { .. }
    ));
    assert_eq!(
        fs::read_to_string(root.path().join("movie.json")).unwrap(),
        "existing"
    );
}

#[test]
fn deserialized_traversal_is_rejected_again_at_the_sdk_boundary() {
    let root = tempfile::tempdir().unwrap();
    let value = serde_json::json!({
        "output_root": root.path(),
        "operations": [{"operation": "write_bytes", "target": "../escape", "content": {"bytes": [120]}}]
    });
    let plan: OutputPlan = serde_json::from_value(value).unwrap();
    assert!(matches!(
        plan.prepare().unwrap_err(),
        ExecutionError::UnsafeTarget { .. }
    ));
}

#[test]
fn deserialized_relative_move_source_is_rejected_without_moving_cwd_file() {
    let cwd = std::env::current_dir().unwrap();
    let sandbox = tempfile::Builder::new()
        .prefix("fixer-relative-move-")
        .tempdir_in(&cwd)
        .unwrap();
    let source = sandbox.path().join("source.mkv");
    fs::write(&source, b"cwd media").unwrap();
    let relative_source = source.strip_prefix(&cwd).unwrap();
    let output = sandbox.path().join("library");
    let value = serde_json::json!({
        "output_root": output,
        "operations": [{
            "operation": "move",
            "source": relative_source,
            "target": "movie.mkv"
        }]
    });
    let plan: OutputPlan = serde_json::from_value(value).unwrap();

    assert!(matches!(
        plan.clone().prepare().unwrap_err(),
        ExecutionError::InvalidPlan(_)
    ));
    let failure = plan.execute(ExecutionPolicy::default()).unwrap_err();
    assert!(matches!(failure.error(), ExecutionError::InvalidPlan(_)));
    assert_eq!(fs::read(&source).unwrap(), b"cwd media");
    assert!(!sandbox.path().join("library/movie.mkv").exists());
}

#[test]
fn changing_a_source_after_prepare_rejects_the_stale_plan() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mkv");
    fs::write(&source, "first").unwrap();
    let mut plan = OutputPlan::new(root.path().join("library"));
    plan.push(OutputOperation::copy(&source, "movie.mkv").unwrap());
    let prepared = plan.prepare().unwrap();
    fs::write(&source, "changed").unwrap();
    let failure = prepared.execute(ExecutionPolicy::default()).unwrap_err();
    assert!(matches!(failure.error(), ExecutionError::StalePlan { .. }));
    assert!(!root.path().join("library/movie.mkv").exists());
}

#[test]
fn move_publishes_destination_then_removes_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("incoming/movie.mkv");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, [0, 1, 2, 255]).unwrap();
    let output = root.path().join("library");
    let mut plan = OutputPlan::new(&output);
    plan.push(OutputOperation::move_file(&source, "Movie/movie.mkv").unwrap());

    let report = plan.execute(ExecutionPolicy::default()).unwrap();

    let operation = &report.operations()[0];
    assert_eq!(operation.outcome(), OperationOutcome::Succeeded);
    assert_eq!(operation.kind(), OutputOperationKind::Move);
    assert_eq!(operation.source(), Some(source.to_string_lossy().as_ref()));
    assert_eq!(
        operation.destination(),
        output
            .join("Movie/movie.mkv")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
    );
    assert!(operation.fingerprint().is_some());
    assert_eq!(
        fs::read(output.join("Movie/movie.mkv")).unwrap(),
        [0, 1, 2, 255]
    );
    assert!(!source.exists());
}

#[test]
fn move_collision_does_not_overwrite_and_preserves_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("incoming.mkv");
    let output = root.path().join("library");
    fs::create_dir_all(&output).unwrap();
    fs::write(&source, b"incoming").unwrap();
    fs::write(output.join("movie.mkv"), b"existing").unwrap();
    let mut plan = OutputPlan::new(&output);
    plan.push(OutputOperation::move_file(&source, "movie.mkv").unwrap());

    let failure = plan.execute(ExecutionPolicy::default()).unwrap_err();

    assert!(matches!(
        failure.error(),
        ExecutionError::TargetExists { .. }
    ));
    assert_eq!(fs::read(&source).unwrap(), b"incoming");
    assert_eq!(fs::read(output.join("movie.mkv")).unwrap(), b"existing");
    assert!(walkdir(&output).iter().all(|path| {
        !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".fixer-tmp")
    }));
}

#[test]
fn later_collision_is_detected_before_an_earlier_move() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("incoming.mkv");
    let output = root.path().join("library");
    fs::create_dir_all(&output).unwrap();
    fs::write(&source, b"incoming").unwrap();
    fs::write(output.join("movie.nfo"), b"existing").unwrap();
    let mut plan = OutputPlan::new(&output);
    plan.push(OutputOperation::move_file(&source, "movie.mkv").unwrap());
    plan.push(
        OutputOperation::write_bytes("movie.nfo", PlannedContent::new(b"replacement")).unwrap(),
    );

    let failure = plan.execute(ExecutionPolicy::default()).unwrap_err();

    assert!(matches!(
        failure.error(),
        ExecutionError::TargetExists { .. }
    ));
    assert_eq!(failure.report().operations()[0].operation_index(), 1);
    assert_eq!(fs::read(&source).unwrap(), b"incoming");
    assert!(!output.join("movie.mkv").exists());
    assert_eq!(fs::read(output.join("movie.nfo")).unwrap(), b"existing");
}

#[test]
fn move_deserialized_traversal_is_rejected_at_the_sdk_boundary() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mkv");
    fs::write(&source, b"media").unwrap();
    let value = serde_json::json!({
        "output_root": root.path(),
        "operations": [{
            "operation": "move",
            "source": source,
            "target": "../escape.mkv"
        }]
    });
    let plan: OutputPlan = serde_json::from_value(value).unwrap();

    assert!(matches!(
        plan.prepare().unwrap_err(),
        ExecutionError::UnsafeTarget { .. }
    ));
}

#[test]
fn move_missing_source_is_unavailable() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("missing.mkv");
    let mut plan = OutputPlan::new(root.path().join("library"));
    plan.push(OutputOperation::move_file(&source, "movie.mkv").unwrap());

    let failure = plan.execute(ExecutionPolicy::default()).unwrap_err();

    assert!(matches!(
        failure.error(),
        ExecutionError::SourceUnavailable { path } if path == &source
    ));
    assert!(!root.path().join("library/movie.mkv").exists());
}

#[test]
fn move_replace_policy_replaces_target_and_removes_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("incoming.mkv");
    let output = root.path().join("library");
    fs::create_dir_all(&output).unwrap();
    fs::write(&source, b"incoming").unwrap();
    fs::write(output.join("movie.mkv"), b"existing").unwrap();
    let mut plan = OutputPlan::new(&output);
    plan.push(OutputOperation::move_file(&source, "movie.mkv").unwrap());

    plan.execute(ExecutionPolicy::default().with_overwrite(OverwritePolicy::Replace))
        .unwrap();

    assert!(!source.exists());
    assert_eq!(fs::read(output.join("movie.mkv")).unwrap(), b"incoming");
}

#[test]
fn move_replace_with_identical_source_and_target_keeps_the_file() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("library");
    let target = output.join("movie.mkv");
    fs::create_dir_all(&output).unwrap();
    fs::write(&target, b"media").unwrap();
    let mut plan = OutputPlan::new(&output);
    plan.push(OutputOperation::move_file(&target, "movie.mkv").unwrap());

    plan.execute(ExecutionPolicy::default().with_overwrite(OverwritePolicy::Replace))
        .unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"media");
}

#[cfg(unix)]
#[test]
fn move_replace_removes_a_source_hardlinked_to_the_target() {
    use std::os::unix::fs::MetadataExt;

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("incoming.mkv");
    let output = root.path().join("library");
    let target = output.join("movie.mkv");
    fs::create_dir_all(&output).unwrap();
    fs::write(&source, b"shared media").unwrap();
    fs::hard_link(&source, &target).unwrap();
    let inode = fs::metadata(&source).unwrap().ino();
    let mut plan = OutputPlan::new(&output);
    plan.push(OutputOperation::move_file(&source, "movie.mkv").unwrap());

    plan.execute(ExecutionPolicy::default().with_overwrite(OverwritePolicy::Replace))
        .unwrap();

    assert!(!source.exists());
    assert_eq!(fs::read(&target).unwrap(), b"shared media");
    assert_eq!(fs::metadata(&target).unwrap().ino(), inode);
}

#[test]
fn writes_and_copies_publish_complete_files_and_leave_no_temps() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mkv");
    fs::write(&source, b"media").unwrap();
    let output = root.path().join("library");
    let mut plan = write_plan(&output, "movie/movie.json", b"metadata");
    plan.push(OutputOperation::copy(&source, "movie/movie.mkv").unwrap());
    let report = plan
        .prepare()
        .unwrap()
        .execute(ExecutionPolicy::default())
        .unwrap();
    assert_eq!(report.operations().len(), 2);
    assert_eq!(
        fs::read(output.join("movie/movie.json")).unwrap(),
        b"metadata"
    );
    assert_eq!(fs::read(output.join("movie/movie.mkv")).unwrap(), b"media");
    assert!(walkdir(&output).iter().all(|path| {
        !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".fixer-tmp")
    }));
}

#[test]
fn relative_and_absolute_symlink_modes_are_distinct() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mkv");
    fs::write(&source, "media").unwrap();
    for (name, mode, absolute) in [
        ("relative.mkv", PlacementMode::RelativeSymlink, false),
        ("absolute.mkv", PlacementMode::AbsoluteSymlink, true),
    ] {
        let output = root.path().join("library");
        let plan = plan_media_placement(&source, &output, name, mode).unwrap();
        plan.prepare()
            .unwrap()
            .execute(ExecutionPolicy::default())
            .unwrap();
        let link = fs::read_link(output.join(name)).unwrap();
        assert_eq!(link.is_absolute(), absolute);
        assert_eq!(fs::read_to_string(output.join(name)).unwrap(), "media");
    }
}

#[test]
fn hardlink_and_copy_modes_have_expected_identity() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mkv");
    fs::write(&source, "media").unwrap();
    let output = root.path().join("library");
    for (name, mode) in [
        ("hard.mkv", PlacementMode::Hardlink),
        ("copy.mkv", PlacementMode::Copy),
    ] {
        plan_media_placement(&source, &output, name, mode)
            .unwrap()
            .prepare()
            .unwrap()
            .execute(ExecutionPolicy::default())
            .unwrap();
    }
    assert_eq!(
        fs::read_to_string(output.join("hard.mkv")).unwrap(),
        "media"
    );
    assert_eq!(
        fs::read_to_string(output.join("copy.mkv")).unwrap(),
        "media"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&source).unwrap().ino(),
            fs::metadata(output.join("hard.mkv")).unwrap().ino()
        );
        assert_ne!(
            fs::metadata(&source).unwrap().ino(),
            fs::metadata(output.join("copy.mkv")).unwrap().ino()
        );
    }
}

#[test]
fn in_place_has_no_operation_and_move_is_not_a_mode() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("movie.mkv");
    fs::write(&source, "media").unwrap();
    let plan =
        plan_media_placement(&source, root.path(), "unused.mkv", PlacementMode::InPlace).unwrap();
    assert!(plan.operations().is_empty());
}

#[test]
fn reflink_required_reports_support_precisely_or_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mkv");
    fs::write(&source, "media").unwrap();
    let output = root.path().join("library");
    let plan = plan_media_placement(&source, &output, "clone.mkv", PlacementMode::Reflink).unwrap();
    let result = plan
        .prepare()
        .unwrap()
        .execute(ExecutionPolicy::default().with_reflink(ReflinkPolicy::Required));
    match result {
        Ok(report) => assert_eq!(
            report.operations()[0].outcome(),
            OperationOutcome::Succeeded
        ),
        Err(failure) => assert!(matches!(
            failure.error(),
            ExecutionError::ReflinkUnsupported { .. }
        )),
    }
}

#[test]
fn preflight_failure_reports_operation_without_partial_output() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing.mkv");
    let output = root.path().join("library");
    let mut plan = write_plan(&output, "movie.json", b"metadata");
    plan.push(OutputOperation::copy(&missing, "movie.mkv").unwrap());
    let prepared = plan.prepare().unwrap();
    let failure = prepared.execute(ExecutionPolicy::default()).unwrap_err();
    assert_eq!(failure.report().operations()[0].operation_index(), 1);
    assert_eq!(
        failure.report().operations()[0].outcome(),
        OperationOutcome::Failed
    );
    assert!(!output.join("movie.json").exists());
    assert!(walkdir(&output).iter().all(|path| {
        !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".fixer-tmp")
    }));
}

#[test]
fn explicit_replace_policy_replaces_bytes_and_hardlinks() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("library");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("movie.json"), "old").unwrap();
    write_plan(&output, "movie.json", b"new")
        .prepare()
        .unwrap()
        .execute(ExecutionPolicy::default().with_overwrite(OverwritePolicy::Replace))
        .unwrap();
    assert_eq!(
        fs::read_to_string(output.join("movie.json")).unwrap(),
        "new"
    );

    let source = root.path().join("source.mkv");
    fs::write(&source, "media").unwrap();
    fs::write(output.join("movie.mkv"), "old media").unwrap();
    plan_media_placement(&source, &output, "movie.mkv", PlacementMode::Hardlink)
        .unwrap()
        .prepare()
        .unwrap()
        .execute(ExecutionPolicy::default().with_overwrite(OverwritePolicy::Replace))
        .unwrap();
    assert_eq!(
        fs::read_to_string(output.join("movie.mkv")).unwrap(),
        "media"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&source).unwrap().ino(),
            fs::metadata(output.join("movie.mkv")).unwrap().ino()
        );
    }
}

#[cfg(unix)]
#[test]
fn symlinked_output_ancestors_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("library")).unwrap();
    let error = write_plan(root.path(), "library/movie.json", b"metadata")
        .prepare()
        .unwrap_err();
    assert!(matches!(error, ExecutionError::UnsafeTarget { .. }));
    assert!(!outside.path().join("movie.json").exists());
}

fn walkdir(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path.clone());
                }
                found.push(path);
            }
        }
    }
    found
}
