import { afterEach, describe, expect, it, vi } from "vitest";

import {
  ApiClient,
  type ApiError,
  type UpdateWorkspaceSettingsRequest,
} from "./api";

afterEach(() => vi.restoreAllMocks());

describe("ApiClient", () => {
  it("sends JSON, cookies, CSRF, and idempotency headers for an approved execution", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          schema_version: 1,
          job: {
            id: 7,
            input: {
              schema_version: 1,
              media_kind: "movie",
              input_path: "/media/film.mkv",
              apply: true,
            },
            state: "writing",
            created_at_ms: 1,
            updated_at_ms: 2,
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    const client = new ApiClient({
      fetch: fetchMock,
      csrfToken: () => "fixer_csrf_test",
    });

    await client.executeJob(7, "execution-7");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/jobs/7/execute",
      expect.objectContaining({
        method: "POST",
        credentials: "same-origin",
        headers: expect.objectContaining({
          "content-type": "application/json",
          "idempotency-key": "execution-7",
          "x-csrf-token": "fixer_csrf_test",
        }),
        body: JSON.stringify({ approved: true }),
      }),
    );
  });

  it("loads auth status and registers the first administrator", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            schema_version: 1,
            registration_required: true,
            authenticated: false,
            username: null,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            schema_version: 1,
            username: "admin",
            csrf_token: "fixer_csrf_register",
            expires_at_ms: 99,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      );
    const client = new ApiClient({ fetch: fetchMock });

    await expect(client.authStatus()).resolves.toEqual(
      expect.objectContaining({
        registration_required: true,
        authenticated: false,
      }),
    );
    await client.register({
      username: "admin",
      password: "correct horse battery staple",
    });

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "/api/v1/auth/status",
      expect.objectContaining({ method: "GET", credentials: "same-origin" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "/api/v1/auth/register",
      expect.objectContaining({
        method: "POST",
        credentials: "same-origin",
        body: JSON.stringify({
          username: "admin",
          password: "correct horse battery staple",
        }),
      }),
    );
  });

  it("reuses the login CSRF token for authenticated mutations", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            schema_version: 1,
            username: "admin",
            csrf_token: "fixer_csrf_login",
            expires_at_ms: 99,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    const client = new ApiClient({ fetch: fetchMock });

    await client.login({
      username: "admin",
      password: "correct horse battery staple",
    });
    await client.logout();

    expect(fetchMock).toHaveBeenLastCalledWith(
      "/api/v1/auth/logout",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({
          "x-csrf-token": "fixer_csrf_login",
        }),
      }),
    );
  });

  it("preserves structured server errors for the interface", async () => {
    const client = new ApiClient({
      fetch: vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            error: {
              code: "invalid_input",
              message: "Request fields are invalid",
              details: { input_path: "must not be empty" },
              request_id: "req-2",
            },
          }),
          { status: 422, headers: { "content-type": "application/json" } },
        ),
      ),
    });

    await expect(
      client.createJob({ media_kind: "book", input_path: "", apply: false }),
    ).rejects.toEqual(
      expect.objectContaining({
        name: "ApiError",
        status: 422,
        code: "invalid_input",
        requestId: "req-2",
      } satisfies Partial<ApiError>),
    );
  });

  it("requests bounded job lists and reconstructed artifacts with stable query parameters", async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(
        async () =>
          new Response(
            JSON.stringify({ schema_version: 1, jobs: [], has_more: false }),
            { status: 200, headers: { "content-type": "application/json" } },
          ),
      );
    const client = new ApiClient({ fetch: fetchMock });

    await client.listJobs({ limit: 25, state: "interrupted" });
    await client.getJobReview(7, 3);
    await client.getJobPlan(7);

    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/api/v1/jobs?limit=25&state=interrupted",
      "/api/v1/jobs/7/review?candidate_index=3",
      "/api/v1/jobs/7/plan",
    ]);
  });

  it("retries only through the dedicated mutation endpoint", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ schema_version: 1, job: { id: 9 } }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    const client = new ApiClient({ fetch: fetchMock, csrfToken: () => "csrf" });

    await client.retryJob(9);

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/jobs/9/retry",
      expect.objectContaining({
        method: "POST",
        credentials: "same-origin",
        headers: expect.objectContaining({ "x-csrf-token": "csrf" }),
      }),
    );
  });

  it("requests opaque library/search resources with encoded query parameters", async () => {
    const fetchMock = vi.fn().mockImplementation(
      async () =>
        new Response(
          JSON.stringify({ schema_version: 1, roots: [], results: [] }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        ),
    );
    const client = new ApiClient({ fetch: fetchMock });

    await client.libraryRoots();
    await client.listLibrary({ rootId: "root-0", path: "Books & Audio" });
    await client.search({
      mediaKind: "book",
      query: "fixture title",
      limit: 20,
    });

    expect(fetchMock.mock.calls.map(([url]) => url)).toEqual([
      "/api/v1/library/roots",
      "/api/v1/library?root_id=root-0&path=Books+%26+Audio",
      "/api/v1/search?media_kind=book&query=fixture+title&limit=20",
    ]);
  });

  it("updates settings and previews providers/templates through CSRF-protected mutations", async () => {
    const fetchMock = vi.fn().mockImplementation(
      async () =>
        new Response(JSON.stringify({ schema_version: 1 }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
    );
    const client = new ApiClient({
      fetch: fetchMock,
      csrfToken: () => "workspace-csrf",
    });
    const settings: UpdateWorkspaceSettingsRequest = {
      offline: false,
      proxy: null,
      preferred_locales: ["ja", "en"],
      timeout_seconds: 20,
      auto_accept_confidence: 0.9,
      review_confidence: 0.6,
      output_preset: "full" as const,
      placement: "in_place" as const,
      conflict_policy: "review" as const,
      enabled_providers: ["local", "tmdb"],
      provider_endpoints: {
        tmdb: "https://api.themoviedb.org/3",
        bangumi: "https://api.bgm.tv",
        anilist: "https://graphql.anilist.co",
        musicbrainz: "https://musicbrainz.org/ws/2",
        openlibrary: "https://openlibrary.org",
        openlibrary_cover: "https://covers.openlibrary.org/b/",
      },
      tmdb_api_token: null,
      anilist_access_token: null,
      clear_tmdb_api_token: false,
      clear_anilist_access_token: false,
    };

    await client.updateSettings(settings);
    await client.testProvider("tmdb");
    await client.previewTemplate({
      path_template: "{{title|sanitize}}/metadata.json",
      content_template: "title={{title}}",
      sample: { title: "Fixture", id: "fixture", year: 2024, edition: null },
    });

    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      "/api/v1/settings",
      expect.objectContaining({
        method: "PUT",
        headers: expect.objectContaining({
          "content-type": "application/json",
          "x-csrf-token": "workspace-csrf",
        }),
        body: JSON.stringify(settings),
      }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      "/api/v1/providers/tmdb/test",
      expect.objectContaining({ method: "POST", body: "{}" }),
    );
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      "/api/v1/templates/preview",
      expect.objectContaining({ method: "POST" }),
    );
  });
});
