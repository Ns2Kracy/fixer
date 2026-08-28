import { Show, createSignal, onSettled } from 'solid-js'
import { useMutation, useQuery, useQueryClient } from '@tanstack/solid-query'
import { Link, createFileRoute } from '@tanstack/solid-router'

import { JobStatus } from '../../../components/job-status'
import { ProgressTimeline } from '../../../components/progress-timeline'
import { RequestError } from '../../../components/request-error'
import { api, type JobState } from '../../../lib/api'
import { connectJobEvents, type JobEventConnectionState } from '../../../lib/sse'

export const Route = createFileRoute('/jobs/$jobId/')({
  component: JobDetailPage,
})

const cancellableStates = new Set<JobState>([
  'queued',
  'scanning',
  'searching',
  'resolving',
  'awaiting_confirmation',
  'planning',
])

function JobDetailPage() {
  const params = Route.useParams()
  const queryClient = useQueryClient()
  const jobId = () => Number(params().jobId)
  const [connectionState, setConnectionState] = createSignal<JobEventConnectionState>('connecting')

  const job = useQuery(() => ({
    queryKey: ['job', jobId()],
    queryFn: () => api.getJob(jobId()),
  }))

  const cancel = useMutation(() => ({
    mutationFn: () => api.cancelJob(jobId()),
    onSuccess: (result) => queryClient.setQueryData(['job', jobId()], result),
  }))
  const retry = useMutation(() => ({
    mutationFn: () => api.retryJob(jobId()),
    onSuccess: (result) => queryClient.setQueryData(['job', jobId()], result),
  }))

  onSettled(() => {
    const connection = connectJobEvents(jobId(), {
      onEvent: () => undefined,
      reconcile: () => job.refetch().then(() => undefined),
      onConnectionChange: setConnectionState,
    })
    return () => connection.close()
  })

  return (
    <div class="job-detail-page">
      <Link class="back-link" to="/jobs">← All jobs</Link>
      <Show when={job.isPending}><p class="loading-line">Loading job…</p></Show>
      <Show when={job.isError}><RequestError error={job.error} /></Show>
      <Show when={job.data?.job}>
        {(current) => (
          <>
            <header class="page-heading detail-heading">
              <div>
                <p class="eyebrow">Job / #{current().id}</p>
                <h1>{current().input.input_path}</h1>
                <p>{current().input.media_kind} · {current().input.apply ? 'approved writes available' : 'dry run only'}</p>
              </div>
              <div class="detail-state">
                <JobStatus state={current().state} />
                <small role="status" aria-live="polite">Events: {connectionState()}</small>
              </div>
            </header>
            <section class="progress-panel" aria-labelledby="progress-title">
              <div class="section-heading"><div><p class="eyebrow">Pipeline</p><h2 id="progress-title">Job progress</h2></div></div>
              <ProgressTimeline state={current().state} progress={current().progress} />
            </section>
            <section class="job-actions" aria-label="Job actions">
              <Show when={current().state === 'awaiting_confirmation'}>
                <Link class="button primary" to="/jobs/$jobId/review" params={{ jobId: params().jobId }}>Review metadata</Link>
              </Show>
              <Show when={current().state === 'planning'}>
                <Link class="button primary" to="/jobs/$jobId/plan" params={{ jobId: params().jobId }}>Review output plan</Link>
              </Show>
              <Show when={current().state === 'interrupted'}>
                <div class="job-action-item">
                  <button class="button primary" type="button" disabled={retry.isPending} onClick={() => retry.mutate()}>
                    {retry.isPending ? 'Retrying…' : 'Retry job'}
                  </button>
                  <small>Restarts scanning from the beginning; no write resumes automatically.</small>
                </div>
              </Show>
              <Show when={cancellableStates.has(current().state)}>
                <div class="job-action-item">
                  <button class="button secondary" type="button" disabled={cancel.isPending} onClick={() => cancel.mutate()}>
                    {cancel.isPending ? 'Cancelling…' : 'Cancel before writing'}
                  </button>
                  <small>Stops processing before writing. Writing jobs cannot be cancelled.</small>
                </div>
              </Show>
            </section>
            <Show when={cancel.isError}><RequestError error={cancel.error} /></Show>
            <Show when={retry.isError}><RequestError error={retry.error} /></Show>
          </>
        )}
      </Show>
    </div>
  )
}
