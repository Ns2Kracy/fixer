# Fixer SDK

`fixer-sdk` provides typed, Tokio-based orchestration over compile-time provider implementations. It searches registered providers concurrently, ranks candidates deterministically, fetches ranked metadata, merges it, and preserves field provenance and non-fatal warnings.

## Run the offline example

The repository example parses the committed movie NFO fixture, registers `LocalProvider`, and performs no network I/O:

```bash
cargo run -p fixer-sdk --example sdk_movie
```

The complete source is [`examples/sdk_movie.rs`](../examples/sdk_movie.rs).

## Minimal movie resolution

```rust
use fixer_provider_local::{LocalProvider, parse_nfo};
use fixer_sdk::Fixer;

# async fn resolve(nfo: &str) -> Result<(), Box<dyn std::error::Error>> {
let movie = parse_nfo(nfo)?;
let fixer = Fixer::builder()
    .provider(LocalProvider::from_documents([movie])?)
    .preferred_languages(["zh-Hans", "en"])?
    .offline()
    .build()?;

let resolved = fixer.movie("花样年华").year(2000).resolve().await?;
assert_eq!(resolved.value().release_year(), Some(2000));
assert!(!resolved.provenance.sources_for("movie.titles").is_empty());
# Ok(())
# }
```

`Fixer` also exposes `television`, `anime`, `music`, and `book` typed query builders. Domain-specific query methods add ordering, external-ID, or ISBN constraints where supported.

## Search, select, and fetch

Use the lower-level flow when a caller must display candidates or require an explicit choice:

```rust
# use fixer_sdk::Fixer;
# async fn choose(fixer: Fixer) -> Result<(), fixer_sdk::SdkError> {
let search = fixer.movie("In the Mood for Love").year(2000).search().await?;
for (index, candidate) in search.candidates().iter().enumerate() {
    println!("{index}: {}:{}", candidate.external_id().namespace, candidate.external_id().value);
}
for warning in search.warnings() {
    eprintln!("{}: {}", warning.code, warning.message);
}

let resolved = search.select(0)?.fetch_selected().await?;
println!(
    "resolved title from {} source(s)",
    resolved.provenance.sources_for("movie.titles").len()
);
# Ok(())
# }
```

`resolve()` searches first. Movie resolution fetches every ranked movie candidate before deterministic merge. Music and book resolution fetch only the deterministic top candidate. Television and anime resolution narrow ranked results to one compatible cross-provider candidate group. `search().select(index).fetch_selected()` restricts fetching to one explicit candidate for every media kind. An invalid index returns `SdkError::CandidateOutOfBounds`.

## Builder injection points

`Fixer::builder()` validates configuration at `build()` time:

| Method | Contract |
| --- | --- |
| `provider(value)` | Registers a concrete `Provider + 'static`. Registration order sets deterministic merge precedence. IDs must be unique. |
| `preferred_languages(tags)` | Parses ordered BCP 47 tags used in provider requests and locale selection. |
| `http_client(value)` | Replaces the default runtime-neutral `HttpClient`; use it for tests, custom TLS, tracing, or an embedding application's transport. |
| `offline()` | Skips every provider whose descriptor declares `requires_network = true` and installs a disabled default HTTP client. |
| `proxy(url)` | Sets one explicit global HTTP, HTTPS, or SOCKS proxy on the default Reqwest transport. |
| `timeout(duration)` | Replaces the online default transport's 30-second request timeout. Zero is rejected only when that default client is built. |
| `build()` | Requires at least one provider, rejects duplicate provider IDs, and validates transport settings when constructing the online default client. |

A custom `http_client` takes precedence over the default transport. In that case, the custom client owns proxy and timeout behavior; `proxy` and `timeout` are accepted but ignored. `offline()` without a custom client installs a disabled transport and likewise does not parse the stored proxy or validate the timeout.

Providers for a media kind are searched concurrently. A failed provider becomes a warning when another provider returns usable candidates or metadata. Search returns `SdkError::NoCandidates` only when the candidate set and warning set are both empty; an empty candidate set with any provider/offline warning returns `SdkError::AllProvidersFailed`. Fetch also returns `AllProvidersFailed` when no metadata document survives. Offline mode adds an `offline_provider_skipped` warning when it omits a compatible network provider.

## Author a provider

Providers are Rust implementations, not runtime-loaded plugins. The compiler enforces `Send + Sync`, typed requests and responses, and an object-safe boxed-future boundary:

```rust
use fixer_core::{
    BoxFuture, Candidate, FetchRequest, HttpClient, MediaKind, MetadataDocument,
    Provider, ProviderDescriptor, ProviderError, ProviderId, SearchRequest,
};

struct CatalogProvider {
    descriptor: ProviderDescriptor,
}

impl CatalogProvider {
    fn new() -> Result<Self, fixer_core::CoreError> {
        Ok(Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new("catalog")?,
                "Private catalog",
                [MediaKind::Movie],
            )?
            .with_network_requirement(false),
        })
    }
}

impl Provider for CatalogProvider {
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
            Ok(Vec::new())
        })
    }

    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        _http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            self.descriptor.ensure_support(request.media_kind())?;
            Err(ProviderError::NotFound)
        })
    }
}
```

A real network provider should issue requests only through the supplied `&dyn HttpClient`. Set `with_network_requirement(true)` (the descriptor default) so `offline()` can skip it. Validate response shape before constructing `MetadataDocument`, map transport failures to `ProviderError::Transport`, malformed payloads to `InvalidResponse`, and unsupported media to `UnsupportedMedia`.

Register the concrete type directly:

```rust
# use fixer_sdk::Fixer;
# fn build(provider: impl fixer_core::Provider + 'static) -> Result<Fixer, fixer_sdk::SdkError> {
let fixer = Fixer::builder().provider(provider).build()?;
# Ok(fixer)
# }
```

## Output plans

Writers return `fixer_core::OutputPlan`; planning performs no I/O. Prepare, preview, and execute plans through `fixer_sdk::output::OutputPlanExt`. Execution defaults to no-overwrite and validates source/target state again before mutation. See [Output planning and execution](output.md) for placement and atomicity guarantees.

## Errors and compatibility

`SdkError` and output `ExecutionError` are `#[non_exhaustive]`; downstream matches need a wildcard arm. Domain validation errors enter through `SdkError::Core`, and online default-client construction failures use `SdkError::HttpConfig`. Typed query orchestration converts individual provider search/fetch failures into `ResolutionWarning` values when usable output remains. It returns `SdkError::AllProvidersFailed` for an empty warned search or a fetch with no surviving document. Do not rely on receiving `SdkError::Provider` directly from normal query flows.

The SDK's Rust domain structs are not the CLI's stable JSON contract. Automation should consume the CLI versioned DTOs described in [CLI workflows](cli.md), or define an application-owned serialization boundary around SDK values.
