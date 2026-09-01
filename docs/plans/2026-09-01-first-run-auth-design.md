# First-run registration and login design

## Goal

Replace the startup environment-variable password with a first-run administrator registration flow. Authentication must live outside the application shell: unauthenticated users see a dedicated login page, not a `Sign in` item in the sidebar.

## Scope

- Keep Fixer single-user.
- Create exactly one administrator account with a username and password.
- Remove the `FIXER_SERVER_PASSWORD` startup requirement and startup password replacement.
- Preserve Argon2id password hashing, HttpOnly session cookies, CSRF protection, bearer-token support, and the existing media-root security boundary.

## User flow

1. An unauthenticated browser requests an application route.
2. The router checks public authentication status and redirects to `/login`, preserving the intended destination.
3. If no administrator exists, `/login` defaults to `Sign up`.
4. Registration accepts a username, password, and password confirmation. A successful registration creates the sole administrator and immediately issues a browser session.
5. If an administrator already exists, `/login` defaults to `Sign in`; further registration attempts are rejected.
6. Sign in accepts the configured username and password and returns the browser to its intended route, or `/` when no destination was preserved.
7. Authenticated users see the application shell. The sidebar has no `Sign in` destination; a `Sign out` action ends the session and returns to `/login`.

## Server design

Expose public authentication endpoints under `/api/v1/auth`:

- `GET /status` reports whether registration is required and whether the current browser session is authenticated.
- `POST /register` atomically creates the first administrator, hashes its password in a blocking worker, and issues a session. It returns a conflict once an administrator exists.
- `POST /login` accepts username and password and retains the generic invalid-credentials response.
- The existing logout endpoint remains responsible for session revocation.

The single-user authentication record stores the administrator username beside the password hash. Registration uses a database transaction or an equivalent conditional insert so concurrent requests cannot create two administrators.

Production startup no longer reads or requires `FIXER_SERVER_PASSWORD` and no longer rewrites the stored password hash. Media-root, origin, bind, and database validation remain unchanged.

Validation:

- Username: 3 to 64 characters.
- Password: 8 to 1024 bytes.
- Password confirmation is a client concern; the server receives only username and password.

## Web design

`/login` is rendered without `AppShell`. It contains accessible `Sign in` and `Sign up` modes, but registration is available only while the server reports that no administrator exists.

A centralized route guard checks authentication before protected routes render. It redirects unauthenticated users before application data queries run. Successful authentication invalidates the cached auth status and navigates to the preserved destination.

The application sidebar removes `Sign in` and exposes `Sign out` only for an authenticated session.

## Error handling

- Malformed registration fields return field-safe validation errors.
- Registration after initialization returns `409 Conflict` without changing the existing account.
- Incorrect username and password combinations return the same `invalid_credentials` response.
- Authentication status failures show a retryable error on the standalone login page rather than rendering the protected shell.
- Duplicate form submissions are disabled while requests are pending.

## Verification

Server tests prove:

- production startup works without `FIXER_SERVER_PASSWORD`;
- first registration succeeds and creates a session;
- concurrent or repeated registration cannot create another account;
- login requires the stored username and password;
- invalid credentials remain generic;
- authentication status reflects registration and session state.

Web tests prove:

- unauthenticated protected routes redirect to `/login`;
- `/login` renders without the sidebar;
- first-run state defaults to `Sign up`;
- initialized state defaults to `Sign in`;
- successful registration and login navigate into the application;
- the sidebar contains no `Sign in` item;
- sign out returns to `/login`.

The final verification pass runs targeted Rust and Web tests, type/lint/build checks, and the critical browser flow where available.
