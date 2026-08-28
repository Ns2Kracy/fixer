import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/solid-query";
import { createMemoryHistory } from "@tanstack/solid-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../app";
import { createAppRouter } from "../router";
import { render, screen, waitFor } from "../test/render";

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function renderApp(initialEntry: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const router = createAppRouter({
    history: createMemoryHistory({ initialEntries: [initialEntry] }),
    queryClient,
  });
  return render(() => <App queryClient={queryClient} router={router} />);
}

beforeEach(() => {
  vi.unstubAllGlobals();
  sessionStorage.clear();
});

describe("server session login", () => {
  it("persists CSRF state and enters the workspace", async () => {
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL, _init?: RequestInit) => {
        const url = String(input);
        if (url === "/api/v1/auth/login") {
          return json({
            schema_version: 1,
            csrf_token: "csrf-browser-session",
            expires_at_ms: 4_102_444_800_000,
          });
        }
        if (url === "/api/v1/health") {
          return json({ schema_version: 1, status: "ok", version: "0.1.0" });
        }
        throw new Error(`Unexpected request: ${url}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp("/login");

    await user.type(
      await screen.findByLabelText("Workspace password"),
      "local-development-password",
    );
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/v1/auth/login",
        expect.objectContaining({
          method: "POST",
          credentials: "same-origin",
          body: JSON.stringify({ password: "local-development-password" }),
        }),
      ),
    );
    expect(sessionStorage.getItem("fixer.csrf-token")).toBe(
      "csrf-browser-session",
    );
    expect(
      await screen.findByRole("heading", {
        name: "Metadata work, without guesswork.",
      }),
    ).toBeVisible();
  });
});
