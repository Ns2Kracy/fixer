# First-run Administrator Authentication Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the environment-provided server password with an atomic first-run administrator registration flow and a standalone guarded login experience.

**Architecture:** Keep the existing single-row authentication model. Add a nullable username migration so existing password-only databases become unregistered until the administrator claims them, expose public auth status/register/login endpoints, and let the Web root route guard protected pages before the application shell renders. Keep session cookies, CSRF, Argon2id, API tokens, and filesystem boundaries unchanged.

**Tech Stack:** Rust, Axum, SQLx/SQLite, Solid 2, TanStack Solid Router/Query, Vitest, Playwright, Docker Compose.

---

### Task 1: Persist the first administrator atomically

**Files:**

- Create: `crates/fixer-server/migrations/0004_single_user_username.sql`
- Modify: `crates/fixer-server/src/store/sqlite.rs:57-85`
- Test: `crates/fixer-server/tests/auth.rs:76-152`

**Step 1: Write the failing store test**

Replace the password-only store test setup with a test that proves the complete single-user credential contract:

```rust
#[tokio::test]
async fn first_administrator_registration_is_atomic_and_credentials_include_username() {
    let root = tempfile::tempdir().unwrap();
    let store = SqliteJobStore::open(root.path().join("auth.sqlite3"))
        .await
        .unwrap();

    assert!(!store.has_registered_user().await.unwrap());

    let first = hash_password("correct horse battery staple").unwrap();
    assert!(store.register_single_user("admin", &first).await.unwrap());
    assert!(store.has_registered_user().await.unwrap());
    assert!(store
        .verify_single_user_credentials("admin", "correct horse battery staple")
        .await
        .unwrap());
    assert!(!store
        .verify_single_user_credentials("other", "correct horse battery staple")
        .await
        .unwrap());

    let replacement = hash_password("replacement password").unwrap();
    assert!(!store
        .register_single_user("second-admin", &replacement)
        .await
        .unwrap());
    assert!(store
        .verify_single_user_credentials("admin", "correct horse battery staple")
        .await
        .unwrap());
}
```

Retain the existing session digest assertions in a separate test or below this registration proof.

**Step 2: Run the focused test and confirm RED**

Run:

```bash
cargo test -p fixer-server --test auth first_administrator_registration_is_atomic_and_credentials_include_username
```

Expected: compilation fails because `has_registered_user`, `register_single_user`, and `verify_single_user_credentials` do not exist.

**Step 3: Add the forward-compatible migration**

Create `0004_single_user_username.sql`:

```sql
ALTER TABLE single_user_auth
ADD COLUMN username TEXT
CHECK (username IS NULL OR length(trim(username)) BETWEEN 3 AND 64);
```

The column is nullable so existing password-only databases migrate safely. A row with `username IS NULL` is intentionally treated as not registered and can be claimed through the new registration page.

**Step 4: Implement the minimum store API**

In `SqliteJobStore`:

```rust
pub async fn has_registered_user(&self) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM single_user_auth WHERE id = 1 AND username IS NOT NULL)",
    )
    .fetch_one(&self.pool)
    .await
    .map_err(Into::into)
}

pub async fn register_single_user(
    &self,
    username: &str,
    password_hash: &PasswordHashValue,
) -> Result<bool, StoreError> {
    let result = sqlx::query(
        "INSERT INTO single_user_auth (id, username, password_hash, updated_at_ms) \
         VALUES (1, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
           username = excluded.username, \
           password_hash = excluded.password_hash, \
           updated_at_ms = excluded.updated_at_ms \
         WHERE single_user_auth.username IS NULL",
    )
    .bind(username)
    .bind(password_hash.as_str())
    .bind(timestamp_ms()?)
    .execute(&self.pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn verify_single_user_credentials(
    &self,
    username: &str,
    password: &str,
) -> Result<bool, StoreError> {
    let Some(encoded) = sqlx::query_scalar::<_, String>(
        "SELECT password_hash FROM single_user_auth WHERE id = 1 AND username = ?",
    )
    .bind(username)
    .fetch_optional(&self.pool)
    .await?
    else {
        return Ok(false);
    };
    let encoded = PasswordHashValue::parse(encoded)?;
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || verify_password(&password, &encoded))
        .await?
        .map_err(Into::into)
}
```

