# Security model

Fixer is a single-user, local-first metadata tool with deliberate filesystem write capability. Treat the server as an administrative service: anyone who authenticates can inspect configured libraries, submit jobs, review paths, and approve writes under allowed media roots.

It is not a multi-tenant authorization system, public metadata proxy, or sandbox for hostile provider plugins.

## Trust boundaries

Fixer relies on five boundaries:

1. The listener and reverse proxy limit who can reach the service.
2. Administrator sessions or bearer API tokens authenticate protected API calls.
3. exact-origin CORS and CSRF checks protect browser sessions.
4. `FsPolicy` confines server reads and writes to canonical media roots.
5. planning, explicit approval, idempotency, no-overwrite, and stale-state checks constrain filesystem mutation.

Each boundary covers a different threat. CORS does not replace authentication; the media-root allowlist does not replace OS permissions; a preview does not make an untrusted provider response correct.

## Bind and authentication defaults

The default listener is `127.0.0.1:3000`. `ServerConfig::new` supports unauthenticated loopback routers for tests and embedding. The production `serve` path requires at least one media root and persistent SQLite authentication state, but it does not read a password from the environment. Keep the listener private unless a trusted reverse proxy or private network controls access.

A new database has no administrator. `GET /api/v1/auth/status` reports `registration_required: true`, and `POST /api/v1/auth/register` atomically creates the single administrator account. Registration is unavailable as soon as that account exists, so complete first-run setup from a trusted client before making the listener broadly reachable.

Registration hashes the chosen password with Argon2id and a random salt in a blocking worker, then stores the username and PHC hash in `fixer_users`. Startup never rewrites the stored credential. Debug output redacts password material.

Login requires both username and password. Failures return one generic `invalid_credentials` response. There is currently no built-in login rate limiter, account lockout, password reset, or password rotation flow. Keep the listener private and apply connection/login limits at a trusted reverse proxy when other users can reach the network.

## Browser sessions and CSRF

A successful `POST /api/v1/auth/register` or `POST /api/v1/auth/login` creates a 12-hour session and returns:

- an opaque `fixer_session` cookie scoped to `/api`, marked `HttpOnly` and `SameSite=Strict`;
- a separate CSRF token in the versioned JSON response;
- the session expiration timestamp.

Only SHA-256 token and CSRF digests are stored in SQLite. State-changing cookie-authenticated requests (`POST`, `PUT`, `PATCH`, and `DELETE`) must send the matching token as `X-CSRF-Token`. Read methods still require a valid session but not CSRF.

Set `FIXER_SERVER_HTTPS_TERMINATION=true` whenever the browser's external origin uses HTTPS. This adds `Secure` to the cookie. The setting does not start TLS or verify that a reverse proxy is configured. Setting it incorrectly on cleartext HTTP prevents the browser from returning the cookie; leaving it false behind HTTPS weakens transport protection.

Logout revokes the persisted session and expires the cookie. Session responses use `Cache-Control: no-store`.

## API tokens

Protected routes accept `Authorization: Bearer fixer_pat_...`. Bearer authentication does not require CSRF because it is not ambient browser-cookie authority.

`SqliteJobStore` can issue and revoke named API tokens. Plaintext appears only on the returned `IssuedApiToken`; SQLite stores a SHA-256 digest and debug output redacts the secret. The current server has no HTTP or CLI token-management command, so operators embedding `fixer-server` must build an administrative issuance path around the store API. Do not manipulate token tables by hand.

Treat bearer tokens as passwords. Send them only over HTTPS, keep them out of URLs and logs, assign one per automation client, and revoke them when a client is retired.

## Origin policy

When an API request includes `Origin`, Fixer requires an exact match in `FIXER_SERVER_ALLOWED_ORIGINS`. Allowed entries must be HTTP or HTTPS origins with a host and optional port, without a path, wildcard, credentials, query, or fragment. Static Web routes are merged outside API CORS middleware and are not rejected by this origin check.

Example:

```bash
FIXER_SERVER_ALLOWED_ORIGINS='https://fixer.example.com,http://127.0.0.1:5173'
```

CORS preflight allows credentials and the headers used by authentication, CSRF, idempotency, and event resumption. Its current method list includes `GET`, `HEAD`, and `POST`, but not `PUT`; a browser cannot call `PUT /api/v1/settings` directly across origins even when that origin is allowed. Requests without `Origin` proceed to authentication. This supports non-browser clients, so origin policy must not be treated as network access control.

`SameSite=Strict` and CSRF validation remain active even with CORS configured. Do not reflect arbitrary origins or use a wildcard at a reverse proxy.

## Trusted proxies

Forwarded identity is disabled by default. Fixer uses the direct socket peer unless both trusted proxy variables are valid:

```bash
FIXER_SERVER_TRUSTED_PROXY_RANGES='10.0.0.0/8,fd00::/8'
FIXER_SERVER_TRUSTED_PROXY_HEADER='x-fixer-client-ip'
```

Fixer accepts the header only when the direct peer belongs to a configured CIDR. The value must parse as one IP address. Invalid, absent, or multi-hop values fall back to the socket peer. `Authorization`, `Cookie`, and `Proxy-Authorization` cannot be selected as identity headers.

Configure the edge proxy to remove the client-supplied header and write one canonical value. Restrict direct listener access. Trusted-proxy identity is currently informational; it does not enable rate limiting or authorization by client IP.

