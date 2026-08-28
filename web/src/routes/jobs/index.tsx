import { For, Show, createSignal } from 'solid-js'
import { useMutation, useQuery, useQueryClient } from '@tanstack/solid-query'
import { Link, createFileRoute } from '@tanstack/solid-router'

import { JobStatus } from '../../components/job-status'
import { RequestError } from '../../components/request-error'
import {
  api,
  type CreateJobRequest,
  type JobState,
  type MediaKind,
} from '../../lib/api'

export const Route = createFileRoute('/jobs/')({
  component: JobsPage,
})

const mediaKinds: Array<{ value: MediaKind; label: string }> = [
  { value: 'anime', label: 'Anime' },
  { value: 'book', label: 'Book' },
  { value: 'movie', label: 'Movie' },
  { value: 'music', label: 'Music' },
  { value: 'television', label: 'Television' },
]

const states: Array<{ value: JobState | ''; label: string }> = [
  { value: '', label: 'All states' },
  { value: 'queued', label: 'Queued' },
  { value: 'scanning', label: 'Scanning' },
  { value: 'searching', label: 'Searching' },
  { value: 'resolving', label: 'Resolving' },
  { value: 'awaiting_confirmation', label: 'Awaiting review' },
  { value: 'planning', label: 'Plan ready' },
  { value: 'writing', label: 'Writing' },
  { value: 'completed', label: 'Completed' },
  { value: 'failed', label: 'Failed' },
  { value: 'cancelled', label: 'Cancelled' },
  { value: 'interrupted', label: 'Interrupted' },
]

function JobsPage() {
  const queryClient = useQueryClient()
  const [stateFilter, setStateFilter] = createSignal<JobState | ''>('')
  const [mediaKind, setMediaKind] = createSignal<MediaKind>('movie')
  const [mediaPath, setMediaPath] = createSignal('')
  const [apply, setApply] = createSignal(false)

  const jobs = useQuery(() => ({
    queryKey: ['jobs', stateFilter()],
    queryFn: () => api.listJobs({
      limit: 50,
      ...(stateFilter() ? { state: stateFilter() as JobState } : {}),
    }),
  }))

  const createJob = useMutation(() => ({
    mutationFn: (request: CreateJobRequest) => api.createJob(request),
    onSuccess: () => {
      setMediaPath('')
      setApply(false)
      void queryClient.invalidateQueries({ queryKey: ['jobs'] })
    },
  }))

  function submit(event: SubmitEvent) {
    event.preventDefault()
    const inputPath = mediaPath().trim()
    if (!inputPath) return
    createJob.mutate({ media_kind: mediaKind(), input_path: inputPath, apply: apply() })
  }

  return (
    <div class="jobs-page">
      <header class="page-heading jobs-heading">
        <div>
          <p class="eyebrow">Jobs / Local queue</p>
          <h1>Scrape jobs</h1>
          <p>Scan media, compare metadata evidence, and stage output without silent writes.</p>
        </div>
        <label class="filter-control">
          <span>State filter</span>
          <select
            value={stateFilter()}
            onChange={(event) => setStateFilter(event.currentTarget.value as JobState | '')}
          >
            <For each={states}>{(state) => <option value={state.value}>{state.label}</option>}</For>
          </select>
        </label>
      </header>

      <section class="job-create-panel" aria-labelledby="create-job-title">
        <div>
          <p class="eyebrow">New scan</p>
          <h2 id="create-job-title">Create a bounded job</h2>
        </div>
        <form onSubmit={submit}>
          <label>
            <span>Media kind</span>
            <select
              value={mediaKind()}
              onChange={(event) => setMediaKind(event.currentTarget.value as MediaKind)}
            >
              <For each={mediaKinds}>{(kind) => <option value={kind.value}>{kind.label}</option>}</For>
            </select>
          </label>
          <label class="path-field">
            <span>Media path</span>
            <input
              type="text"
              value={mediaPath()}
              placeholder="/media/Title.mkv"
              required
              onInput={(event) => setMediaPath(event.currentTarget.value)}
            />
          </label>
          <label class="write-toggle">
            <input
              type="checkbox"
              aria-label="Allow approved writes"
              checked={apply()}
              onChange={(event) => setApply(event.currentTarget.checked)}
            />
            <span><strong>Allow approved writes</strong><small>A plan still requires review.</small></span>
          </label>
          <button class="button primary" type="submit" disabled={createJob.isPending || !mediaPath().trim()}>
            {createJob.isPending ? 'Creating…' : 'Create job'}
          </button>
        </form>
        <Show when={createJob.isError}><RequestError error={createJob.error} /></Show>
      </section>

      <section class="job-list" aria-labelledby="job-list-title">
        <div class="section-heading">
          <div><p class="eyebrow">Queue</p><h2 id="job-list-title">Recent jobs</h2></div>
          <span class="count">{jobs.data?.jobs.length ?? 0} shown</span>
        </div>
        <Show when={jobs.isPending}><p class="loading-line">Loading jobs…</p></Show>
        <Show when={jobs.isError}><RequestError error={jobs.error} /></Show>
        <Show when={jobs.isSuccess && jobs.data?.jobs.length === 0}>
          <div class="empty-inline"><div><h3>No matching jobs</h3><p>Change the filter or create a new scan.</p></div></div>
        </Show>
        <div class="job-cards">
          <For each={jobs.data?.jobs ?? []}>
            {(job) => (
              <article class="job-card">
                <div class="job-card-number">#{job.id}</div>
                <div class="job-card-main">
                  <p class="job-path">{job.input.input_path}</p>
                  <p>{job.input.media_kind} · updated {new Date(job.updated_at_ms).toLocaleString()}</p>
                </div>
                <JobStatus state={job.state} />
                <Link class="text-link" to="/jobs/$jobId" params={{ jobId: String(job.id) }}>
                  Inspect <span aria-hidden="true">→</span>
                </Link>
              </article>
            )}
          </For>
        </div>
      </section>
    </div>
  )
}
