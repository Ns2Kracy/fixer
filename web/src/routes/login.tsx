import { Show, createSignal } from "solid-js";
import { useMutation } from "@tanstack/solid-query";
import { createFileRoute, useNavigate } from "@tanstack/solid-router";

import { RequestError } from "../components/request-error";
import { api } from "../lib/api";

export const Route = createFileRoute("/login")({
  component: LoginPage,
});

function LoginPage() {
  const navigate = useNavigate();
  const [password, setPassword] = createSignal("");
  const login = useMutation(() => ({
    mutationFn: () => api.login({ password: password() }),
    onSuccess: async () => {
      await navigate({ to: "/" });
    },
  }));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (password()) login.mutate();
  }

  return (
    <div class="workspace-page login-page">
      <header class="workspace-heading">
        <div>
          <p class="eyebrow">Session / Single user</p>
          <h1>Unlock workspace</h1>
        </div>
        <p>
          Authenticate to create jobs and approve filesystem changes. The
          password is sent only to this Fixer server.
        </p>
      </header>

      <section class="login-panel" aria-labelledby="login-panel-title">
        <div>
          <p class="eyebrow">Protected operations</p>
          <h2 id="login-panel-title">Start a server session</h2>
          <p>
            Your session cookie is HTTP-only. CSRF state stays in this browser
            tab and is cleared when the session ends.
          </p>
        </div>
        <form onSubmit={submit}>
          <label>
            <span>Workspace password</span>
            <input
              type="password"
              required
              autocomplete="current-password"
              value={password()}
              disabled={login.isPending}
              onInput={(event) => setPassword(event.currentTarget.value)}
            />
          </label>
          <button
            class="button primary"
            type="submit"
            disabled={login.isPending || !password()}
          >
            {login.isPending ? "Signing in…" : "Sign in"}
          </button>
          <Show when={login.isError}>
            <RequestError error={login.error} fallback="Sign-in failed" />
          </Show>
        </form>
      </section>
    </div>
  );
}
