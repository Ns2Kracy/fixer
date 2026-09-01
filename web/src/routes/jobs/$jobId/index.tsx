import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute } from "@tanstack/solid-router";
import { Show, createSignal, onSettled } from "solid-js";

import { JobStatus } from "../../../components/job-status";
import { ProgressTimeline } from "../../../components/progress-timeline";
import { RequestError } from "../../../components/request-error";
import { Button, buttonStyles } from "../../../components/ui/button";
import { LoadingState } from "../../../components/ui/loading-state";
import { SectionHeader } from "../../../components/ui/section-header";
import { api, type JobState } from "../../../lib/api";
import {
  connectJobEvents,
  type JobEventConnectionState,
} from "../../../lib/sse";

export const Route = createFileRoute("/jobs/$jobId/")({
  component: JobDetailPage,
});

const cancellableStates = new Set<JobState>([
  "queued",
  "scanning",
  "searching",
  "resolving",
  "awaiting_confirmation",
  "planning",
]);

function JobDetailPage() {
  const params = Route.useParams();
  const queryClient = useQueryClient();
  const jobId = () => Number(params().jobId);
  const [connectionState, setConnectionState] =
    createSignal<JobEventConnectionState>("connecting");

  const job = useQuery(() => ({
    queryKey: ["job", jobId()],
    queryFn: () => api.getJob(jobId()),
  }));

  const cancel = useMutation(() => ({
    mutationFn: () => api.cancelJob(jobId()),
    onSuccess: (result) => queryClient.setQueryData(["job", jobId()], result),
  }));
  const retry = useMutation(() => ({
    mutationFn: () => api.retryJob(jobId()),
    onSuccess: (result) => queryClient.setQueryData(["job", jobId()], result),
  }));

  onSettled(() => {
    const connection = connectJobEvents(jobId(), {
      onEvent: () => undefined,
      reconcile: () => job.refetch().then(() => undefined),
      onConnectionChange: setConnectionState,
    });
    return () => connection.close();
  });

  return (
    <div class="mx-auto max-w-[1180px]">
      <Link
        class="mb-8 inline-block text-xs font-bold uppercase tracking-[0.06em] text-muted underline decoration-1 underline-offset-4 hover:text-moss"
        to="/jobs"
      >
        ← All jobs
      </Link>
      <Show when={job.isPending}>
        <LoadingState>Loading job…</LoadingState>
      </Show>
      <Show when={job.isError}>
        <RequestError error={job.error} />
      </Show>
      <Show when={job.data?.job}>
        {(current) => (
          <>
            <header class="flex items-end justify-between gap-12 border-b border-line pt-4 pb-12 max-[900px]:flex-col max-[900px]:items-start max-[900px]:gap-6">
              <div>
                <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
                  Job / #{current().id}
                </p>
                <h1 class="m-0 max-w-[900px] font-serif text-[clamp(2.6rem,5vw,4.8rem)] leading-[0.94] font-medium tracking-[-0.04em] wrap-anywhere">
                  {current().input.input_path}
                </h1>
                <p class="mt-6 mb-0 max-w-[680px] text-muted">
                  {current().input.media_kind} ·{" "}
                  {current().input.apply
                    ? "approved writes available"
                    : "dry run only"}
                </p>
              </div>
              <div class="grid shrink-0 justify-items-end gap-2 max-[900px]:justify-items-start">
                <JobStatus state={current().state} />
                <small
                  class="text-[0.7rem] capitalize text-muted"
                  role="status"
                  aria-live="polite"
                >
                  Events: {connectionState()}
                </small>
              </div>
            </header>
            <section
              class="mt-12 border-t-2 border-ink pt-8"
              aria-labelledby="progress-title"
            >
              <SectionHeader
                eyebrow="Pipeline"
                title="Job progress"
                titleId="progress-title"
              />
              <ProgressTimeline
                state={current().state}
                progress={current().progress}
              />
            </section>
            <section
              class="mt-12 flex items-start gap-4 border-t border-line pt-8 max-[640px]:flex-col"
              aria-label="Job actions"
            >
              <Show when={current().state === "awaiting_confirmation"}>
                <Link
                  class={buttonStyles()}
                  to="/jobs/$jobId/review"
                  params={{ jobId: params().jobId }}
                >
                  Review metadata
                </Link>
              </Show>
              <Show when={current().state === "planning"}>
                <Link
                  class={buttonStyles()}
                  to="/jobs/$jobId/plan"
                  params={{ jobId: params().jobId }}
                >
                  Review output plan
                </Link>
              </Show>
              <Show when={current().state === "interrupted"}>
                <div class="grid max-w-[290px] gap-2">
                  <Button
                    class="justify-self-start"
                    type="button"
                    disabled={retry.isPending}
                    onClick={() => retry.mutate()}
                  >
                    {retry.isPending ? "Retrying…" : "Retry job"}
                  </Button>
                  <small class="text-xs text-muted">
                    Restarts scanning from the beginning; no write resumes
                    automatically.
                  </small>
                </div>
              </Show>
              <Show when={cancellableStates.has(current().state)}>
                <div class="grid max-w-[290px] gap-2">
                  <Button
                    class="justify-self-start"
                    variant="secondary"
                    type="button"
                    disabled={cancel.isPending}
                    onClick={() => cancel.mutate()}
                  >
                    {cancel.isPending ? "Cancelling…" : "Cancel before writing"}
                  </Button>
                  <small class="text-xs text-muted">
                    Stops processing before writing. Writing jobs cannot be
                    cancelled.
                  </small>
                </div>
              </Show>
            </section>
            <Show when={cancel.isError}>
              <RequestError error={cancel.error} />
            </Show>
            <Show when={retry.isError}>
              <RequestError error={retry.error} />
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}