## Filesystem security

`FIXER_SERVER_MEDIA_ROOTS` is the server's mandatory filesystem allowlist. Roots are existing canonical directories. Reads canonicalize the requested path. Writes check the nearest existing ancestor so a not-yet-created target cannot escape through a symlink. Output targets and absolute sources are revalidated immediately before execution.

A known embedding gap affects relative non-symlink operation sources: `FsPolicy` resolves them against the output root, while the SDK executor resolves them against the process working directory. Built-in server writers currently avoid such operations. Embedders must construct absolute canonical copy/hardlink/reflink sources; do not accept externally supplied relative-source plans.

The SDK adds independent target traversal, symlink-ancestor, stale fingerprint, and default no-overwrite checks. These checks reduce path confusion and time-of-check/time-of-use risk; they do not compensate for excessive OS permissions.

Run Fixer as a dedicated unprivileged account. Give that account read access to source media and write access only where output is intended. Avoid roots containing unrelated secrets, home directories, sockets, device files, or host mounts.

Local scanning does not follow symlinks. Symlink placement is an explicit reviewed output operation and remains subject to source/target policy.

## Write approval and recovery

Planning performs no filesystem mutation. Server jobs expose candidate/conflict review and a bounded operation preview before execution. Execution requires all of:

- a job created with `apply: true` and in the correct planned state;
- a review decision accepting the complete zero-based set of conflicts;
- explicit `{ "approved": true }` input;
- a valid `Idempotency-Key`;
- authentication and CSRF for browser sessions;
- a plan that passes media-root and SDK execution validation.

Default execution refuses existing file-like targets; `create_directory` accepts an existing directory. Byte writes and copies publish complete temporary files, but the whole plan is not transactional. Earlier successful operations remain after a later failure. Review `ExecutionFailure` or persisted execution details before retrying.

On restart, active jobs become `interrupted`. Jobs with an execution reservation fail closed rather than automatically replaying a possible write. See [Output planning and execution](output.md) and [Server operations](server.md#restart-and-interrupted-jobs).

## Provider and metadata trust

Network providers and endpoint overrides are external trust boundaries. Fixer validates typed response structure before merging, but a syntactically valid provider can still return incorrect titles, paths-as-data, artwork URLs, or maliciously chosen text.

- Use official endpoints or controlled compatible mirrors.
- Keep bearer credentials scoped and outside committed configuration.
- Inspect warnings, conflicts, output roots, and operation targets before approval.
- Treat provider IDs, external IDs, and provenance manifests as potentially sensitive library metadata.
- Do not implement a provider by running response text as code or shell input.

The default HTTP client buffers response bodies and has no application-specific response-size cap or retry layer. Apply egress restrictions, DNS/TLS policy, and response limits in infrastructure when provider endpoints are not fully trusted.

## Data at rest

SQLite contains library paths, job inputs, review decisions, plan counts/fingerprints, execution counts, the administrator username and password hash, and token/session digests. It does not persist full output operation bytes or plaintext administrator passwords, issued session tokens, CSRF tokens, or API tokens. A database leak still exposes operational metadata and material for offline password guessing. Restoring the database also restores its administrator account, unexpired sessions, and unrevoked API tokens.

Web workspace provider settings and plaintext provider tokens live in process memory only and reset on restart. CLI secrets can come from environment-backed references. Configuration validation and debug output redact secret values.

Protect the database and backups with restrictive ownership, encrypted storage, and retention limits. Stop the server before a file-copy backup. See [backup and restore](server.md#backup-and-restore).

## Known limits

- Fixer provides one administrator account, not multiple users or role-based permissions.
- There is no built-in password reset, password rotation, or administrator-management command.
- There is no built-in request, login, or provider rate limiter except MusicBrainz request pacing.
- API token issuance/revocation has no operator CLI or HTTP route.
- Workspace settings are not persisted.
- Cross-origin preflight omits `PUT`, so direct cross-origin settings updates do not work.
- Axum's default 2 MiB JSON body limit is framework-defined rather than an explicit Fixer policy.
- Provider HTTP responses have no separate size cap and are buffered in memory.
- Local EPUB scanning caps selected container/OPF entries at 1 MiB but currently has no whole-archive size cap.
- Review/plan path strings are clipped to 2,048 characters without per-field truncation flags.
- Relative non-symlink operation source resolution differs between `FsPolicy` and the SDK executor.
- Open server-sent event connections can delay graceful shutdown.
- CORS accepts API requests without `Origin` and therefore cannot serve as an API firewall.
- The server does not terminate TLS.

Place compensating controls at the process, filesystem, reverse proxy, firewall, and backup layers.

## Deployment checklist

- Bind to loopback or a private interface behind authenticated network access.
- Complete first-run registration from a trusted client, choose a long unique administrator password, and use HTTPS for every non-local browser.
- Set `FIXER_SERVER_HTTPS_TERMINATION=true` behind HTTPS.
- List exact browser origins and test a rejected origin.
- Configure trusted proxy CIDRs/header only when client identity is needed.
- Run as an unprivileged account with narrow canonical media roots.
- Store SQLite at an explicit protected absolute path and test stopped-server restore.
- Build `web/dist` from the lockfile and keep HTML revalidating at the proxy.
- Review provider endpoint overrides and secret sources.
- Preview a job, reject any path that may exceed the 2,048-character display bound, verify the visible output root and operations, then test one bounded write.
