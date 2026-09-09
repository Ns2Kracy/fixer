import { QueryClient } from "@tanstack/solid-query";
import { createMemoryHistory } from "@tanstack/solid-router";
import userEvent from "@testing-library/user-event";
import { within } from "@testing-library/dom";
import { describe, expect, it, vi } from "vitest";

import { App } from "../app";
import { createAppRouter } from "../router";
import { render, screen, waitFor } from "../test/render";

function run(id = 4) {
  return {
    id,
    item_name: "Arrival (2016)",
    media_kind: "movie",
    status: "succeeded",
    selected_target: {
      media_kind: "movie",
      provider: "tmdb",
      external_id: { namespace: "tmdb", value: "329865" },
    },
    candidate_count: 2,
    conflict_count: 0,
    created_at_ms: 1,
    updated_at_ms: 2,
  };
}

function mount(path: string, fetcher?: typeof fetch) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  queryClient.setQueryData(["auth", "status"], {
    schema_version: 1,
    authenticated: true,
    registration_required: false,
    username: "admin",
  });
  vi.stubGlobal(
    "fetch",
    fetcher ??
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/v1/scrape-runs?limit=50")
          return Response.json({
            schema_version: 1,
            runs: [run()],
            has_more: false,
          });
        if (url === "/api/v1/health")
          return Response.json({
            schema_version: 1,
            status: "ok",
            version: "test",
          });
        return Response.json({ schema_version: 1, roots: [] });
      }),
  );
  const router = createAppRouter({
    history: createMemoryHistory({ initialEntries: [path] }),
    queryClient,
  });
  return render(() => <App queryClient={queryClient} router={router} />);
}

describe("scrape workflow", () => {
  it("creates one background scrape from an opaque directory selection", async () => {
    const user = userEvent.setup();
    const fetcher = vi.fn(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        if (url === "/api/v1/scrape-runs?limit=50")
          return Response.json({
            schema_version: 1,
            runs: [],
            has_more: false,
          });
        if (url === "/api/v1/library/roots")
          return Response.json({
            schema_version: 1,
            roots: [{ id: "root-media", label: "Media" }],
          });
        if (url.startsWith("/api/v1/library?")) {
          const path =
            new URL(url, "https://fixer.test").searchParams.get("path") ?? "";
          return Response.json({
            schema_version: 1,
            root_id: "root-media",
            path,
            entries:
              path === ""
                ? [
                    {
                      name: "Arrival (2016)",
                      path: "Arrival (2016)",
                      kind: "directory",
                    },
                  ]
                : [],
            truncated: false,
          });
        }
        if (url === "/api/v1/scrape-runs" && init?.method === "POST")
          return Response.json(
            { schema_version: 1, run: run(9) },
            { status: 202 },
          );
        if (url === "/api/v1/scrape-runs/9")
          return Response.json({ schema_version: 1, run: run(9) });
        throw new Error(`Unexpected request: ${url}`);
      },
    );
    mount("/", fetcher);

    expect(
      await screen.findByRole("heading", { name: "刮削，然后等结果。" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "选择目录" }));
    await user.click(await screen.findByRole("button", { name: "Media" }));
    await user.click(screen.getByRole("button", { name: "Arrival (2016)" }));
    await user.click(
      screen.getByRole("button", { name: "Select this folder" }),
    );
    await user.click(screen.getByRole("button", { name: "开始刮削" }));

    await waitFor(() => {
      const request = fetcher.mock.calls.find(
        ([url, options]) =>
          String(url) === "/api/v1/scrape-runs" && options?.method === "POST",
      );
      expect(JSON.parse(String(request?.[1]?.body))).toEqual({
        media_kind: "movie",
        source: { root_id: "root-media", path: "Arrival (2016)" },
      });
    });
    expect(
      await screen.findByRole("heading", { name: "Arrival (2016)" }),
    ).toBeVisible();
  });

  it("sends an exact provider identity instead of a search hint", async () => {
    const user = userEvent.setup();
    const fetcher = vi.fn(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        if (url === "/api/v1/scrape-runs?limit=50")
          return Response.json({
            schema_version: 1,
            runs: [],
            has_more: false,
          });
        if (url === "/api/v1/library/roots")
          return Response.json({
            schema_version: 1,
            roots: [{ id: "r", label: "Media" }],
          });
        if (url.startsWith("/api/v1/library?"))
          return Response.json({
            schema_version: 1,
            root_id: "r",
            path: "",
            entries: [],
            truncated: false,
          });
        if (url === "/api/v1/scrape-runs" && init?.method === "POST")
          return Response.json(
            { schema_version: 1, run: run(9) },
            { status: 202 },
          );
        if (url === "/api/v1/scrape-runs/9")
          return Response.json({ schema_version: 1, run: run(9) });
        throw new Error(`Unexpected request: ${url}`);
      },
    );
    mount("/", fetcher);

    await screen.findByRole("heading", { name: "刮削，然后等结果。" });
    await user.click(screen.getByRole("button", { name: "选择目录" }));
    await user.click(await screen.findByRole("button", { name: "Media" }));
    await user.click(
      screen.getByRole("button", { name: "Select this folder" }),
    );
    await user.click(screen.getByRole("checkbox"));
    await user.type(screen.getByLabelText("Provider ID"), "329865");
    await user.click(screen.getByRole("button", { name: "开始刮削" }));

    await waitFor(() => {
      const request = fetcher.mock.calls.find(
        ([url, options]) =>
          String(url) === "/api/v1/scrape-runs" && options?.method === "POST",
      );
      expect(JSON.parse(String(request?.[1]?.body))).toEqual({
        media_kind: "movie",
        source: { root_id: "r", path: "" },
        target: {
          media_kind: "movie",
          provider: "tmdb",
          external_id: { namespace: "tmdb", value: "329865" },
        },
      });
    });
  });

  it("blocks unsupported TMDB media kinds before submission", async () => {
    const user = userEvent.setup();
    const fetcher = vi.fn(
      async (input: RequestInfo | URL, _init?: RequestInit) => {
        const url = String(input);
        if (url === "/api/v1/scrape-runs?limit=50")
          return Response.json({
            schema_version: 1,
            runs: [],
            has_more: false,
          });
        if (url === "/api/v1/library/roots")
          return Response.json({
            schema_version: 1,
            roots: [{ id: "r", label: "Media" }],
          });
        if (url.startsWith("/api/v1/library?"))
          return Response.json({
            schema_version: 1,
            root_id: "r",
            path: "",
            entries: [],
            truncated: false,
          });
        throw new Error(`Unexpected request: ${url}`);
      },
    );
    mount("/", fetcher);

    await screen.findByRole("heading", { name: "刮削，然后等结果。" });
    await user.selectOptions(screen.getByLabelText("媒体类型"), "anime");
    await user.click(screen.getByRole("button", { name: "选择目录" }));
    await user.click(await screen.findByRole("button", { name: "Media" }));
    await user.click(
      screen.getByRole("button", { name: "Select this folder" }),
    );
    await user.click(screen.getByRole("checkbox"));
    await user.type(screen.getByLabelText("Provider ID"), "123");

    expect(
      screen.getByText("TMDB 仅支持电影或剧集，并且 ID 必须是大于零的数字。"),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "开始刮削" })).toBeDisabled();
    expect(
      fetcher.mock.calls.some(
        ([url, options]) =>
          String(url) === "/api/v1/scrape-runs" && options?.method === "POST",
      ),
    ).toBe(false);
  });
});

