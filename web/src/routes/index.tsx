import { Show } from 'solid-js'
import { useQuery } from '@tanstack/solid-query'
import { createFileRoute } from '@tanstack/solid-router'

import { ApiError, api } from '../lib/api'

export const Route = createFileRoute('/')({
  component: Workspace,
})

function Workspace() {
  const health = useQuery(() => ({
    queryKey: ['health'],
    queryFn: () => api.health(),
  }))

  return (
    <div class="dashboard">
      <section class="hero" aria-labelledby="workspace-title">
        <div>
          <p class="eyebrow">Workspace / Overview</p>
          <h1 id="workspace-title" aria-label="Metadata work, without guesswork.">Metadata work,<br /><em>without guesswork.</em></h1>
          <p class="lede">Inspect evidence, resolve conflicts, and approve every filesystem change before Fixer writes a byte.</p>
          <div class="hero-actions">
            <a class="button primary" href="#activity-title">Review workspace</a>
          </div>
        </div>
        <div class="system-card" aria-live="polite">
          <p class="eyebrow">System status</p>
          <Show when={health.isPending}>
            <p class="status-line"><span class="pulse" aria-hidden="true" /> Connecting to server…</p>
          </Show>
          <Show when={health.isSuccess}>
            <p class="status-line ready"><span aria-hidden="true" /> Server connected</p>
            <dl>
              <div><dt>API schema</dt><dd>v{health.data?.schema_version}</dd></div>
              <div><dt>Server</dt><dd>{health.data?.version}</dd></div>
              <div><dt>Write mode</dt><dd>Approval only</dd></div>
            </dl>
          </Show>
          <Show when={health.isError}>
            <ApiFailure error={health.error} />
          </Show>
        </div>
      </section>
      <section class="activity" aria-labelledby="activity-title">
        <div class="section-heading">
          <div><p class="eyebrow">Queue</p><h2 id="activity-title">Recent work</h2></div>
          <span class="count">0 active</span>
        </div>
        <div class="empty-inline">
          <span class="empty-glyph" aria-hidden="true">◇</span>
          <div><h3>No jobs yet</h3><p>New scans and review sessions will appear here.</p></div>
        </div>
      </section>
    </div>
  )
}

function ApiFailure(props: { error: Error | null }) {
  const requestId = () => props.error instanceof ApiError ? props.error.requestId : undefined
  return (
    <div class="api-error" role="alert">
      <strong>{props.error?.message ?? 'The server could not be reached'}</strong>
      <p>Check the server and try again.</p>
      <Show when={requestId()}>{(id) => <code>Request {id()}</code>}</Show>
    </div>
  )
}
