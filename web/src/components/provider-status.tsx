import { Show } from "solid-js";
import type { JSX } from "@solidjs/web";

import type { ProviderDto, ProviderProbeEnvelope } from "../lib/api";

interface ProviderStatusProps {
  provider: ProviderDto;
  enabled: boolean;
  testing: boolean;
  disabled: boolean;
  result: ProviderProbeEnvelope | undefined;
  onTest: () => void;
}

export function ProviderStatus(props: ProviderStatusProps): JSX.Element {
  return (
    <article class="provider-row">
      <div class="provider-index" aria-hidden="true">
        {props.provider.id.slice(0, 2).toUpperCase()}
      </div>
      <div class="provider-identity">
        <div>
          <h2>{props.provider.name}</h2>
          <span
            class={`provider-mode ${props.provider.network ? "network" : "local"}`}
          >
            {props.provider.network ? "Network" : "On device"}
          </span>
        </div>
        <p>{props.provider.media_kinds.join(" / ")}</p>
      </div>
      <div class="provider-enabled">
        <span
          class={props.enabled ? "status-mark ready" : "status-mark"}
          aria-hidden="true"
        />
        {props.enabled ? "Enabled" : "Disabled"}
      </div>
      <button
        class="button secondary"
        type="button"
        disabled={props.disabled || props.testing}
        onClick={props.onTest}
      >
        {props.testing ? "Testing…" : `Test ${props.provider.name}`}
      </button>
      <Show when={props.result}>
        {(result) => (
          <p
            class={`provider-result ${result().ok ? "ready" : "attention"}`}
            role="status"
          >
            <strong>{result().category.replaceAll("_", " ")}</strong>
            <span>{result().message}</span>
          </p>
        )}
      </Show>
    </article>
  );
}
