use fixer_core::{
    BoxFuture, Candidate, ExternalId, FetchRequest, Header, HttpClient, HttpError, HttpMethod,
    HttpRequest, HttpResponse, MediaKind, MetadataDocument, Movie, MovieCandidate, OutputOperation,
    OutputPlan, PlannedContent, PlanningError, Provider, ProviderDescriptor, ProviderError,
    ProviderId, SearchRequest, WorkId, WriteRequest, Writer,
};
use std::{path::PathBuf, sync::Arc};

struct FakeHttp;

impl HttpClient for FakeHttp {
    fn execute<'a>(
        &'a self,
        request: HttpRequest,
    ) -> BoxFuture<'a, Result<HttpResponse, HttpError>> {
        Box::pin(async move { Ok(HttpResponse::new(200).with_body(request.url.into_bytes())) })
    }
}

struct FakeProvider {
    descriptor: ProviderDescriptor,
}

impl FakeProvider {
    fn new() -> Self {
        Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new("fixture").unwrap(),
                "Fixture",
                [MediaKind::Movie],
            )
            .unwrap(),
        }
    }
}

impl Provider for FakeProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn search<'a>(
        &'a self,
        request: SearchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        Box::pin(async move {
            self.descriptor.ensure_support(request.media_kind())?;
            Ok(vec![Candidate::Movie(MovieCandidate::new(
                self.descriptor.id().clone(),
                ExternalId::new("fixture", "movie-1").unwrap(),
                request.title().unwrap_or_default(),
                request.year(),
            )?)])
        })
    }

    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            self.descriptor.ensure_support(request.media_kind())?;
            let mut titles = fixer_core::LocalizedValue::new();
            titles.insert("en", "Fixture Movie".to_owned())?;
            Ok(MetadataDocument::Movie(Movie::new(
                WorkId::new("movie-1")?,
                titles,
            )))
        })
    }
}

struct FakeWriter;

impl Writer for FakeWriter {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError> {
        let mut plan = OutputPlan::new(request.output_root);
        plan.push(OutputOperation::write_bytes(
            "movie.json",
            PlannedContent::new(serde_json::to_vec(&request.document)?),
        )?);
        Ok(plan)
    }
}

#[test]
fn extension_contracts_are_object_safe() {
    let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new());
    let http: Arc<dyn HttpClient> = Arc::new(FakeHttp);
    let writer: Arc<dyn Writer> = Arc::new(FakeWriter);

    let candidates = futures_lite::future::block_on(provider.search(
        SearchRequest::movie("Fixture Movie", Some(2000)).unwrap(),
        http.as_ref(),
    ))
    .unwrap();
    assert_eq!(candidates.len(), 1);

    let response = futures_lite::future::block_on(http.execute(HttpRequest::new(
        HttpMethod::Get,
        "https://example.invalid/movie",
    )))
    .unwrap();
    assert_eq!(response.status, 200);

    let document = futures_lite::future::block_on(provider.fetch(
        FetchRequest::new(
            MediaKind::Movie,
            ExternalId::new("fixture", "movie-1").unwrap(),
        ),
        http.as_ref(),
    ))
    .unwrap();
    let plan = writer
        .plan(WriteRequest::new(document, PathBuf::from("output")))
        .unwrap();
    assert_eq!(plan.operations().len(), 1);
}

#[test]
fn capabilities_filter_media_with_a_structured_error() {
    let descriptor = FakeProvider::new().descriptor;
    assert!(descriptor.supports(MediaKind::Movie));
    assert!(!descriptor.supports(MediaKind::Book));

    let error = descriptor.ensure_support(MediaKind::Book).unwrap_err();
    assert!(matches!(
        error,
        ProviderError::UnsupportedMedia {
            provider,
            media_kind: MediaKind::Book
        } if provider.as_str() == "fixture"
    ));
}

#[test]
fn output_plans_serialize_with_previewable_sources_and_targets() {
    let mut plan = OutputPlan::new("library");
    plan.push(OutputOperation::create_directory("Movie (2000)").unwrap());
    plan.push(
        OutputOperation::write_bytes(
            "Movie (2000)/movie.json",
            PlannedContent::new(br#"{"title":"Movie"}"#.to_vec()),
        )
        .unwrap(),
    );
    plan.push(OutputOperation::copy("incoming/movie.mkv", "Movie (2000)/movie.mkv").unwrap());
    plan.push(OutputOperation::symlink("Movie (2000)/movie.mkv", "by-title/Movie.mkv").unwrap());
    plan.push(
        OutputOperation::hardlink("incoming/movie.mkv", "Movie (2000)/movie-hard.mkv").unwrap(),
    );
    plan.push(
        OutputOperation::reflink("incoming/movie.mkv", "Movie (2000)/movie-clone.mkv").unwrap(),
    );

    for operation in plan.operations() {
        assert!(operation.target().is_some());
        if matches!(
            operation,
            OutputOperation::Copy { .. }
                | OutputOperation::Symlink { .. }
                | OutputOperation::Hardlink { .. }
                | OutputOperation::Reflink { .. }
        ) {
            assert!(operation.source().is_some());
        }
    }

    let json = serde_json::to_string(&plan).unwrap();
    let decoded: OutputPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, plan);
}

#[test]
fn sensitive_headers_are_redacted_in_debug_output() {
    let request = HttpRequest::new(HttpMethod::Get, "https://example.invalid")
        .with_header(Header::new("authorization", "Bearer super-secret").unwrap())
        .with_header(Header::new("accept", "application/json").unwrap());

    let debug = format!("{request:?}");
    assert!(!debug.contains("super-secret"));
    assert!(debug.contains("[REDACTED]"));
    assert!(debug.contains("application/json"));
}