Delete `set_password_hash` and `verify_single_user_password` after all test helpers move to registration.

**Step 5: Run the focused store test and confirm GREEN**

Run the same focused command. Expected: PASS.

**Step 6: Commit**

```bash
git add crates/fixer-server/migrations/0004_single_user_username.sql \
  crates/fixer-server/src/store/sqlite.rs crates/fixer-server/tests/auth.rs
git commit -m "feat(server): persist first-run administrator credentials"
```

---

### Task 2: Add auth status and first-registration HTTP APIs

**Files:**

- Modify: `crates/fixer-server/src/api/v1/auth.rs:1-117`
- Modify: `crates/fixer-server/src/api/v1/mod.rs:45-72`
- Modify: `crates/fixer-server/tests/auth.rs:154-442`

**Step 1: Write failing HTTP tests**

Add tests that start with an unregistered `SqliteJobStore` and prove:

```rust
GET /api/v1/auth/status
=> 200 { "schema_version": 1, "registration_required": true, "authenticated": false, "username": null }

POST /api/v1/auth/register { "username": "admin", "password": "long enough password" }
=> 200 + strict HttpOnly session cookie + CSRF token

GET /api/v1/auth/status with cookie
=> 200 { "registration_required": false, "authenticated": true, "username": "admin" }

second POST /api/v1/auth/register
=> 409 with code "registration_closed"

POST /api/v1/auth/login { "username": "admin", "password": "long enough password" }
=> 200

wrong username or wrong password
=> the same 401 "invalid_credentials" envelope
```

Also add invalid registration cases for a username shorter than 3 characters and a password shorter than 8 bytes; expect `422 invalid_registration` with safe field details.

**Step 2: Run the focused HTTP tests and confirm RED**

```bash
cargo test -p fixer-server --test auth registration
cargo test -p fixer-server --test auth login_sets_strict_http_only_cookie
```

Expected: `/status` and `/register` are 404, and login does not accept username.

**Step 3: Implement status, registration, and username login**

In `auth.rs`:

- Route `GET /auth/status` and `POST /auth/register` from `public_router`.
- Change `LoginRequest` to `{ username, password }`.
- Add `RegisterRequest`, `AuthStatusResponse`, and a shared `SessionResponse` that includes `username`.
- Validate trimmed username length with `chars().count()` in `3..=64`.
- Validate password byte length in `8..=1024` for registration and `1..=1024` for generic login handling.
- Check `has_registered_user` before expensive hashing, then call `register_single_user` as the authoritative atomic gate.
- Return `409 registration_closed` when the conditional upsert affects zero rows.
- Extract the existing cookie/CSRF response construction into one `session_response` helper used by registration and login.
- For status, inspect `fixer_session`, authenticate it without CSRF, and report the configured username only for a valid session. Add a small store getter such as `registered_username()` if needed; do not expose password data.
- Keep all auth responses `Cache-Control: no-store`.

**Step 4: Run auth tests and confirm GREEN**

```bash
cargo test -p fixer-server --test auth
```

Expected: all authentication, CSRF, token, CORS, registration, login, and logout tests pass.

**Step 5: Commit**

```bash
git add crates/fixer-server/src/api/v1/auth.rs \
  crates/fixer-server/src/api/v1/mod.rs \
  crates/fixer-server/src/store/sqlite.rs \
  crates/fixer-server/tests/auth.rs
git commit -m "feat(server): add first-run registration endpoints"
```

---

### Task 3: Remove the startup password configuration

**Files:**

- Modify: `crates/fixer-server/src/lib.rs:13-103,157-213,248-357`
- Modify: `crates/fixer-server/tests/startup.rs:8-126`

**Step 1: Rewrite startup tests for the new contract**

Tests must prove:

