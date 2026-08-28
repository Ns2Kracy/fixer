# Providers and connectivity

Fixer composes concrete Rust provider crates through the [`Provider`](sdk.md#author-a-provider) trait. The CLI contains a fixed set of compiled providers and applies configuration before registering them.

## Provider matrix

| ID | Media | Network | Credential | Default endpoint |
| --- | --- | --- | --- | --- |
| `local` | movie, television, anime, music, book | no | none | local files and sidecars |
| `tmdb` | movie, television | yes | TMDB bearer token required | `https://api.themoviedb.org` |
| `bangumi` | anime | yes | none | `https://api.bgm.tv` |
| `anilist` | anime | yes | optional bearer token | `https://graphql.anilist.co` |
| `musicbrainz` | music | yes | none | `https://musicbrainz.org/ws/2/` |
| `openlibrary` | book | yes | none | `https://openlibrary.org/` and `https://covers.openlibrary.org/b/` |

Inspect the compiled matrix with:

```bash
fixer providers list
```

The command lists compiled capabilities, not effective registration. `enabled_providers` is an allowlist, but the CLI registers allowed providers in fixed order: local, TMDB, Bangumi, MusicBrainz, Open Library, then AniList. That fixed order controls CLI merge precedence. AniList is disabled by default. TMDB is omitted when no token resolves, even if `tmdb` appears in `enabled_providers`.

See [configuration](configuration.md) for file discovery, precedence, all endpoint fields, and secret references.

## Credentials

TMDB requires a non-empty token. Prefer an environment-backed secret reference or `TMDB_API_TOKEN` rather than a token in `fixer.json`:

```bash
export TMDB_API_TOKEN='...'
fixer --local-root ./library search movie 'Arrival' --year 2016
```

AniList works without authentication for public requests and accepts `ANILIST_ACCESS_TOKEN` when needed. Bangumi, MusicBrainz, and Open Library send identifiable application user agents. Keep provider identity intact and follow service terms and rate limits.

## Direct and proxied access

The default `fixer-http` client uses Reqwest with rustls and a 30-second timeout.

### Direct access

For direct routing, leave `proxy`/`FIXER_PROXY`/`--proxy` unset and clear proxy environment variables in the Fixer process. This disables inherited shell/service variables, but the pinned Reqwest `system-proxy` feature can still discover macOS or Windows system proxy settings. Fixer currently has no CLI switch that forces the default client to bypass OS proxy discovery. Inject a custom `HttpClient` with an explicit no-proxy policy when direct routing must be guaranteed.

### Standard proxy variables

Without an explicit Fixer proxy, Reqwest reads conventional upper- and lower-case variables:

```bash
export HTTPS_PROXY='http://127.0.0.1:7890'
export HTTP_PROXY='http://127.0.0.1:7890'
export ALL_PROXY='socks5h://127.0.0.1:1080'
export NO_PROXY='127.0.0.1,localhost,.example.internal'
fixer --local-root ./library search movie 'Arrival'
```

`HTTP_PROXY` applies to HTTP destinations, `HTTPS_PROXY` to HTTPS destinations, and `ALL_PROXY` to both. Scheme-specific values override `ALL_PROXY`; upper-case values win when both cases are set. `NO_PROXY` bypasses environment proxies for matching hosts. Lower-case forms are also recognized. On macOS and Windows, the system-proxy layer can add OS-configured routes after environment processing.

### Explicit global proxy

Set one validated HTTP, HTTPS, SOCKS4, SOCKS4a, SOCKS5, or SOCKS5h proxy through a CLI flag, Fixer environment variable, config file, or the SDK:

```bash
fixer --proxy socks5h://127.0.0.1:1080 \
  --local-root ./library search anime 'Frieren'
```

```rust
# use std::time::Duration;
# use fixer_sdk::Fixer;
# fn configure(provider: impl fixer_core::Provider + 'static) -> Result<Fixer, fixer_sdk::SdkError> {
let fixer = Fixer::builder()
    .provider(provider)
    .proxy("socks5h://127.0.0.1:1080")
    .timeout(Duration::from_secs(20))
    .build()?;
# Ok(fixer)
# }
```

An explicit Fixer proxy configures `reqwest::Proxy::all`; it replaces automatic system-proxy selection for that client. If selected destinations must bypass a proxy, use standard environment routing without `FIXER_PROXY`, or inject a custom `HttpClient` that implements the required policy.

## Endpoint overrides

Endpoint overrides support local fixture servers and protocol-compatible mirrors:

| Provider | CLI/config environment | SDK method |
| --- | --- | --- |
| TMDB API | `TMDB_BASE_URL` | `TmdbConfig::with_base_url` |
| TMDB images | SDK only | `TmdbConfig::with_image_base_url` |
| Bangumi | `BANGUMI_BASE_URL` | `BangumiConfig::with_base_url` |
| AniList | `ANILIST_ENDPOINT` | `AniListConfig::with_endpoint` |
| MusicBrainz | `MUSICBRAINZ_BASE_URL` | `MusicBrainzConfig::with_base_url` |
| Open Library API | `OPENLIBRARY_BASE_URL` | `OpenLibraryConfig::with_api_base_url` |
| Open Library covers | `OPENLIBRARY_COVER_BASE_URL` | `OpenLibraryConfig::with_cover_base_url` |

Overrides must use HTTP or HTTPS where the provider enforces that restriction. They are not generic HTML scraper URLs. A replacement service must preserve the provider's expected API paths and response schema. Treat a custom endpoint as a trusted metadata source and protect its credentials and TLS termination.

## Chinese-region connectivity

Fixer does not claim that any public provider is reachable, fast, or legally available from every network in mainland China or another restricted region. Availability can differ by ISP, DNS resolver, provider policy, account, and time.

Use these options in order:

1. Try direct access with a bounded timeout.
2. Use standard proxy variables when the host environment already manages routing and `NO_PROXY` policy.
3. Use Fixer's explicit global proxy when every provider request should follow one route.
4. Point a provider at a controlled, protocol-compatible endpoint when you operate or trust that service.
5. Use `--offline` with local metadata when network access is unavailable.

Fixer does not ship a relay, mirror, VPN, DNS workaround, censorship bypass, or provider response cache. Operators remain responsible for local law, provider terms, credentials, and network policy.

## Offline behavior

`--offline`, `FIXER_OFFLINE=true`, or `FixerBuilder::offline()` skips providers whose descriptor requires network access. It does not replay prior responses and does not make a network-only workflow complete. Register `local` or another provider marked `requires_network = false` when an offline result is required.

A skipped compatible provider adds an `offline_provider_skipped` warning. CLI workflows can still return partial-success exit code `3`. If no remaining provider returns candidates, the SDK returns `SdkError::NoCandidates` or an all-provider failure.

## Timeouts, pacing, retries, and caching

- The default request timeout is 30 seconds and can be changed with `timeout_seconds`, `FIXER_TIMEOUT_SECONDS`, or `FixerBuilder::timeout`.
- MusicBrainz provider instances enforce a one-second minimum interval by default. SDK callers can set a different positive interval with `MusicBrainzConfig::with_minimum_request_interval` when service policy permits it.
- Fixer adds no application-level retry, status-code retry, or backoff policy. The pinned Reqwest client can retry a narrow class of protocol-level NACK failures, so provider implementations must still treat a transport request as potentially replayed.
- Provider responses are not persisted or cached. Server job records preserve workflow artifacts, not a reusable provider response cache.
- The HTTP transport currently buffers each response body in memory and does not impose a separate body-size cap. Use trusted endpoints and infrastructure-level response limits where needed.

Provider search/fetch errors remain structured warnings when another provider produces usable metadata. Inspect warnings before approving output.