describe("scrape-centred navigation", () => {
  it("shows scrape, audit, and settings as the primary destinations", async () => {
    mount("/");
    await screen.findByRole("heading", { name: "刮削，然后等结果。" });
    const nav = screen.getByRole("navigation", { name: "主导航" });
    expect(
      within(nav)
        .getAllByRole("link")
        .map((link) => link.textContent?.trim()),
    ).toEqual(["刮削", "刮削审计", "设置"]);
    expect(screen.queryByText(/job/iu)).not.toBeInTheDocument();
    const title = await screen.findByText("Arrival (2016)");
    expect(title).toBeVisible();
    expect(within(title.closest("article")!).getByText(/tmdb/iu)).toBeVisible();
  });

  it("renders persisted failure phase, code, and message in the audit", async () => {
    const failed = {
      ...run(),
      status: "failed",
      execution: {
        schema_version: 1,
        completed_operations: 0,
        failed_operations: 0,
        failure: {
          schema_version: 1,
          phase: "searching",
          code: "search_failed",
          message:
            "Fixer could not retrieve metadata from the selected provider",
        },
      },
    };
    mount(
      "/scrapes/4",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/v1/scrape-runs/4")
          return Response.json({ schema_version: 1, run: failed });
        if (url === "/api/v1/health")
          return Response.json({
            schema_version: 1,
            status: "ok",
            version: "test",
          });
        throw new Error(`Unexpected request: ${url}`);
      }),
    );

    expect(
      await screen.findByRole("heading", { name: "Arrival (2016)" }),
    ).toBeVisible();
    const failure = await screen.findByRole("alert");
    expect(
      within(failure).getByText(
        "Fixer could not retrieve metadata from the selected provider",
      ),
    ).toBeVisible();
    expect(
      within(failure).getByText("searching · search_failed"),
    ).toBeVisible();
  });
});
