import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/solid-query";
import { createMemoryHistory } from "@tanstack/solid-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../app";
import { createAppRouter } from "../router";
import { render, screen, waitFor } from "../test/render";
import type { IngestionRuleDto, IngestionRuleRequest } from "../lib/api";

const watchingRule: IngestionRuleDto = {
  id: 7,
  name: "Incoming movies",
  source: { root_id: "root-media", path: "Downloads" },
  destination: { root_id: "root-media", path: "Movies" },
  media_kind_mode: { fixed: "movie" },
  placement: "copy",
  path_template_override: null,
  enabled: true,
  status: "needs_review",
  review_count: 1,
  last_error: null,
  created_at_ms: 1,
  updated_at_ms: 2,
};

const pausedRule: IngestionRuleDto = {
  ...watchingRule,
  id: 8,
  name: "Book drop",
  source: { root_id: "root-media", path: "Book drop" },
  destination: { root_id: "root-media", path: "Books" },
  media_kind_mode: "auto",
  placement: "hardlink",
  enabled: false,
  status: "paused",
  review_count: 0,
};

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function installApi(
  initialRules: IngestionRuleDto[] = [watchingRule, pausedRule],
  options: { failUpdate?: boolean; failList?: boolean } = {},
) {
  let rules = [...initialRules];
  let reviewResolved = false;
  const fetchMock = vi.fn<typeof fetch>(async (input, init) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    if (url === "/api/v1/ingestion-rules" && method === "GET") {
      if (options.failList === true) {
        return json(
          {
            error: {
              code: "unavailable",
              message: "Rules unavailable",
              request_id: "request-folders",
            },
          },
          503,
        );
      }
      return json({ schema_version: 1, rules });
    }
    if (url === "/api/v1/library/roots") {
      return json({
        schema_version: 1,
        roots: [{ id: "root-media", label: "Media" }],
      });
    }
    if (url.startsWith("/api/v1/library?")) {
      const query = new URL(url, "https://fixer.test").searchParams;
      const path = query.get("path") ?? "";
      return json({
        schema_version: 1,
        root_id: "root-media",
        path,
        entries:
          path === ""
            ? [
                { name: "Downloads", path: "Downloads", kind: "directory" },
                { name: "Movies", path: "Movies", kind: "directory" },
                { name: "movie.mkv", path: "movie.mkv", kind: "file" },
              ]
            : [],
        truncated: false,
      });
    }
    if (url === "/api/v1/templates/preview" && method === "POST") {
      return json({
        schema_version: 1,
        path: "Custom/Example Title",
        content: "Example Title",
        content_bytes: 13,
      });
    }
    if (url === "/api/v1/ingestion-rules" && method === "POST") {
      const request = JSON.parse(String(init?.body));
      const rule = {
        ...request,
        id: 9,
        status: "watching",
        review_count: 0,
        last_error: null,
        created_at_ms: 3,
        updated_at_ms: 3,
      } as IngestionRuleDto;
      rules = [...rules, rule];
      return json({ schema_version: 1, rule }, 201);
    }
    if (url === "/api/v1/ingestion-rules/7/reviews" && method === "GET") {
      return json({
        schema_version: 1,
        rule_id: 7,
        reviews: reviewResolved
          ? []
          : [
              {
                source_id: 41,
                relative_path: "Shared release",
                media_kinds: ["movie", "television"],
              },
            ],
      });
    }
    if (url === "/api/v1/ingestion-sources/41/resolve" && method === "POST") {
      reviewResolved = true;
      rules = rules.map((rule) =>
        rule.id === 7 ? { ...rule, status: "processing" } : rule,
      );
      return json({ schema_version: 1, source_id: 41, job_id: 73 }, 202);
    }
    const ruleMatch = url.match(/^\/api\/v1\/ingestion-rules\/(\d+)$/u);
    if (ruleMatch !== null && method === "PUT") {
      if (options.failUpdate === true) {
        return json(
          {
            error: {
              code: "invalid_directories",
              message: "Folder pair is invalid",
              details: { destination: "must not overlap source" },
              request_id: "request-overlap",
            },
          },
          422,
        );
      }
      const id = Number(ruleMatch[1]);
      const request: IngestionRuleRequest = JSON.parse(String(init?.body));
      const current = rules.find((rule) => rule.id === id)!;
      const rule = {
        ...current,
        ...request,
        status: request.enabled ? "watching" : "paused",
      } as IngestionRuleDto;
      rules = rules.map((item) => (item.id === id ? rule : item));
      return json({ schema_version: 1, rule });
    }
    if (ruleMatch !== null && method === "DELETE") {
      const id = Number(ruleMatch[1]);
      rules = rules.filter((rule) => rule.id !== id);
      return new Response(null, { status: 204 });
    }
    if (url.endsWith("/scan") && method === "POST") {
      return json({ schema_version: 1, rule_id: 7, requested: true }, 202);
    }
    throw new Error(`Unexpected request: ${method} ${url}`);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function renderApp() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  queryClient.setQueryData(["auth", "status"], {
    schema_version: 1,
    registration_required: false,
    authenticated: true,
    username: "admin",
  });
  const router = createAppRouter({
    history: createMemoryHistory({ initialEntries: ["/folders"] }),
    queryClient,
  });
  return render(() => <App queryClient={queryClient} router={router} />);
}

beforeEach(() => vi.restoreAllMocks());

describe("Folders route", () => {
  it("renders operational rows and runs edit, resume, scan, and delete actions", async () => {
    const user = userEvent.setup();
    const fetchMock = installApi();
    renderApp();

    expect(
      await screen.findByRole("heading", { name: "Folders" }),
    ).toBeVisible();
    const firstRule = (await screen.findByText("Incoming movies")).closest(
      "article",
    );
    expect(firstRule).toHaveTextContent("Downloads → Movies");
    expect(screen.getByText("Needs review")).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: "Review media type (1)" }),
    );
    expect(await screen.findByText("Shared release")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Use Television" }));
    await waitFor(() => {
      const call = fetchMock.mock.calls.find(
        ([url, init]) =>
          String(url) === "/api/v1/ingestion-sources/41/resolve" &&
          init?.method === "POST",
      );
      expect(JSON.parse(String(call?.[1]?.body))).toEqual({
        media_kind: "television",
      });
    });

    await user.click(screen.getAllByRole("button", { name: "Edit" })[0]!);
    expect(
      screen.getByRole("heading", { name: "Edit folder rule" }),
    ).toBeVisible();
    expect(screen.getByLabelText("Rule name")).toHaveValue("Incoming movies");
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    await user.click(screen.getByRole("button", { name: "Resume" }));
    await user.click(screen.getAllByRole("button", { name: "Scan now" })[0]!);
    await user.click(screen.getAllByRole("button", { name: "Delete" })[0]!);
    await waitFor(() => {
      expect(
        fetchMock.mock.calls.some(
          ([url, init]) => String(url).endsWith("/8") && init?.method === "PUT",
        ),
      ).toBe(true);
      expect(
        fetchMock.mock.calls.some(
          ([url, init]) =>
            String(url).endsWith("/7/scan") && init?.method === "POST",
        ),
      ).toBe(true);
      expect(
        fetchMock.mock.calls.some(
          ([url, init]) =>
            String(url).endsWith("/7") && init?.method === "DELETE",
        ),
      ).toBe(true);
    });
  });

  it("creates a rule from directory pickers and requires an organization method", async () => {
    const user = userEvent.setup();
    const fetchMock = installApi([]);
    renderApp();
    expect(
      await screen.findByRole("status", { name: "No folder rules" }),
    ).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Add folder rule" }));
    expect(screen.queryByLabelText(/source path/iu)).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Rule name"), "Movie intake");
    await user.selectOptions(screen.getByLabelText("Media detection"), "movie");

    await user.click(screen.getByRole("button", { name: "Choose source" }));
    await user.click(await screen.findByRole("button", { name: "Media" }));
    await user.click(screen.getByRole("button", { name: "Downloads" }));
    await user.click(
      screen.getByRole("button", { name: "Select this folder" }),
    );

    await user.click(
      screen.getByRole("button", { name: "Choose destination" }),
    );
    await user.click(await screen.findByRole("button", { name: "Media" }));
    await user.click(screen.getByRole("button", { name: "Movies" }));
    await user.click(
      screen.getByRole("button", { name: "Select this folder" }),
    );

    const save = screen.getByRole("button", { name: "Save rule" });
    expect(save).toBeDisabled();
    await user.selectOptions(
      screen.getByLabelText("Organization method"),
      "hardlink",
    );
    expect(save).toBeEnabled();

    const pathLayout = screen.getByLabelText("Path layout");
    expect(pathLayout).toHaveValue("recommended");
    await user.selectOptions(pathLayout, "id-title");
    expect(screen.getByText("{{ id }}/{{ title | sanitize }}")).toBeVisible();
    await user.selectOptions(pathLayout, "custom");
    expect(save).toBeDisabled();
    await user.click(screen.getByLabelText("Path template override"));
    await user.paste("Custom/{{ title | sanitize }}");
    expect(await screen.findByLabelText("Template preview")).toHaveTextContent(
      "Custom/Example Title",
    );
    await user.click(save);

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(
        ([url, init]) =>
          String(url) === "/api/v1/ingestion-rules" && init?.method === "POST",
      );
      expect(JSON.parse(String(call?.[1]?.body))).toEqual({
        name: "Movie intake",
        source: { root_id: "root-media", path: "Downloads" },
        destination: { root_id: "root-media", path: "Movies" },
        media_kind_mode: { fixed: "movie" },
        placement: "hardlink",
        path_template_override: "Custom/{{ title | sanitize }}",
        enabled: true,
      });
    });
  });

  it("keeps overlap validation inspectable while editing", async () => {
    const user = userEvent.setup();
    installApi([watchingRule], { failUpdate: true });
    renderApp();

    await screen.findByText("Incoming movies");
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Save rule" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Folder pair is invalid",
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "destination must not overlap source",
    );
  });

  it("renders list request errors", async () => {
    installApi([], { failList: true });
    renderApp();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Rules unavailable",
    );
  });
});
