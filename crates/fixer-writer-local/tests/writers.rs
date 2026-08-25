use fixer_core::{
    FieldPath, LocalizedValue, MetadataDocument, Movie, OutputOperation, ProvenanceMap, ProviderId,
    ReleaseDate, ReleaseId, Resolved, SourceRef, WorkId, Writer,
};
use fixer_writer_local::{
    ContentTemplate, JsonWriter, ManifestWriter, NfoWriter, PathTemplate, TemplateContext,
};
use std::{collections::BTreeMap, path::PathBuf, time::UNIX_EPOCH};

fn resolved_movie() -> Resolved<Movie> {
    let mut titles = LocalizedValue::new();
    titles.insert("zh-CN", "花样年华".to_owned()).unwrap();
    titles
        .insert("en", "In the Mood for Love".to_owned())
        .unwrap();
    let mut movie = Movie::new(WorkId::new("movie-1").unwrap(), titles);
    movie.releases.push(fixer_core::MovieRelease::new(
        ReleaseId::new("movie-1-release").unwrap(),
        ReleaseDate::year(2000).unwrap(),
    ));
    let source = SourceRef::new(
        ProviderId::new("local").unwrap(),
        None,
        Some("zh-CN".parse().unwrap()),
        UNIX_EPOCH,
    );
    let mut provenance = ProvenanceMap::new();
    provenance.add("movie.titles", source).unwrap();
    Resolved {
        value: movie,
        provenance,
        conflicts: Vec::new(),
        completeness: 0.5,
        warnings: Vec::new(),
    }
}

#[test]
fn path_template_renders_allowlisted_variables_and_filters() {
    let template = PathTemplate::new("{{ title | sanitize }} ({{ year }})/movie.json").unwrap();
    let context = TemplateContext::movie(&resolved_movie(), ["zh-CN", "en"]).unwrap();
    assert_eq!(
        template.render(&context).unwrap(),
        PathBuf::from("花样年华 (2000)/movie.json")
    );
}

#[test]
fn path_template_rejects_traversal_absolute_nul_and_missing_values() {
    let context = TemplateContext::movie(&resolved_movie(), ["en"]).unwrap();
    for template in [
        "../escape",
        "/absolute",
        "{{ title }}/../../escape",
        "bad\0path",
    ] {
        assert!(
            PathTemplate::new(template)
                .and_then(|template| template.render(&context))
                .is_err()
        );
    }
    assert!(
        PathTemplate::new("{{ edition }}/movie.json")
            .unwrap()
            .render(&context)
            .is_err()
    );
    assert!(PathTemplate::new("{{ arbitrary }}").is_err());
}

#[test]
fn content_template_projects_preferred_locale() {
    let template = ContentTemplate::new("{{ title }}|{{ year }}").unwrap();
    let context = TemplateContext::movie(&resolved_movie(), ["en", "zh-CN"]).unwrap();
    assert_eq!(
        template.render(&context).unwrap(),
        "In the Mood for Love|2000"
    );
}

#[test]
fn json_and_nfo_writers_are_deterministic_and_only_plan() {
    let resolved = resolved_movie();
    let output = tempfile::tempdir().unwrap();
    let target = output.path().join("not-created");
    let json = JsonWriter.plan_resolved(&resolved, &target).unwrap();
    let nfo = NfoWriter.plan_resolved(&resolved, &target).unwrap();
    assert!(!target.exists());
    assert!(matches!(
        json.operations()[0],
        OutputOperation::WriteBytes { .. }
    ));
    assert!(matches!(
        nfo.operations()[0],
        OutputOperation::WriteBytes { .. }
    ));
    insta::assert_snapshot!("movie_json", bytes(&json));
    insta::assert_snapshot!("movie_nfo", bytes(&nfo));
}

#[test]
fn manifest_records_field_sources_and_planned_files_without_secrets() {
    let resolved = resolved_movie();
    let mut planned_files = BTreeMap::new();
    planned_files.insert("movie_json".to_owned(), PathBuf::from("Movie/movie.json"));
    let plan = ManifestWriter::new(planned_files)
        .plan_resolved(&resolved, "library")
        .unwrap();
    let content = bytes(&plan);
    assert!(content.contains("movie.titles"));
    assert!(content.contains("local"));
    assert!(content.contains("Movie/movie.json"));
    assert!(!content.to_ascii_lowercase().contains("authorization"));
    insta::assert_snapshot!("movie_manifest", content);
}

fn bytes(plan: &fixer_core::OutputPlan) -> String {
    match &plan.operations()[0] {
        OutputOperation::WriteBytes { content, .. } => {
            String::from_utf8(content.as_bytes().to_vec()).unwrap()
        }
        operation => panic!("unexpected operation: {operation:?}"),
    }
}

#[test]
fn core_writer_trait_remains_planning_only() {
    let writer: Box<dyn Writer> = Box::new(JsonWriter);
    let request = fixer_core::WriteRequest::new(
        MetadataDocument::Movie(resolved_movie().value),
        PathBuf::from("library"),
    );
    assert_eq!(writer.plan(request).unwrap().operations().len(), 1);
    assert!(FieldPath::new("movie.titles").is_ok());
}
