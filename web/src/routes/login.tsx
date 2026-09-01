import { useMutation } from "@tanstack/solid-query";
import { createFileRoute, useNavigate } from "@tanstack/solid-router";
import { Show, createSignal } from "solid-js";

import { RequestError } from "../components/request-error";
import { Button } from "../components/ui/button";
import { FormField } from "../components/ui/form-field";
import { PageHeader } from "../components/ui/page-header";
import { api } from "../lib/api";

export const Route = createFileRoute("/login")({
  component: LoginPage,
});

function LoginPage() {
  const navigate = useNavigate();
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const login = useMutation(() => ({
    mutationFn: () => api.login({ username: username().trim(), password: password() }),
    onSuccess: async () => {
      await navigate({ to: "/" });
    },
  }));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (username().trim() && password()) login.mutate();
  }

  return (
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        eyebrow="Session / Single user"
        title="Unlock workspace"
        description="Authenticate to create jobs and approve filesystem changes. The password is sent only to this Fixer server."
      />

      <section
        class="mt-12 grid grid-cols-[minmax(220px,0.7fr)_minmax(300px,1.3fr)] gap-[clamp(2rem,7vw,7rem)] border-t-2 border-ink py-10 max-[700px]:grid-cols-1"
        aria-labelledby="login-panel-title"
      >
        <div>
          <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            Protected operations
          </p>
          <h2
            class="m-0 font-serif text-2xl font-medium"
            id="login-panel-title"
          >
            Start a server session
          </h2>
          <p class="mt-3 mb-0 text-sm text-muted">
            Your session cookie is HTTP-only. CSRF state stays in this browser
            tab and is cleared when the session ends.
          </p>
        </div>
        <form class="grid content-start gap-5" onSubmit={submit}>
          <FormField label="Username">
            <input
              type="text"
              required
              autocomplete="username"
              value={username()}
              disabled={login.isPending}
              onInput={(event) => setUsername(event.currentTarget.value)}
            />
          </FormField>
          <FormField label="Password">
            <input
              type="password"
              required
              autocomplete="current-password"
              value={password()}
              disabled={login.isPending}
              onInput={(event) => setPassword(event.currentTarget.value)}
            />
          </FormField>
          <Button
            class="justify-self-start max-[700px]:w-full"
            type="submit"
            disabled={login.isPending || !username().trim() || !password()}
          >
            {login.isPending ? "Signing in…" : "Sign in"}
          </Button>
          <Show when={login.isError}>
            <RequestError error={login.error} fallback="Sign-in failed" />
          </Show>
        </form>
      </section>
    </div>
  );
}
