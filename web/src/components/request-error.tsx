import { For, Show } from "solid-js";

import { ApiError } from "../lib/api";
import { Notice } from "./ui/notice";

export function RequestError(props: {
  error: Error | null;
  fallback?: string;
}) {
  const apiError = () =>
    props.error instanceof ApiError ? props.error : undefined;
  const details = () => Object.entries(apiError()?.details ?? {});

  return (
    <Notice class="my-4" tone="danger" role="alert">
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
      <Show when={apiError()?.requestId}>
        {(requestId) => <small class="text-xs">Request {requestId()}</small>}
      </Show>
    </Notice>
  );
}
