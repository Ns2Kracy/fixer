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

const settings = {
  offline: false,
  proxy: null,
  preferred_locales: ["zh-Hans", "ja", "en"],
  timeout_seconds: 30,
  auto_accept_confidence: 0.9,
  review_confidence: 0.6,
  output_preset: "full",
  placement: "in_place",
  conflict_policy: "review",
  enabled_providers: ["local", "tmdb", "bangumi"],
  provider_endpoints: {
    tmdb: "https://api.themoviedb.org/3",
    bangumi: "https://api.bgm.tv",
    anilist: "https://graphql.anilist.co",
    musicbrainz: "https://musicbrainz.org/ws/2",
    openlibrary: "https://openlibrary.org",
    openlibrary_cover: "https://covers.openlibrary.org/b/",
  },
  secrets: {
    tmdb_api_token_configured: true,
    anilist_access_token_configured: false,
  },
};

beforeEach(() => vi.unstubAllGlobals());

describe("scraper workspace routes", () => {
  it("searches every supported media kind and renders root-relative matches", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.startsWith("/api/v1/search?")) {
        return json({
          schema_version: 1,
          media_kind: "book",
          results: [
            {
              root_id: "root-0",
              path: "Books/Fixture.epub",
              name: "Fixture.epub",
            },
          ],
          truncated: false,
        });
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp("/search");

    expect(
      await screen.findByRole("heading", { name: "Search every collection" }),
    ).toBeVisible();
    const kind = screen.getByLabelText("Media kind");
    expect(
      Array.from((kind as HTMLSelectElement).options).map(
        (option) => option.value,
      ),
    ).toEqual(["movie", "television", "anime", "music", "book"]);
    await user.selectOptions(kind, "book");
    await user.type(screen.getByLabelText("Search terms"), "fixture title");
    await user.click(screen.getByRole("button", { name: "Search library" }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "/api/v1/search?media_kind=book&query=fixture+title&limit=50",
        expect.any(Object),
      ),
    );
    expect(await screen.findByText("Books/Fixture.epub")).toBeVisible();
    expect(screen.getByText("root-0")).toBeVisible();
  });

  it("browses only server-issued roots and relative directory buttons", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/v1/library/roots") {
        return json({
          schema_version: 1,
          roots: [{ id: "root-0", label: "Media" }],
        });
      }
      if (url === "/api/v1/library?root_id=root-0&path=Books") {
        return json({
          schema_version: 1,
          root_id: "root-0",
          path: "Books",
          entries: [
            {
              name: "Fixture.epub",
              path: "Books/Fixture.epub",
              kind: "file",
              size_bytes: 7,
            },
          ],
          truncated: false,
        });
      }
      if (url === "/api/v1/library?root_id=root-0&path=") {
        return json({
          schema_version: 1,
          root_id: "root-0",
          path: "",
          entries: [{ name: "Books", path: "Books", kind: "directory" }],
          truncated: false,
        });
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp("/library");

    expect(
      await screen.findByRole("heading", { name: "Browse configured roots" }),
    ).toBeVisible();
    expect(
      await screen.findByRole("button", { name: "Open Books" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Root" })).toHaveAttribute(
      "aria-current",
      "location",
    );
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open Books" }));

    expect(await screen.findByText("Books/Fixture.epub")).toBeVisible();
    expect(screen.getByRole("button", { name: "Books" })).toHaveAttribute(
      "aria-current",
      "location",
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/library?root_id=root-0&path=Books",
      expect.any(Object),
    );
  });

  it("tests provider connectivity and renders the server safe category", async () => {
    let resolveProbe!: (response: Response) => void;
    const probeResponse = new Promise<Response>((resolve) => {
      resolveProbe = resolve;
    });
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/v1/providers") {
        return json({
          schema_version: 1,
          providers: [
            {
              id: "local",
              name: "Local files",
              media_kinds: ["movie", "book"],
              network: false,
              optional: false,
            },
            {
              id: "tmdb",
              name: "TMDB",
              media_kinds: ["movie", "television"],
              network: true,
              optional: true,
            },
          ],
        });
      }
      if (url === "/api/v1/settings")
        return json({ schema_version: 1, settings });
      if (url === "/api/v1/providers/tmdb/test") return probeResponse;
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp("/providers");

    expect(
      await screen.findByRole("heading", { name: "Provider readiness" }),
    ).toBeVisible();
    await user.click(await screen.findByRole("button", { name: "Test TMDB" }));

    expect(
      screen.getByRole("button", { name: "Test Local files" }),
    ).toBeDisabled();
    resolveProbe(
      json({
        schema_version: 1,
        provider: "tmdb",
        ok: true,
        category: "ready",
        message: "Provider endpoint is reachable",
      }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Provider endpoint is reachable",
    );
  });

  it("does not label providers disabled when settings are unavailable", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/v1/providers") {
        return json({
          schema_version: 1,
          providers: [
            {
              id: "local",
              name: "Local files",
              media_kinds: ["movie", "book"],
              network: false,
              optional: false,
            },
          ],
        });
      }
      if (url === "/api/v1/settings") {
        return json(
          {
            error: {
              code: "unavailable",
              message: "Settings unavailable",
              request_id: "req-settings",
            },
          },
          503,
        );
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    renderApp("/providers");

    expect(await screen.findByText("Settings unavailable")).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Test Local files" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Disabled")).not.toBeInTheDocument();
  });

  it("edits non-secret settings while keeping configured secrets write-only", async () => {
    let resolveUpdate!: (response: Response) => void;
    const updateResponse = new Promise<Response>((resolve) => {
      resolveUpdate = resolve;
    });
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        if (url === "/api/v1/settings" && init?.method === "PUT") {
          return updateResponse;
        }
        if (url === "/api/v1/settings")
          return json({ schema_version: 1, settings });
        throw new Error(`Unexpected request: ${url}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp("/settings");

    expect(
      await screen.findByRole("heading", { name: "Workspace settings" }),
    ).toBeVisible();
    const tmdbToken = await screen.findByLabelText("TMDB API token");
    expect(tmdbToken).toHaveValue("");
    expect(tmdbToken).toHaveAccessibleDescription("Configured");
    expect(
      screen.getByRole("checkbox", { name: "Clear TMDB API token" }),
    ).toBeVisible();
    await user.clear(screen.getByLabelText("Request timeout (seconds)"));
    await user.type(screen.getByLabelText("Request timeout (seconds)"), "45");
    await user.click(screen.getByRole("button", { name: "Save settings" }));

    expect(screen.getByLabelText("Request timeout (seconds)")).toBeDisabled();
    expect(tmdbToken).toBeDisabled();
    resolveUpdate(
      json({
        schema_version: 1,
        settings: { ...settings, timeout_seconds: 45 },
      }),
    );
    await waitFor(() => {
      const update = fetchMock.mock.calls.find(
        ([, init]) => init?.method === "PUT",
      );
      expect(update).toBeDefined();
      const body = JSON.parse(String(update?.[1]?.body));
      expect(body.timeout_seconds).toBe(45);
      expect(body.tmdb_api_token).toBeNull();
      expect(body.anilist_access_token).toBeNull();
      expect(body).not.toHaveProperty("secrets");
    });
    expect(await screen.findByText("Settings saved")).toBeVisible();
  });

  it("previews path and content templates through the no-write endpoint", async () => {
    let resolvePreview!: (response: Response) => void;
    const previewResponse = new Promise<Response>((resolve) => {
      resolvePreview = resolve;
    });
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === "/api/v1/templates/preview") return previewResponse;
      throw new Error(`Unexpected request: ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderApp("/templates");

    expect(
      await screen.findByRole("heading", { name: "Template studio" }),
    ).toBeVisible();
    await user.clear(screen.getByLabelText("Sample title"));
    await user.type(screen.getByLabelText("Sample title"), "Fixture Movie");
    await user.click(screen.getByRole("button", { name: "Preview template" }));

    expect(screen.getByLabelText("Sample title")).toBeDisabled();
    resolvePreview(
      json({
        schema_version: 1,
        path: "Fixture Movie (2024)/metadata.json",
        content: "title=Fixture Movie",
        content_bytes: 19,
      }),
    );
    expect(
      await screen.findByText("Fixture Movie (2024)/metadata.json"),
    ).toBeVisible();
    expect(screen.getByText("title=Fixture Movie")).toBeVisible();
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/templates/preview",
      expect.objectContaining({ method: "POST" }),
    );
  });
});
