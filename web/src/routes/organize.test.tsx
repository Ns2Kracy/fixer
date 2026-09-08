import { QueryClient } from "@tanstack/solid-query";
import { createMemoryHistory } from "@tanstack/solid-router";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../app";
import { createAppRouter } from "../router";
import { render, screen, waitFor } from "../test/render";
import { within } from "@testing-library/dom";

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
        if (url.startsWith("/api/v1/jobs"))
          return Response.json({
            schema_version: 1,
            jobs: [
              {
                id: 1,
                state: "awaiting_confirmation",
                input: {
                  input_path: "Videos/赛博朋克 (2022)",
                  media_kind: "television",
                  apply: true,
                },
                created_at_ms: 1,
                updated_at_ms: 2,
              },
              {
                id: 2,
                state: "completed",
                input: {
                  input_path: "Videos/Arrival (2016)",
                  media_kind: "movie",
                  apply: true,
                },
                created_at_ms: 1,
                updated_at_ms: 2,
              },
            ],
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

describe("organize workflow", () => {
  it("starts from one selection and only asks for media kind on explicit ambiguity", async () => {
    const user = userEvent.setup();
    let creationCount = 0;
    const fetcher = vi.fn(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        if (url === "/api/v1/ingestion-rules") {
          // The server may know about a rule added after this cached response.
          return Response.json({ schema_version: 1, rules: [] });
        }
        if (url === "/api/v1/library/roots") {
          return Response.json({
            schema_version: 1,
            roots: [{ id: "root-media", label: "Media" }],
          });
        }
        if (url.startsWith("/api/v1/library?")) {
          const path =
            new URL(url, "https://fixer.test").searchParams.get("path") ?? "";
          const entries =
            path === ""
              ? [{ name: "Videos", path: "Videos", kind: "directory" }]
              : path === "Videos"
                ? [
                    {
                      name: "Cyberpunk",
                      path: "Videos/Cyberpunk",
                      kind: "directory",
                    },
                  ]
                : [];
          return Response.json({
            schema_version: 1,
            root_id: "root-media",
            path,
            entries,
            truncated: false,
          });
        }
        if (url === "/api/v1/jobs" && init?.method === "POST") {
          creationCount += 1;
          if (creationCount === 1) {
            return Response.json(
              {
                error: {
                  code: "ambiguous_media_kind",
                  message: "Choose the media kind for the selected work",
                },
              },
              { status: 422 },
            );
          }
          return Response.json(
            {
              schema_version: 1,
              job: {
                id: 9,
                state: "queued",
                input: {
                  input_path: "Videos/Cyberpunk",
                  media_kind: "television",
                  apply: true,
                },
                created_at_ms: 1,
                updated_at_ms: 1,
              },
            },
            { status: 202 },
          );
        }
        if (url === "/api/v1/jobs/9") {
          return Response.json({
            schema_version: 1,
            job: {
              id: 9,
              state: "queued",
              input: {
                input_path: "Videos/Cyberpunk",
                media_kind: "television",
                apply: true,
              },
              created_at_ms: 1,
              updated_at_ms: 1,
            },
          });
        }
        if (url.startsWith("/api/v1/jobs")) {
          return Response.json({
            schema_version: 1,
            jobs: [],
            has_more: false,
          });
        }
        if (url === "/api/v1/health") {
          return Response.json({
            schema_version: 1,
            status: "ok",
            version: "test",
          });
        }
        return Response.json({ schema_version: 1 });
      },
    );
    mount("/", fetcher);

    expect(
      await screen.findByRole("button", { name: "选择目录并整理" }),
    ).toBeVisible();
    expect(screen.queryByLabelText("Media kind")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "选择目录并整理" }));
    await user.click(await screen.findByRole("button", { name: "Media" }));
    await user.click(screen.getByRole("button", { name: "Videos" }));
    await user.click(screen.getByRole("button", { name: "Cyberpunk" }));
    await user.click(
      screen.getByRole("button", { name: "Select this folder" }),
    );

    await waitFor(() => {
      const request = fetcher.mock.calls.find(
        ([url, options]) =>
          String(url) === "/api/v1/jobs" && options?.method === "POST",
      );
      expect(JSON.parse(String(request?.[1]?.body))).toEqual({
        rule: "matching",
        source: { root_id: "root-media", path: "Videos/Cyberpunk" },
      });
    });

    await screen.findByLabelText("Media kind");
    expect(screen.queryByText("Destination folder")).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("Media kind"), "television");
    await user.click(screen.getByRole("button", { name: "开始识别" }));

    await waitFor(() => {
      const requests = fetcher.mock.calls.filter(
        ([url, options]) =>
          String(url) === "/api/v1/jobs" && options?.method === "POST",
      );
      expect(JSON.parse(String(requests.at(1)?.[1]?.body))).toEqual({
        rule: "matching",
        source: { root_id: "root-media", path: "Videos/Cyberpunk" },
        media_kind: "television",
      });
    });
  });

  it("does not treat a rule-loading failure as an unmatched folder", async () => {
    const fetcher = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/v1/ingestion-rules") {
        return Response.json(
          {
            error: {
              code: "rule_list_unavailable",
              message: "Folder rules could not be loaded",
            },
          },
          { status: 500 },
        );
      }
      if (url.startsWith("/api/v1/jobs")) {
        return Response.json({ schema_version: 1, jobs: [], has_more: false });
      }
      if (url === "/api/v1/health") {
        return Response.json({
          schema_version: 1,
          status: "ok",
          version: "test",
        });
      }
      return Response.json({ schema_version: 1 });
    });
    mount("/", fetcher);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Folder rules could not be loaded",
    );
    expect(
      screen.queryByRole("button", { name: "选择目录并整理" }),
    ).not.toBeInTheDocument();
  });
});

describe("work-centred navigation", () => {
  it("exposes only organize, records and settings as primary destinations", async () => {
    mount("/");
    await screen.findByRole("heading", { name: "整理" });
    const nav = screen.getByRole("navigation", { name: "主导航" });
    expect(
      within(nav)
        .getAllByRole("link")
        .map((link) => link.textContent?.trim()),
    ).toEqual(["整理", "整理记录", "设置"]);
    expect(await screen.findByText("赛博朋克 (2022)")).toBeVisible();
    expect(screen.queryByText("Arrival (2016)")).not.toBeInTheDocument();
    const options = screen
      .getAllByRole("option")
      .map((option) => option.textContent);
    expect(options).toContain("Queued");
    expect(options).not.toContain("Completed");
  });
  it("shows finished records without a create form", async () => {
    mount("/jobs");
    await screen.findByRole("heading", { name: "整理记录" });
    expect(await screen.findByText("Arrival (2016)")).toBeVisible();
    expect(screen.queryByText("赛博朋克 (2022)")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "开始识别" }),
    ).not.toBeInTheDocument();
    const options = screen
      .getAllByRole("option")
      .map((option) => option.textContent);
    expect(options).toContain("Completed");
    expect(options).not.toContain("Queued");
  });
  it("keeps Settings selected while visiting its configuration tools", async () => {
    mount("/library");
    await screen.findByRole("navigation", { name: "设置分类" });
    const primary = screen.getByRole("navigation", { name: "主导航" });
    await waitFor(() => {
      expect(
        within(primary).getByRole("link", { name: "设置" }),
      ).toHaveAttribute("aria-current", "page");
    });
  });
});
