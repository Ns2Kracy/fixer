import { useQuery } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";
import { Show } from "solid-js";

import { CountBadge } from "../components/ui/count-badge";
import { EmptyState } from "../components/ui/empty-state";
import { SectionHeader } from "../components/ui/section-header";
import { buttonStyles } from "../components/ui/button";
import { ApiError, api } from "../lib/api";

export const Route = createFileRoute("/")({
  component: Workspace,
});

function Workspace() {
  const health = useQuery(() => ({
    queryKey: ["health"],
    queryFn: () => api.health(),
  }));

  return (
    <div class="mx-auto max-w-[1150px]">
      <section
        class="grid grid-cols-[minmax(0,1.45fr)_minmax(280px,0.55fr)] items-end gap-[clamp(3rem,8vw,8rem)] pt-4 pb-24 max-[800px]:grid-cols-1 max-[800px]:gap-12 max-[800px]:pb-16"
        aria-labelledby="workspace-title"
      >
        <div>
          <p class="mb-4 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            Workspace / Overview
          </p>
          <h1
            class="m-0 max-w-[800px] font-serif text-[clamp(3.1rem,7vw,6.7rem)] leading-[0.91] font-medium tracking-[-0.055em] max-[480px]:text-[3.15rem]"
            id="workspace-title"
            aria-label="Metadata work, without guesswork."
          >
            Metadata work,
            <br />
            <em class="font-normal text-moss">without guesswork.</em>
          </h1>
          <p class="my-8 max-w-[590px] text-[clamp(1rem,1.5vw,1.22rem)] text-muted">
            Inspect evidence, resolve conflicts, and approve every filesystem
            change before Fixer writes a byte.
          </p>
          <div class="flex items-center gap-6 max-[480px]:flex-col max-[480px]:items-start max-[480px]:gap-4">
            <a class={buttonStyles()} href="#activity-title">
              Review workspace
            </a>
          </div>
        </div>
        <div
          class="min-h-[230px] border-t-2 border-ink pt-4"
          aria-live="polite"
        >
          <p class="mb-4 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            System status
          </p>
          <Show when={health.isPending}>
            <p class="my-6 font-serif text-[1.35rem] font-medium">
              <span
                class="mr-3 inline-block size-[9px] rounded-full bg-muted"
                aria-hidden="true"
              />
              Connecting to server…
            </p>
          </Show>
          <Show when={health.isSuccess}>
            <p class="my-6 font-serif text-[1.35rem] font-medium">
              <span
                class="mr-3 inline-block size-[9px] rounded-full bg-success"
                aria-hidden="true"
              />
              Server connected
            </p>
            <dl class="m-0">
              <div class="flex justify-between border-t border-line py-3 text-xs">
                <dt class="text-muted">API schema</dt>
                <dd class="m-0 font-semibold">
                  v{health.data?.schema_version}
                </dd>
              </div>
              <div class="flex justify-between border-t border-line py-3 text-xs">
                <dt class="text-muted">Server</dt>
                <dd class="m-0 font-semibold">{health.data?.version}</dd>
              </div>
              <div class="flex justify-between border-t border-line py-3 text-xs">
                <dt class="text-muted">Write mode</dt>
                <dd class="m-0 font-semibold">Approval only</dd>
              </div>
            </dl>
          </Show>
          <Show when={health.isError}>
            <ApiFailure error={health.error} />
          </Show>
        </div>
      </section>
      <section
        class="border-t border-line pt-8"
        aria-labelledby="activity-title"
      >
        <SectionHeader
          eyebrow="Queue"
          title="Recent work"
          titleId="activity-title"
          meta={<CountBadge>0 active</CountBadge>}
        />
        <EmptyState
          glyph="◇"
          title="No jobs yet"
          description="New scans and review sessions will appear here."
        />
      </section>
    </div>
  );
}

function ApiFailure(props: { error: Error | null }) {
  const requestId = () =>
    props.error instanceof ApiError ? props.error.requestId : undefined;

  return (
    <div class="border-l-[3px] border-coral pl-4" role="alert">
      <strong>
        {props.error?.message ?? "The server could not be reached"}
      </strong>
      <p class="my-1">Check the server and try again.</p>
      <Show when={requestId()}>
        {(id) => <code class="text-xs">Request {id()}</code>}
      </Show>
    </div>
  );
}
