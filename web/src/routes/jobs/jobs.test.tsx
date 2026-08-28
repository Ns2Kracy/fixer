import userEvent from '@testing-library/user-event'
import { QueryClient } from '@tanstack/solid-query'
import { createMemoryHistory } from '@tanstack/solid-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { App } from '../../app'
import { createAppRouter } from '../../router'
import { render, screen, waitFor } from '../../test/render'

class SilentEventSource {
  static instances: SilentEventSource[] = []

  onopen: ((event: Event) => void) | null = null
  onerror: ((event: Event) => void) | null = null
  closed = false

  constructor(_url: string | URL, _options?: EventSourceInit) {
    SilentEventSource.instances.push(this)
  }
  addEventListener() {}
  removeEventListener() {}
  close() { this.closed = true }
}

const job = {
  id: 7,
  input: {
    schema_version: 1,
    media_kind: 'movie',
    input_path: '/media/Fixture Movie.mkv',
    apply: true,
  },
  state: 'awaiting_confirmation',
  progress: { schema_version: 1, stage: 'review', completed_items: 1, total_items: 1 },
  review: { schema_version: 1, candidate_count: 2, conflict_count: 1 },
  created_at_ms: 1,
  updated_at_ms: 2,
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })
}

function renderApp(initialEntry: string) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const router = createAppRouter({
    history: createMemoryHistory({ initialEntries: [initialEntry] }),
    queryClient,
  })
  return render(() => <App queryClient={queryClient} router={router} />)
}

beforeEach(() => {
  SilentEventSource.instances = []
  vi.stubGlobal('EventSource', SilentEventSource)
  vi.stubGlobal('crypto', { randomUUID: () => 'execution-uuid' })
})

