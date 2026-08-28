import type { JobState } from '../lib/api'

const labels: Record<JobState, string> = {
  queued: 'Queued',
  scanning: 'Scanning',
  searching: 'Searching',
  resolving: 'Resolving',
  awaiting_confirmation: 'Awaiting review',
  planning: 'Plan ready',
  writing: 'Writing',
  completed: 'Completed',
  failed: 'Failed',
  cancelled: 'Cancelled',
  interrupted: 'Interrupted',
}

export function jobStateLabel(state: JobState) {
  return labels[state]
}

export function JobStatus(props: { state: JobState }) {
  return (
    <span class={`job-status job-status-${props.state}`}>
      <span aria-hidden="true" />
      {jobStateLabel(props.state)}
    </span>
  )
}
