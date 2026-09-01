import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/solid-query";
import {
  createFileRoute,
  useNavigate,
  useRouter,
} from "@tanstack/solid-router";
import { Show, createSignal } from "solid-js";

import { RequestError } from "../components/request-error";
import { Button } from "../components/ui/button";
import { FormField } from "../components/ui/form-field";
import { LoadingState } from "../components/ui/loading-state";
import { PageHeader } from "../components/ui/page-header";
import { authStatusQuery } from "../lib/auth";
import { api } from "../lib/api";

interface LoginSearch {
  redirect?: string;
}

type AuthMode = "signin" | "signup";

export const Route = createFileRoute("/login")({
  validateSearch: (search: Record<string, unknown>): LoginSearch => {
    const redirect = search.redirect;
    return typeof redirect === "string" && redirect ? { redirect } : {};
  },
  component: LoginPage,
});

function LoginPage() {
  const navigate = useNavigate();
  const router = useRouter();
  const search = Route.useSearch();
  const queryClient = useQueryClient();
  const status = useQuery(authStatusQuery);
  const [selectedMode, setSelectedMode] = createSignal<AuthMode>();
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [confirmation, setConfirmation] = createSignal("");
  const [confirmationError, setConfirmationError] = createSignal<string>();
  const mode = (): AuthMode =>
    selectedMode() ??
    (status.data?.registration_required ? "signup" : "signin");
  const registrationClosed = () =>
    status.data !== undefined && !status.data.registration_required;

  const authenticate = useMutation(() => ({
    mutationFn: () => {
      const credentials = {
        username: username().trim(),
        password: password(),
      };
      return mode() === "signup"
        ? api.register(credentials)
        : api.login(credentials);
    },
    onSuccess: async (session) => {
      queryClient.setQueryData(["auth", "status"], {
        schema_version: 1,
        registration_required: false,
        authenticated: true,
        username: session.username,
      });
      const redirect = safeInternalRedirect(search().redirect);
      if (redirect) {
        router.history.push(redirect);
        return;
      }
      await navigate({ to: "/" });
    },
  }));

  function selectMode(nextMode: AuthMode) {
    if (nextMode === "signup" && registrationClosed()) return;
    setSelectedMode(nextMode);
    setConfirmationError(undefined);
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    setConfirmationError(undefined);
    if (!username().trim() || !password()) return;
    if (mode() === "signup" && password() !== confirmation()) {
      setConfirmationError("Passwords do not match");
      return;
    }
    authenticate.mutate();
  }

  return (
    <div class="mx-auto min-h-screen max-w-[1180px] px-[clamp(1rem,4vw,4rem)] py-[clamp(2rem,7vw,6rem)]">
      <PageHeader
        eyebrow="Administrator access"
        title="Unlock workspace"
        description="Create the single administrator on first use, then sign in to manage metadata jobs and approve filesystem changes."
      />

      <section
        class="mt-12 grid grid-cols-[minmax(220px,0.7fr)_minmax(300px,1.3fr)] gap-[clamp(2rem,7vw,7rem)] border-t-2 border-ink py-10 max-[700px]:grid-cols-1"
        aria-labelledby="login-panel-title"
      >
        <div>
          <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            Single administrator
          </p>
          <h2
            class="m-0 font-serif text-2xl font-medium"
            id="login-panel-title"
          >
            Secure this Fixer instance
          </h2>
          <p class="mt-3 mb-0 text-sm text-muted">
            Passwords are Argon2id-hashed. Browser sessions use an HTTP-only
            cookie, while CSRF state stays in this tab.
          </p>
        </div>

        <div class="min-w-0">
          <Show when={status.isPending}>
            <LoadingState>Checking administrator status…</LoadingState>
          </Show>
          <Show when={status.isError}>
            <RequestError
              error={status.error}
              fallback="Authentication status is unavailable"
            />
            <Button type="button" variant="secondary" onClick={() => status.refetch()}>
              Retry
            </Button>
          </Show>
          <Show when={status.isSuccess}>
            <div
              class="mb-8 flex gap-6 border-b border-line"
              role="tablist"
              aria-label="Authentication mode"
            >
              <button
                id="signin-tab"
                class={`border-b-2 px-1 py-3 text-sm font-semibold transition-colors ${
                  mode() === "signin"
                    ? "border-ink text-ink"
                    : "border-transparent text-muted hover:text-moss"
                }`}
                type="button"
                role="tab"
                aria-controls="authentication-panel"
                aria-selected={mode() === "signin" ? "true" : "false"}
                onClick={() => selectMode("signin")}
              >
                Sign in
              </button>
              <button
                id="signup-tab"
                class={`border-b-2 px-1 py-3 text-sm font-semibold transition-colors ${
                  mode() === "signup"
                    ? "border-ink text-ink"
                    : "border-transparent text-muted hover:text-moss"
                }`}
                type="button"
                role="tab"
                aria-controls="authentication-panel"
                aria-selected={mode() === "signup" ? "true" : "false"}
                disabled={registrationClosed()}
                onClick={() => selectMode("signup")}
              >
                Sign up
              </button>
            </div>

            <form
              id="authentication-panel"
              class="grid content-start gap-5"
              role="tabpanel"
              aria-labelledby={mode() === "signup" ? "signup-tab" : "signin-tab"}
              onSubmit={submit}
            >
              <FormField label="Username">
                <input
                  type="text"
                  required
                  minlength="3"
                  maxlength="64"
                  autocomplete="username"
                  value={username()}
                  disabled={authenticate.isPending}
                  onInput={(event) => setUsername(event.currentTarget.value)}
                />
              </FormField>
              <FormField label="Password">
                <input
                  type="password"
                  required
                  minlength={mode() === "signup" ? "8" : "1"}
                  maxlength="1024"
                  autocomplete={
                    mode() === "signup" ? "new-password" : "current-password"
                  }
                  value={password()}
                  disabled={authenticate.isPending}
                  onInput={(event) => setPassword(event.currentTarget.value)}
                />
              </FormField>
              <Show when={mode() === "signup"}>
                <FormField label="Confirm password">
                  <input
                    type="password"
                    required
                    minlength="8"
                    maxlength="1024"
                    autocomplete="new-password"
                    value={confirmation()}
                    disabled={authenticate.isPending}
                    onInput={(event) =>
                      setConfirmation(event.currentTarget.value)
                    }
                  />
                </FormField>
              </Show>
              <Button
                class="justify-self-start max-[700px]:w-full"
                type="submit"
                disabled={
                  authenticate.isPending ||
                  !username().trim() ||
                  !password() ||
                  (mode() === "signup" && !confirmation())
                }
              >
                {authenticate.isPending
                  ? mode() === "signup"
                    ? "Creating administrator…"
                    : "Signing in…"
                  : mode() === "signup"
                    ? "Create administrator"
                    : "Sign in"}
              </Button>
              <Show when={confirmationError()}>
                <p class="m-0 text-sm font-semibold text-coral" role="alert">
                  {confirmationError()}
                </p>
              </Show>
              <Show when={authenticate.isError}>
                <RequestError
                  error={authenticate.error}
                  fallback={
                    mode() === "signup"
                      ? "Administrator registration failed"
                      : "Sign-in failed"
                  }
                />
              </Show>
            </form>
          </Show>
        </div>
      </section>
    </div>
  );
}

function safeInternalRedirect(value: string | undefined): string | undefined {
  return value?.startsWith("/") && !value.startsWith("//") ? value : undefined;
}