- `ServerConfig::new` and `ServerConfig::parse` accept loopback and non-loopback binds because authentication is now initialized from SQLite.
- `validate_for_serve` requires media roots but not a startup password.
- `from_env` ignores/removes the `FIXER_SERVER_PASSWORD` contract.
- Debug output contains no password field.
- Origin, proxy, bind, database, and media-root validation remain intact.

**Step 2: Run startup tests and confirm RED**

```bash
cargo test -p fixer-server --test startup
```

Expected: tests fail because the current configuration rejects public binds and requires a password.

**Step 3: Delete the old password startup path**

In `lib.rs`:

- Delete `ServerPassword`, `MAX_PASSWORD_BYTES`, and the `password` field.
- Delete `ServerConfig::authenticated` and `validate_password`.
- Let `ServerConfig::new` construct the base config for any valid socket address.
- Remove `FIXER_SERVER_PASSWORD` parsing from `from_env`.
- Make `validate_for_serve` check only `media_policy`.
- Delete `AuthenticationRequired`, `MissingPassword`, and `InvalidPassword` errors.
- Delete password hashing/task errors from `ServeError`.
- In `serve`, open/migrate the store and proceed directly to workspace/auth state construction without writing a password hash.

Do not change CORS, trusted-proxy, HTTPS cookie, media-root, worker, or static-Web initialization.

**Step 4: Run startup and auth tests**

```bash
cargo test -p fixer-server --test startup
cargo test -p fixer-server --test auth
```

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/fixer-server/src/lib.rs crates/fixer-server/tests/startup.rs
git commit -m "refactor(server): remove environment password startup"
```

---

### Task 4: Add the Web auth client contract

**Files:**

- Modify: `web/src/lib/api.ts:186-194,381-461`
- Modify: `web/src/lib/api.test.ts`
- Create: `web/src/lib/auth.ts`

**Step 1: Write failing API client tests**

Add state-based tests that verify exact request and response behavior:

```ts
await client.authStatus()
// GET /api/v1/auth/status with same-origin credentials

await client.register({ username: "admin", password: "long enough password" })
// POST /api/v1/auth/register and persist returned csrf_token

await client.login({ username: "admin", password: "long enough password" })
// POST /api/v1/auth/login and persist returned csrf_token
```

**Step 2: Run the API test and confirm RED**

```bash
cd web && pnpm exec vitest run src/lib/api.test.ts
```

Expected: missing types/methods and old password-only login assertions fail.

**Step 3: Add auth DTOs and methods**

Define:

```ts
export interface AuthStatusResponse {
  schema_version: SchemaVersion;
  registration_required: boolean;
  authenticated: boolean;
  username: string | null;
}

export interface CredentialsRequest {
  username: string;
  password: string;
}

export interface SessionResponse {
  schema_version: SchemaVersion;
  username: string;
  csrf_token: string;
  expires_at_ms: number;
}
```

Add `authStatus`, `register`, and username-aware `login`; registration and login both update the in-memory/sessionStorage CSRF token.

Create `web/src/lib/auth.ts` with one shared query definition:

```ts
export const authStatusQuery = () => ({
  queryKey: ["auth", "status"] as const,
  queryFn: () => api.authStatus(),
  staleTime: Infinity,
});
```

**Step 4: Run the API test and confirm GREEN**

Run the same Vitest command. Expected: PASS.

**Step 5: Commit**

```bash
git add web/src/lib/api.ts web/src/lib/api.test.ts web/src/lib/auth.ts
git commit -m "feat(web): add administrator auth client"
```

---

### Task 5: Guard protected routes and remove login from the shell

**Files:**

- Modify: `web/src/routes/__root.tsx:1-36`
- Modify: `web/src/components/app-shell.tsx:1-144`
- Modify: `web/src/app.test.tsx`
- Modify: route tests under `web/src/routes/**/*.test.tsx` as required by the new status request

**Step 1: Write failing route/layout tests**

Add tests proving:

- `/login` renders without `Workspace navigation` and without the Fixer application header.
- Visiting `/` with `{ authenticated: false }` redirects to `/login` before `/health` is fetched.
- Visiting `/` with `{ authenticated: true, username: "admin" }` renders the workspace.
- The workspace navigation has no `Sign in` link.

Update existing protected-route fetch mocks to return an authenticated status for `/api/v1/auth/status` before their normal endpoint responses.

**Step 2: Run focused Web tests and confirm RED**

```bash
cd web && pnpm exec vitest run src/app.test.tsx src/routes/login.test.tsx
```

Expected: login still renders inside `AppShell`, protected routes do not redirect, and `Sign in` remains in navigation.

**Step 3: Implement the root auth gate**

In `__root.tsx`:

- Add `beforeLoad: async ({ context, location }) => { ... }`.
- Skip redirect logic only for `/login`.
- Use `context.queryClient.ensureQueryData(authStatusQuery())`.
- Throw `redirect({ to: "/login", search: { redirect: location.href } })` when not authenticated.
- Render a root layout that returns a bare `<Outlet />` on `/login` and `<AppShell />` everywhere else.

In `login.tsx`, add `validateSearch` for an optional internal `redirect` string. Before navigating to it later, accept only values beginning with `/` and reject `//` to prevent open redirects.

