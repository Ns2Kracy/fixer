import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiClient, type ApiError } from './api'

afterEach(() => vi.restoreAllMocks())

describe('ApiClient', () => {
  it('sends JSON, cookies, CSRF, and idempotency headers for an approved execution', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          schema_version: 1,
          job: {
            id: 7,
            input: { schema_version: 1, media_kind: 'movie', input_path: '/media/film.mkv', apply: true },
            state: 'writing',
            created_at_ms: 1,
            updated_at_ms: 2,
          },
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    )
    const client = new ApiClient({ fetch: fetchMock, csrfToken: () => 'fixer_csrf_test' })

    await client.executeJob(7, 'execution-7')

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/v1/jobs/7/execute',
      expect.objectContaining({
        method: 'POST',
        credentials: 'same-origin',
        headers: expect.objectContaining({
          'content-type': 'application/json',
          'idempotency-key': 'execution-7',
          'x-csrf-token': 'fixer_csrf_test',
        }),
        body: JSON.stringify({ approved: true }),
      }),
    )
  })

  it('reuses the login CSRF token for authenticated mutations', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({ schema_version: 1, csrf_token: 'fixer_csrf_login', expires_at_ms: 99 }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      )
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
    const client = new ApiClient({ fetch: fetchMock })

    await client.login({ password: 'correct horse battery staple' })
    await client.logout()

    expect(fetchMock).toHaveBeenLastCalledWith(
      '/api/v1/auth/logout',
      expect.objectContaining({
        method: 'POST',
        headers: expect.objectContaining({ 'x-csrf-token': 'fixer_csrf_login' }),
      }),
    )
  })

  it('preserves structured server errors for the interface', async () => {
    const client = new ApiClient({
      fetch: vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            error: {
              code: 'invalid_input',
              message: 'Request fields are invalid',
              details: { input_path: 'must not be empty' },
              request_id: 'req-2',
            },
          }),
          { status: 422, headers: { 'content-type': 'application/json' } },
        ),
      ),
    })

    await expect(client.createJob({ media_kind: 'book', input_path: '', apply: false })).rejects.toEqual(
      expect.objectContaining({
        name: 'ApiError',
        status: 422,
        code: 'invalid_input',
        requestId: 'req-2',
      } satisfies Partial<ApiError>),
    )
  })
})
