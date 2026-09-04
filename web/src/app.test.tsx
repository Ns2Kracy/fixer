import { render, screen, waitFor } from "./test/render";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/solid-query";
import { createMemoryHistory } from "@tanstack/solid-router";
import { describe, expect, it, vi } from "vitest";

import { App } from "./app";
import { createAppRouter } from "./router";

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function authenticatedFetch(
  handler: (url: string) => Response | Promise<Response>,
) {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url === "/api/v1/auth/status") {
      return json({
        schema_version: 1,
        registration_required: false,
        authenticated: true,
        username: "admin",
      });
    }
    return handler(url);
  });
}

function renderApp(initialEntry = "/") {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });
  const router = createAppRouter({
    history: createMemoryHistory({ initialEntries: [initialEntry] }),
    queryClient,
  });

  return render(() => <App queryClient={queryClient} router={router} />);
}

describe("Fixer app", () => {
  it("redirects unauthenticated visits before loading protected data", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/auth/status") {
        return json({
          schema_version: 1,
          registration_required: false,
          authenticated: false,
          username: null,
        });
      }
      throw new Error(`Unexpected request: ${String(input)}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    renderApp();

    expect(
      await screen.findByRole("heading", { name: "Fixer access" }),
    ).toBeVisible();
    expect(fetchMock).not.toHaveBeenCalledWith(
      "/api/v1/health",
      expect.anything(),
    );
  });

  it("mounts the Overview dashboard", async () => {
    vi.stubGlobal(
      "fetch",
      authenticatedFetch((url) => {
        if (url === "/api/v1/health") {
          return json({ schema_version: 1, status: "ok", version: "0.1.0" });
        }
        throw new Error(`Unexpected request: ${url}`);
      }),
    );

    renderApp();

    expect(
      await screen.findByRole("heading", {
        name: "Overview",
      }),
    ).toBeVisible();
    expect(await screen.findByText("Server connected")).toBeVisible();
    expect(
      screen.queryByRole("link", { name: "Sign in" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: "Manage folders" }),
    ).toHaveAttribute("href", "/folders");
    expect(screen.getByRole("link", { name: "Overview" })).toHaveAttribute(
      "href",
      "/",
    );
    expect(screen.getByRole("link", { name: "Folders" })).toHaveAttribute(
      "href",
      "/folders",
    );
    expect(document.body).not.toHaveTextContent(/workspace/iu);
  });

  it("recovers from an unknown route through client-side navigation", async () => {
    vi.stubGlobal(
      "fetch",
      authenticatedFetch((url) => {
        if (url === "/api/v1/health") {
          return json({ schema_version: 1, status: "ok", version: "0.1.0" });
        }
        throw new Error(`Unexpected request: ${url}`);
      }),
    );
    const user = userEvent.setup();
    renderApp("/missing");

    expect(
      await screen.findByRole("heading", { name: "Page not found" }),
    ).toBeVisible();
    const overviewLink = screen.getByRole("link", {
      name: "Return to Overview",
    });
    overviewLink.focus();
    await user.keyboard("{Enter}");

    expect(
      await screen.findByRole("heading", {
        name: "Overview",
      }),
    ).toBeVisible();
  });

  it("shows the structured API error instead of a generic failure", async () => {
    vi.stubGlobal(
      "fetch",
      authenticatedFetch((url) => {
        if (url === "/api/v1/health") {
          return json(
            {
              error: {
                code: "authentication_required",
                message: "Authentication is required",
                request_id: "req-0000000000000001",
              },
            },
            401,
          );
        }
        throw new Error(`Unexpected request: ${url}`);
      }),
    );

    renderApp();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Authentication is required");
    expect(alert).toHaveTextContent("req-0000000000000001");
  });

  it("signs out from the header and returns to the login page", async () => {
    sessionStorage.setItem("fixer.csrf-token", "csrf-sign-out");
    const fetchMock = authenticatedFetch((url) => {
      if (url === "/api/v1/health") {
        return json({ schema_version: 1, status: "ok", version: "0.1.0" });
      }
      if (url === "/api/v1/auth/logout")
        return new Response(null, { status: 204 });
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp();

    await screen.findByRole("heading", {
      name: "Overview",
    });
    await user.click(screen.getByRole("button", { name: "Sign out" }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/v1/auth/logout",
        expect.objectContaining({
          method: "POST",
          headers: expect.objectContaining({ "x-csrf-token": "csrf-sign-out" }),
        }),
      );
    });
    expect(
      await screen.findByRole("heading", { name: "Fixer access" }),
    ).toBeVisible();
    expect(sessionStorage.getItem("fixer.csrf-token")).toBeNull();
  });

  it("offers a keyboard skip link that moves focus to main content", async () => {
    vi.stubGlobal(
      "fetch",
      authenticatedFetch((url) => {
        if (url === "/api/v1/health") {
          return json({ schema_version: 1, status: "ok", version: "0.1.0" });
        }
        throw new Error(`Unexpected request: ${url}`);
      }),
    );
    const user = userEvent.setup();
    renderApp();

    await screen.findByRole("heading", {
      name: "Overview",
    });
    await user.tab();
    expect(screen.getByRole("link", { name: "Skip to content" })).toHaveFocus();
    await user.keyboard("{Enter}");

    await waitFor(() => expect(screen.getByRole("main")).toHaveFocus());
  });
});
