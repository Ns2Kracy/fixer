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

function authStatus(registrationRequired: boolean) {
  return json({
    schema_version: 1,
    registration_required: registrationRequired,
    authenticated: false,
    username: null,
  });
}

function renderApp(initialEntry = "/login") {
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

describe("administrator authentication", () => {
  it("defaults to Sign up when the administrator has not been registered", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        if (String(input) === "/api/v1/auth/status") return authStatus(true);
        throw new Error(`Unexpected request: ${String(input)}`);
      }),
    );

    renderApp();

    expect(
      await screen.findByRole("tab", { name: "Sign up" }),
    ).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Sign in" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
    expect(screen.getByLabelText("Username")).toBeVisible();
    expect(screen.getByLabelText("Password")).toHaveAttribute(
      "autocomplete",
      "new-password",
    );
    expect(screen.getByLabelText("Confirm password")).toBeVisible();
    expect(
      screen.queryByRole("complementary", { name: "Workspace navigation" }),
    ).not.toBeInTheDocument();
  });

  it("signs in from the Sign in tab and enters the workspace", async () => {
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL, _init?: RequestInit) => {
        const url = String(input);
        if (url === "/api/v1/auth/status") return authStatus(true);
        if (url === "/api/v1/auth/login") {
          return json({
            schema_version: 1,
            username: "admin",
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
    renderApp();

    await user.click(await screen.findByRole("tab", { name: "Sign in" }));
    await user.type(screen.getByLabelText("Username"), "admin");
    await user.type(
      screen.getByLabelText("Password"),
      "local-development-password",
    );
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/v1/auth/login",
        expect.objectContaining({
          method: "POST",
          credentials: "same-origin",
          body: JSON.stringify({
            username: "admin",
            password: "local-development-password",
          }),
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

  it("blocks registration when password confirmation does not match", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/auth/status") return authStatus(true);
      throw new Error(`Unexpected request: ${String(input)}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp();

    await screen.findByRole("tab", { name: "Sign up" });
    await user.type(screen.getByLabelText("Username"), "admin");
    await user.type(screen.getByLabelText("Password"), "first password");
    await user.type(
      screen.getByLabelText("Confirm password"),
      "different password",
    );
    await user.click(
      screen.getByRole("button", { name: "Create administrator" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Passwords do not match",
    );
    expect(fetchMock).not.toHaveBeenCalledWith(
      "/api/v1/auth/register",
      expect.anything(),
    );
  });

  it("registers the first administrator and enters the workspace", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/v1/auth/status") return authStatus(true);
      if (url === "/api/v1/auth/register") {
        return json({
          schema_version: 1,
          username: "admin",
          csrf_token: "csrf-registration",
          expires_at_ms: 4_102_444_800_000,
        });
      }
      if (url === "/api/v1/health") {
        return json({ schema_version: 1, status: "ok", version: "0.1.0" });
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp();

    await screen.findByRole("tab", { name: "Sign up" });
    await user.type(screen.getByLabelText("Username"), "admin");
    await user.type(
      screen.getByLabelText("Password"),
      "local-development-password",
    );
    await user.type(
      screen.getByLabelText("Confirm password"),
      "local-development-password",
    );
    await user.click(
      screen.getByRole("button", { name: "Create administrator" }),
    );

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/v1/auth/register",
        expect.objectContaining({
          method: "POST",
          credentials: "same-origin",
          body: JSON.stringify({
            username: "admin",
            password: "local-development-password",
          }),
        }),
      ),
    );
    expect(sessionStorage.getItem("fixer.csrf-token")).toBe(
      "csrf-registration",
    );
    expect(
      await screen.findByRole("heading", {
        name: "Metadata work, without guesswork.",
      }),
    ).toBeVisible();
  });

  it("defaults to Sign in and closes Sign up after registration", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        if (String(input) === "/api/v1/auth/status") return authStatus(false);
        throw new Error(`Unexpected request: ${String(input)}`);
      }),
    );

    renderApp();

    expect(
      await screen.findByRole("tab", { name: "Sign in" }),
    ).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Sign up" })).toBeDisabled();
  });
});
