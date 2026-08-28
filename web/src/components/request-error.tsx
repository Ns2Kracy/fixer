import { For, Show } from 'solid-js'

import { ApiError } from '../lib/api'

export function RequestError(props: { error: Error | null; fallback?: string }) {
  const details = () => props.error instanceof ApiError ? Object.entries(props.error.details ?? {}) : []
  return (
    <div class="request-error" role="alert">
      <strong>{props.error?.message ?? props.fallback ?? 'The request could not be completed'}</strong>
      <For each={details()}>{([field, reason]) => <p><code>{field}</code> {reason}</p>}</For>
      <Show when={props.error instanceof ApiError && props.error.requestId}>
        <small>Request {(props.error as ApiError).requestId}</small>
      </Show>
    </div>
  )
}