In `app-shell.tsx`, delete the `/login` navigation item. Do not add another login affordance inside the authenticated shell.

**Step 4: Run focused tests and confirm GREEN**

Run the same two Vitest files, then the other route test files affected by the status call. Expected: PASS.

**Step 5: Commit**

```bash
git add web/src/routes/__root.tsx web/src/routes/login.tsx \
  web/src/components/app-shell.tsx web/src/app.test.tsx web/src/routes
git commit -m "feat(web): guard the workspace behind standalone auth"
```

---

### Task 6: Build registration, sign-in tabs, and sign-out

**Files:**

- Modify: `web/src/routes/login.tsx:1-82`
- Modify: `web/src/routes/login.test.tsx:1-80`
- Modify: `web/src/components/app-shell.tsx`
- Test: `web/src/app.test.tsx`

**Step 1: Write failing interaction tests**

Prove these user-visible behaviors:

1. Unregistered status defaults to the `Sign up` tab and shows Username, Password, and Confirm password.
2. Clicking the `Sign in` tab shows Username and Password and submits the username-aware login payload.
3. Mismatched confirmation blocks registration locally.
4. Successful registration submits `/auth/register`, stores CSRF, updates auth status, and enters the intended route.
5. Registered status defaults to `Sign in`; the `Sign up` tab is disabled or explains that registration is closed.
6. Authenticated shell exposes `Sign out`; clicking it posts `/auth/logout`, clears auth state, and navigates to `/login`.

Use semantic tab roles and accessible field labels in assertions.

**Step 2: Run focused tests and confirm RED**

```bash
cd web && pnpm exec vitest run src/routes/login.test.tsx src/app.test.tsx
```

Expected: registration tabs/forms and sign-out do not exist.

**Step 3: Implement the standalone auth UI**

- Query `authStatusQuery()` on the login page.
- Default mode to `signup` only when `registration_required` is true; otherwise `signin`.
- Render a `role="tablist"` with `Sign in` and `Sign up` buttons using `aria-selected` and `aria-controls`.
- Keep one trimmed username signal and separate password/confirmation signals.
- Disable submit while pending or invalid.
- Show a local confirmation error with `role="alert"`.
- On success, set `["auth", "status"]` to authenticated using the returned username, then navigate to the validated internal redirect or `/`.
- Show status/query failures with `RequestError` and a retry action.
- Add a header-level `Sign out` button in `AppShell`; on success set auth status to unauthenticated, clear CSRF through `api.logout`, and navigate to `/login`.

Keep the existing paper/moss editorial design tokens, responsive behavior, visible labels, keyboard operation, and pending text.

**Step 4: Run focused tests and confirm GREEN**

Run the same Vitest files. Expected: PASS.

**Step 5: Commit**

```bash
git add web/src/routes/login.tsx web/src/routes/login.test.tsx \
  web/src/components/app-shell.tsx web/src/app.test.tsx
git commit -m "feat(web): add registration and sign-in experience"
```

