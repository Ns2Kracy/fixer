import { Show, createSignal } from 'solid-js'
import { useMutation, useQuery, useQueryClient } from '@tanstack/solid-query'
import { Link, createFileRoute } from '@tanstack/solid-router'

import { OutputDiff } from '../../../components/output-diff'
import { RequestError } from '../../../components/request-error'
import { api } from '../../../lib/api'

export const Route = createFileRoute('/jobs/$jobId/plan')({
  component: PlanPage,
})

function PlanPage() {
  const params = Route.useParams()
  const queryClient = useQueryClient()
  const jobId = () => Number(params().jobId)
  const [approved, setApproved] = createSignal(false)
  const executionNonce = crypto.randomUUID()

  const job = useQuery(() => ({
    queryKey: ['job', jobId()],
    queryFn: () => api.getJob(jobId()),
  }))

  const plan = useQuery(() => ({
    queryKey: ['job-plan', jobId()],
    queryFn: () => api.getJobPlan(jobId()),
  }))

  const execute = useMutation(() => ({
    mutationFn: () => api.executeJob(jobId(), `job-${jobId()}-${executionNonce}`),
    onSuccess: (result) => queryClient.setQueryData(['job', jobId()], result),
  }))

  return (
    <div class="plan-page">
      <Link class="back-link" to="/jobs/$jobId/review" params={{ jobId: params().jobId }}>← Metadata review</Link>
      <header class="page-heading">
        <div><p class="eyebrow">Job / #{params().jobId} / Filesystem</p><h1>Output plan</h1><p>Review targets and operation types. File contents stay server-owned and are never returned here.</p></div>
      </header>
      <Show when={plan.isPending || job.isPending}><p class="loading-line">Loading output operations…</p></Show>
      <Show when={plan.isError}><RequestError error={plan.error} /></Show>
      <Show when={job.isError}><RequestError error={job.error} /></Show>
      <Show when={plan.data}>
        {(data) => (
          <>
            <OutputDiff operations={data().operations} outputRoot={data().output_root} />
            <Show when={data().operations_truncated}>
              <p class="truncation-note" role="alert">The operation list is incomplete. Execution is disabled.</p>
            </Show>
            <Show when={job.data && !job.data.job.input.apply}>
              <p class="preview-only">This is a dry-run job. The plan cannot be executed.</p>
            </Show>
            <Show when={job.data && job.data.job.input.apply && job.data.job.state !== 'planning'}>
              <p class="preview-only">Execution is unavailable while the job is {job.data?.job.state}.</p>
            </Show>
            <div class="execution-gate">
              <div>
                <p class="eyebrow">Final gate</p>
                <h2>Filesystem approval</h2>
                <p>This authorizes only the bounded server-owned plan shown here.</p>
              </div>
              <label>
                <input
                  type="checkbox"
                  checked={approved()}
                  disabled={!data().requires_approval || data().operations_truncated || !job.data?.job.input.apply || job.data?.job.state !== 'planning' || execute.isSuccess}
                  onChange={(event) => setApproved(event.currentTarget.checked)}
                />
                I approve these filesystem operations
              </label>
              <button
                class="button primary danger"
                type="button"
                disabled={!approved() || data().operations_truncated || !job.data?.job.input.apply || job.data?.job.state !== 'planning' || execute.isPending || execute.isSuccess}
                onClick={() => execute.mutate()}
              >
                {execute.isPending ? 'Executing…' : execute.isSuccess ? 'Execution requested' : 'Execute approved plan'}
              </button>
            </div>
            <Show when={execute.isSuccess}><p class="success-note" role="status">Execution was accepted. Live progress is available on the job page.</p></Show>
            <Show when={execute.isError}><RequestError error={execute.error} /></Show>
          </>
        )}
      </Show>
    </div>
  )
}
