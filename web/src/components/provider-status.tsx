import type { JSX } from "@solidjs/web";
import { Show } from "solid-js";

import type { ProviderDto, ProviderProbeEnvelope } from "../lib/api";
import { Button } from "./ui/button";

interface ProviderStatusProps {
  provider: ProviderDto;
  enabled: boolean;
  testing: boolean;
  disabled: boolean;
  result: ProviderProbeEnvelope | undefined;
  onTest: () => void;
}

const modeClasses = {
  network: "border-coral text-danger",
  local: "border-line text-muted",
} as const;

const resultClasses = {
  ready: "border-success bg-success-surface",
  attention: "border-coral bg-danger-surface",
} as const;

export function ProviderStatus(props: ProviderStatusProps): JSX.Element {
  return (
    <article class="grid min-h-[116px] grid-cols-[52px_minmax(220px,1fr)_110px_auto] items-center gap-5 border-b border-line max-[1000px]:grid-cols-[44px_minmax(0,1fr)_auto] max-[700px]:grid-cols-[38px_minmax(0,1fr)] max-[700px]:py-4">
      <div class="font-serif text-base font-medium text-muted" aria-hidden="true">
        {props.provider.id.slice(0, 2).toUpperCase()}
      </div>
      <div class="min-w-0">
        <div class="flex items-center gap-3">
          <h2 class="m-0 font-serif text-xl font-medium">
            {props.provider.name}
          </h2>
          <span
            class={`border px-1.5 py-0.5 text-[0.57rem] font-extrabold uppercase tracking-[0.08em] ${props.provider.network ? modeClasses.network : modeClasses.local}`}
          >
            {props.provider.network ? "Network" : "On device"}
          </span>
        </div>
        <p class="mt-1 mb-0 text-[0.68rem] capitalize text-muted">
          {props.provider.media_kinds.join(" / ")}
        </p>
      </div>
      <div class="text-[0.7rem] font-bold uppercase text-muted max-[1000px]:col-start-2 max-[700px]:row-auto">
        <span
          class={`mr-2 inline-block size-2 rounded-full border ${props.enabled ? "border-success bg-success" : "border-muted bg-transparent"}`}
          aria-hidden="true"
        />
        {props.enabled ? "Enabled" : "Disabled"}
      </div>
      <Button
        class="max-[1000px]:col-start-3 max-[1000px]:row-span-2 max-[1000px]:row-start-1 max-[700px]:col-start-2 max-[700px]:row-auto max-[700px]:justify-self-start"
        variant="secondary"
        type="button"
        disabled={props.disabled || props.testing}
        onClick={props.onTest}
      >
        {props.testing ? "Testing…" : `Test ${props.provider.name}`}
      </Button>
      <Show when={props.result}>
        {(result) => (
          <p
            class={`col-start-2 col-end-[-1] -mt-4 mb-4 grid grid-cols-[130px_minmax(0,1fr)] gap-4 border-l-[3px] px-4 py-2.5 text-xs max-[700px]:col-start-2 max-[700px]:row-auto max-[700px]:mt-0 max-[700px]:grid-cols-1 ${result().ok ? resultClasses.ready : resultClasses.attention}`}
            role="status"
          >
            <strong class="capitalize">
              {result().category.replaceAll("_", " ")}
            </strong>
            <span>{result().message}</span>
          </p>
        )}
      </Show>
    </article>
  );
}