---

### Task 7: Update deployment, operator docs, and critical E2E flow

**Files:**

- Modify: `compose.yaml`
- Modify: `.env.docker.example`
- Modify: `README.md`
- Modify: `docs/server.md`
- Modify: `docs/security.md`
- Modify: `docs/development.md`
- Modify: `docs/troubleshooting.md`
- Modify: `web/e2e/critical-flow.spec.ts`

**Step 1: Update the browser flow first**

Change the critical flow to require `FIXER_E2E_USERNAME` and `FIXER_E2E_PASSWORD`, request auth status, register when `registration_required` is true, otherwise click `Sign in` and log in with both fields. Assert the login page has no workspace navigation before authentication.

**Step 2: Remove environment-password deployment wiring**

- Delete `FIXER_SERVER_PASSWORD` from `compose.yaml`.
- Delete it from `.env.docker.example`.
- Keep the required absolute media path and allowed-origin settings.

**Step 3: Rewrite current operator documentation**

Document:

- startup no longer accepts/requires `FIXER_SERVER_PASSWORD`;
- first browser visit creates the sole administrator;
- existing password-only databases require one-time username/password registration after upgrade;
- first-run registration is claimable by the first client that reaches an uninitialized server, so operators should perform initialization on loopback/private access before exposing the listener;
- registration closes atomically after the administrator exists;
- password rotation is now an account-flow concern and is not implemented in this task;
- sessions and API tokens retain existing backup/restore behavior.

Do not rewrite historical design/implementation plan documents; they are records of past decisions.

**Step 4: Run narrow static checks**

```bash
rg -n 'FIXER_SERVER_PASSWORD' README.md compose.yaml .env.docker.example docs/server.md docs/security.md docs/development.md docs/troubleshooting.md
```

Expected: no matches in current operator surfaces.

```bash
FIXER_MEDIA_PATH="$PWD/Movies" docker compose config
```

Expected: Compose renders successfully without a password variable.

**Step 5: Commit**

```bash
git add compose.yaml .env.docker.example README.md docs/server.md docs/security.md \
  docs/development.md docs/troubleshooting.md web/e2e/critical-flow.spec.ts
git commit -m "docs: switch deployment to first-run registration"
```

---

### Task 8: Final verification and cleanup

**Files:**

- Modify only files required by failures.

**Step 1: Run proactive diagnostics**

Run LSP diagnostics for `crates/fixer-server/src`, `crates/fixer-server/tests`, and `web/src`. Expected: no errors.

**Step 2: Verify Rust formatting and linting**

```bash
cargo fmt --check
cargo clippy -p fixer-server --all-targets -- -D warnings
```

Expected: PASS with no warnings.

**Step 3: Verify server behavior**

```bash
cargo test -p fixer-server --test auth
cargo test -p fixer-server --test startup
cargo test -p fixer-server
```

Expected: PASS.

**Step 4: Verify Web behavior and build**

```bash
cd web
pnpm test
pnpm typecheck
pnpm build
```

Expected: PASS.

**Step 5: Run direct mechanical probes**

Confirm:

```bash
rg -n 'Sign in' web/src/components/app-shell.tsx
rg -n 'FIXER_SERVER_PASSWORD' README.md compose.yaml .env.docker.example docs/server.md docs/security.md docs/development.md docs/troubleshooting.md
```

Expected: no matches.

Inspect route generation/build output to confirm `/login` remains registered and no generated route changes are missing.

**Step 6: Run the critical browser flow when its environment is available**

```bash
cd web && pnpm test:e2e
```

Expected: first-run registration or existing-user sign-in succeeds, protected workspace loads, and the login page is standalone. If the external E2E server/environment is unavailable, record that specific limitation rather than claiming the browser flow passed.

**Step 7: Review the final diff and commit only necessary fixes**

Use `git diff` and `git status` to verify no `.env`, database, build output, or unrelated files are staged. If final verification required code changes, commit each logical fix separately; do not squash the preceding implementation commits.