describe('jobs workflow', () => {
  it('renders fetched progress and closes the live event connection on unmount', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => json({ schema_version: 1, job })))
    const view = renderApp('/jobs/7')

    expect(await screen.findByRole('heading', { name: '/media/Fixture Movie.mkv' })).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Job progress' })).toBeVisible()
    expect(screen.getByRole('status')).toHaveTextContent('Events: connecting')
    expect(SilentEventSource.instances).toHaveLength(1)

    view.unmount()
    expect(SilentEventSource.instances[0]?.closed).toBe(true)
  })

  it.each([
    ['queued', false, true],
    ['scanning', false, true],
    ['searching', false, true],
    ['resolving', false, true],
    ['awaiting_confirmation', false, true],
    ['planning', false, true],
    ['interrupted', true, false],
    ['failed', false, false],
    ['writing', false, false],
    ['completed', false, false],
    ['cancelled', false, false],
  ] as const)('shows valid actions for %s jobs', async (state, canRetry, canCancel) => {
    vi.stubGlobal('fetch', vi.fn(async () => json({ schema_version: 1, job: { ...job, state } })))
    const view = renderApp('/jobs/7')

    expect(await screen.findByRole('heading', { name: '/media/Fixture Movie.mkv' })).toBeVisible()
    expect(Boolean(screen.queryByRole('button', { name: 'Retry job' }))).toBe(canRetry)
    expect(Boolean(screen.queryByRole('button', { name: 'Cancel before writing' }))).toBe(canCancel)
    view.unmount()
  })

  it('lists, filters, and creates scrape jobs', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (init?.method === 'POST') {
        return json({ schema_version: 1, job: { ...job, id: 8, state: 'queued' } }, 202)
      }
      const jobs = url.includes('state=interrupted')
        ? [{ ...job, id: 8, state: 'interrupted', input: { ...job.input, input_path: '/media/B.mkv' } }]
        : [
            job,
            { ...job, id: 8, state: 'interrupted', input: { ...job.input, input_path: '/media/B.mkv' } },
          ]
      return json({ schema_version: 1, jobs, has_more: false })
    })
    vi.stubGlobal('fetch', fetchMock)
    const user = userEvent.setup()
    renderApp('/jobs')

    expect(await screen.findByRole('heading', { name: 'Scrape jobs' })).toBeVisible()
    expect(await screen.findByText('/media/B.mkv')).toBeVisible()

    await user.selectOptions(screen.getByLabelText('State filter'), 'interrupted')
    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        '/api/v1/jobs?limit=50&state=interrupted',
        expect.any(Object),
      ),
    )

    await user.selectOptions(screen.getByLabelText('Media kind'), 'movie')
    await user.type(screen.getByLabelText('Media path'), '/media/New Movie.mkv')
    await user.click(screen.getByLabelText('Allow approved writes'))
    await user.click(screen.getByRole('button', { name: 'Create job' }))

    await waitFor(() => {
      const create = fetchMock.mock.calls.find(([, init]) => init?.method === 'POST')
      expect(create?.[0]).toBe('/api/v1/jobs')
      expect(JSON.parse(String(create?.[1]?.body))).toEqual({
        media_kind: 'movie',
        input_path: '/media/New Movie.mkv',
        apply: true,
      })
    })
  })

  it('compares candidates, preserves partial warnings, and acknowledges sourced conflicts', async () => {
    const review = (selected = 1) => ({
      schema_version: 1,
      job_id: 7,
      selected_candidate_index: selected,
      candidates: [
        {
          index: 0,
          media_kind: 'movie',
          provider: 'fixture.local',
          external_id: { namespace: 'local', value: 'one' },
          title: 'Candidate One',
          year: 2000,
          score: 100,
          evidence: [{ kind: 'title', points: 100, detail: 'exact title' }],
          evidence_truncated: false,
        },
        {
          index: 1,
          media_kind: 'movie',
          provider: 'fixture.remote',
          external_id: { namespace: 'tmdb', value: '843' },
          title: 'Candidate Two',
          year: 2000,
          score: 90,
          evidence: [{ kind: 'year', points: 20, detail: 'year matched' }],
          evidence_truncated: true,
        },
      ],
      candidates_truncated: true,
      warnings: [{ code: 'provider_search_failed', message: 'TMDB timed out; local metadata remains usable.' }],
      warnings_truncated: false,
      conflicts: [{
        index: 0,
        field_path: 'summaries',
        message: 'summary differs',
        providers: ['fixture.local', 'fixture.remote'],
        providers_truncated: false,
        sources: [{ provider: 'fixture.local', locale: 'zh-CN' }],
        sources_truncated: true,
      }],
      conflicts_truncated: false,
    })
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/jobs/7/review') && init?.method === 'POST') {
        return json({ schema_version: 1, job: { ...job, state: 'planning' } })
      }
      if (url.includes('/jobs/7/review')) {
        return json(review(url.includes('candidate_index=0') ? 0 : 1))
      }
      if (url.endsWith('/jobs/7/plan')) {
        return json({ schema_version: 1, job_id: 7, output_root: '/media', operations: [], operations_truncated: false, requires_approval: true })
      }
      return json({ schema_version: 1, job })
    })
    vi.stubGlobal('fetch', fetchMock)
    const user = userEvent.setup()
    renderApp('/jobs/7/review')

    expect(await screen.findByRole('heading', { name: 'Review metadata' })).toBeVisible()
    expect(await screen.findByText('TMDB timed out; local metadata remains usable.')).toBeVisible()
    expect(screen.getByText('exact title')).toBeVisible()
    expect(screen.getByRole('radio', { name: 'Select Candidate Two from fixture.remote' })).toBeChecked()
    expect(fetchMock).toHaveBeenCalledWith('/api/v1/jobs/7/review', expect.any(Object))
    expect(screen.getByText(/^Additional candidates were omitted by the server./)).toBeVisible()
    expect(screen.getByText('Additional matching evidence was omitted.')).toBeVisible()

    await user.click(screen.getByRole('radio', { name: 'Select Candidate One from fixture.local' }))
    await user.click(screen.getByRole('radio', { name: 'Select Candidate Two from fixture.remote' }))
    expect(await screen.findByText('summary differs')).toBeVisible()
    expect(screen.getByText('zh-CN · fixture.local')).toBeVisible()
    expect(screen.getByText('Additional source context was omitted by the server.')).toBeVisible()
    await user.click(screen.getByRole('checkbox', { name: 'Acknowledge conflict summaries' }))
    await user.click(screen.getByRole('button', { name: 'Accept candidate and build plan' }))

    await waitFor(() => {
      const request = fetchMock.mock.calls.find(
        ([input, init]) => String(input).endsWith('/jobs/7/review') && init?.method === 'POST',
      )
      expect(JSON.parse(String(request?.[1]?.body))).toEqual({
        candidate_index: 1,
        accepted_conflict_indexes: [0],
      })
    })
  })

  it('previews filesystem operations and requires explicit approval before execution', async () => {
    const planningJob = {
      ...job,
      state: 'planning',
      review_decision: { schema_version: 1, candidate_index: 0, accepted_conflict_indexes: [] },
      plan: { schema_version: 1, operation_count: 2, requires_confirmation: true },
    }
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/jobs/7/execute') && init?.method === 'POST') {
        return json({ schema_version: 1, job: { ...planningJob, state: 'completed' } })
      }
      if (url.endsWith('/jobs/7/plan')) {
        return json({
          schema_version: 1,
          job_id: 7,
          output_root: '/media',
          operations: [
            { index: 0, kind: 'write', source: null, target: 'movie.json', content_bytes: 128 },
            { index: 1, kind: 'reflink', source: 'source.mkv', target: 'Movie.mkv' },
          ],
          operations_truncated: false,
          requires_approval: true,
        })
      }
      return json({ schema_version: 1, job: planningJob })
    })
    vi.stubGlobal('fetch', fetchMock)
    const user = userEvent.setup()
    renderApp('/jobs/7/plan')

    expect(await screen.findByRole('heading', { name: 'Output plan' })).toBeVisible()
    expect(await screen.findByText('movie.json')).toBeVisible()
    expect(screen.getByText('source.mkv → Movie.mkv')).toBeVisible()
    const execute = screen.getByRole('button', { name: 'Execute approved plan' })
    expect(execute).toBeDisabled()

    await user.click(screen.getByLabelText('I approve these filesystem operations'))
    expect(execute).toBeEnabled()
    await user.click(execute)

    await waitFor(() => {
      const request = fetchMock.mock.calls.find(
        ([input, init]) => String(input).endsWith('/jobs/7/execute') && init?.method === 'POST',
      )
      expect(request?.[1]?.headers).toEqual(expect.objectContaining({ 'idempotency-key': 'job-7-execution-uuid' }))
      expect(JSON.parse(String(request?.[1]?.body))).toEqual({ approved: true })
    })
  })

  it('reuses one execution key when an ambiguous request is retried', async () => {
    const randomUUID = vi.fn()
      .mockReturnValueOnce('first-attempt')
      .mockReturnValueOnce('second-attempt')
    vi.stubGlobal('crypto', { randomUUID })
    let attempts = 0
    const planningJob = {
      ...job,
      state: 'planning',
      review_decision: { schema_version: 1, candidate_index: 0, accepted_conflict_indexes: [] },
      plan: { schema_version: 1, operation_count: 0, requires_confirmation: true },
    }
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/jobs/7/execute') && init?.method === 'POST') {
        attempts += 1
        if (attempts === 1) {
          return json({ error: { code: 'response_lost', message: 'Response lost', request_id: 'req-1' } }, 503)
        }
        return json({ schema_version: 1, job: { ...planningJob, state: 'writing' } })
      }
      if (url.endsWith('/jobs/7/plan')) {
        return json({ schema_version: 1, job_id: 7, output_root: '/media', operations: [], operations_truncated: false, requires_approval: true })
      }
      return json({ schema_version: 1, job: planningJob })
    })
    vi.stubGlobal('fetch', fetchMock)
    const user = userEvent.setup()
    renderApp('/jobs/7/plan')

    expect(await screen.findByRole('heading', { name: 'Output plan' })).toBeVisible()
    expect(await screen.findByText('Output root')).toBeVisible()
    await user.click(screen.getByLabelText('I approve these filesystem operations'))
    await user.click(screen.getByRole('button', { name: 'Execute approved plan' }))
    expect(await screen.findByText('Response lost')).toBeVisible()
    await user.click(screen.getByRole('button', { name: 'Execute approved plan' }))

    await waitFor(() => {
      const requests = fetchMock.mock.calls.filter(
        ([input, init]) => String(input).endsWith('/jobs/7/execute') && init?.method === 'POST',
      )
      expect(requests).toHaveLength(2)
      expect(requests.map(([, init]) => init?.headers)).toEqual([
        expect.objectContaining({ 'idempotency-key': 'job-7-first-attempt' }),
        expect.objectContaining({ 'idempotency-key': 'job-7-first-attempt' }),
      ])
      expect(randomUUID).toHaveBeenCalledTimes(1)
    })
  })

  it('blocks acceptance when the server omits conflicts', async () => {
    const truncatedReview = {
      schema_version: 1,
      job_id: 7,
      selected_candidate_index: 0,
      candidates: [{
        index: 0,
        media_kind: 'movie',
        provider: 'fixture.local',
        external_id: { namespace: 'local', value: 'one' },
        title: 'Candidate One',
        score: 100,
        evidence: [],
        evidence_truncated: false,
      }],
      candidates_truncated: false,
      warnings: [],
      warnings_truncated: false,
      conflicts: [{
        index: 0,
        field_path: 'title',
        message: 'title differs',
        providers: ['fixture.local'],
        providers_truncated: false,
        sources: [],
        sources_truncated: false,
      }],
      conflicts_truncated: true,
    }
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) =>
      String(input).includes('/jobs/7/review')
        ? json(truncatedReview)
        : json({ schema_version: 1, job }),
    ))
    const user = userEvent.setup()
    renderApp('/jobs/7/review')

    expect(await screen.findByText(/server omitted additional conflicts/)).toBeVisible()
    await user.click(screen.getByRole('checkbox', { name: 'Acknowledge conflict title' }))
    expect(screen.getByRole('button', { name: 'Accept candidate and build plan' })).toBeDisabled()
  })

  it('blocks execution when the server omits operations', async () => {
    const planningJob = { ...job, state: 'planning' }
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) =>
      String(input).endsWith('/jobs/7/plan')
        ? json({
            schema_version: 1,
            job_id: 7,
            output_root: '/media',
            operations: [{ index: 0, kind: 'write', source: null, target: 'movie.json', content_bytes: 128 }],
            operations_truncated: true,
            requires_approval: true,
          })
        : json({ schema_version: 1, job: planningJob }),
    ))
    renderApp('/jobs/7/plan')

    expect(await screen.findByText('The operation list is incomplete. Execution is disabled.')).toBeVisible()
    expect(screen.getByLabelText('I approve these filesystem operations')).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Execute approved plan' })).toBeDisabled()
  })

  it('keeps dry-run plans preview-only', async () => {
    const dryRunJob = { ...job, state: 'planning', input: { ...job.input, apply: false } }
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) =>
      String(input).endsWith('/jobs/7/plan')
        ? json({ schema_version: 1, job_id: 7, output_root: '/media', operations: [], operations_truncated: false, requires_approval: true })
        : json({ schema_version: 1, job: dryRunJob }),
    ))
    renderApp('/jobs/7/plan')

    expect(await screen.findByText('This is a dry-run job. The plan cannot be executed.')).toBeVisible()
    expect(screen.getByLabelText('I approve these filesystem operations')).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Execute approved plan' })).toBeDisabled()
  })
})
