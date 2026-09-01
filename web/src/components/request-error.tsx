import { For, Show } from "solid-js";

import { ApiError } from "../lib/api";

export function RequestError(props: {
  error: Error | null;
  fallback?: string;
}) {
  const details = () =>
    props.error instanceof ApiError
      ? Object.entries(props.error.details ?? {})
      : [];

  return (
    <div
      class="my-4 border-l-[3px] border-coral bg-danger-surface px-4 py-3 text-ink"
      role="alert"
    >
      <strong>
        {props.error?.message ??
          props.fallback ??
          "The request could not be completed"}
      </strong>
      <For each={details()}>
        {([field, reason]) => (
          <p class="my-1">
            <code class="text-xs">{field}</code> {reason}
          </p>
        )}
      </For>
      <Show when={props.error instanceof ApiError && props.error.requestId}>
        <small class="text-xs">
          Request {(props.error as ApiError).requestId}
        </small>
      </Show>
    </div>
  );
}
