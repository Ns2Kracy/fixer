import { QueryClient } from "@tanstack/solid-query";
import { createMemoryHistory } from "@tanstack/solid-router";
import { describe, expect, it, vi } from "vitest";

import { App } from "../../app";
import { createAppRouter } from "../../router";
import { render, screen } from "../../test/render";

function mount(path: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  queryClient.setQueryData(["auth", "status"], {
    schema_version: 1,
    authenticated: true,
    registration_required: false,
    username: "admin",
  });
  const fetcher = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url === "/api/v1/scrape-runs?limit=50") {
      return Response.json({ schema_version: 1, runs: [], has_more: false });
    }
    if (url === "/api/v1/health") {
      return Response.json({
        schema_version: 1,
        status: "ok",
        version: "test",
      });
    }
    throw new Error(`Unexpected request: ${url}`);
  });
  vi.stubGlobal("fetch", fetcher);
  const router = createAppRouter({
    history: createMemoryHistory({ initialEntries: [path] }),
    queryClient,
  });
  render(() => <App queryClient={queryClient} router={router} />);
  return fetcher;
}

describe("retired Job routes", () => {
  it.each(["/jobs", "/jobs/7", "/jobs/7/review", "/jobs/7/plan"])(
    "redirects %s to scrape audits without calling the Job API",
    async (path) => {
      const fetcher = mount(path);

      expect(
        await screen.findByRole("heading", { name: "刮削审计" }),
      ).toBeVisible();
      expect(
        fetcher.mock.calls.some(([url]) => String(url).includes("/jobs")),
      ).toBe(false);
      expect(screen.queryByText(/job/iu)).not.toBeInTheDocument();
    },
  );
});
