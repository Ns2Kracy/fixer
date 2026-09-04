import { useQuery } from "@tanstack/solid-query";
import { Link, createFileRoute } from "@tanstack/solid-router";
import { Show } from "solid-js";

import { ApiError, api } from "../lib/api";
import { buttonStyles } from "../components/ui/button";
import { CountBadge } from "../components/ui/count-badge";
import { EmptyState } from "../components/ui/empty-state";
import { Notice } from "../components/ui/notice";
import { SectionHeader } from "../components/ui/section-header";

export const Route = createFileRoute("/")({
  component: OverviewPage,
});

function OverviewPage() {
  const health = useQuery(() => ({
    queryKey: ["health"],
    queryFn: () => api.health(),
  }));

  return (
    <div class="mx-auto max-w-[1150px]">
      <section
        class="grid grid-cols-[minmax(0,1.45fr)_minmax(280px,0.55fr)] items-end gap-[clamp(3rem,8vw,8rem)] pt-4 pb-16 max-[800px]:grid-cols-1 max-[800px]:gap-12"
        aria-labelledby="overview-title"
      >
        <div>
          <h1
            class="m-0 font-serif text-[clamp(3.1rem,7vw,6rem)] leading-[0.91] font-medium tracking-[-0.04em]"
            id="overview-title"
          >
            Overview
          </h1>
          <div class="mt-8 flex flex-wrap items-center gap-6">
            <Link class={buttonStyles()} to="/folders" preload="intent">
              Manage folders
            </Link>
            <Link class="font-bold no-underline hover:text-moss" to="/jobs">
              View jobs <span aria-hidden="true">→</span>
            </Link>
          </div>
        </div>
        <div
          class="min-h-[190px] border-t-2 border-ink pt-4"
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
              Connecting…
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
          title="Recent work"
          titleId="activity-title"
          meta={<CountBadge>0 active</CountBadge>}
        />
        <EmptyState glyph="◇" title="No jobs yet" />
      </section>
    </div>
  );
}

function ApiFailure(props: { error: Error | null }) {
  const requestId = () =>
    props.error instanceof ApiError ? props.error.requestId : undefined;

  return (
    <Notice tone="danger" role="alert">
      <strong>
        {props.error?.message ?? "The server could not be reached"}
      </strong>
      <p class="my-1">Check the server and try again.</p>
      <Show when={requestId()}>
        {(id) => <code class="text-xs">Request {id()}</code>}
      </Show>
    </Notice>
  );
}
