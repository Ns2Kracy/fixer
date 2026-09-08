import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute } from "@tanstack/solid-router";
import { Show, createSignal, onSettled } from "solid-js";

import { JobStatus } from "../../../components/job-status";
import { ProgressTimeline } from "../../../components/progress-timeline";
import { RequestError } from "../../../components/request-error";
import { Button } from "../../../components/ui/button";
import { LoadingState } from "../../../components/ui/loading-state";
import { ReviewPanel } from "./review";
import { PlanPanel } from "./plan";
import { api, type JobState } from "../../../lib/api";
import { displayPathName } from "../../../lib/path";
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
      onEvent: () => {},
      reconcile: async () => {
        await job.refetch();
      },
      onConnectionChange: setConnectionState,
    });
    return () => {
      connection.close();
    };
  });

  return (
    <div class="mx-auto max-w-[1180px]">
      <Link
        class="mb-8 inline-block text-xs font-bold uppercase tracking-[0.06em] text-muted underline decoration-1 underline-offset-4 hover:text-moss"
        to="/jobs"
      >
        ← 整理记录
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
                  整理 #{current().id}
                </p>
                <h1 class="m-0 max-w-[900px] text-3xl leading-tight font-semibold wrap-anywhere">
                  {displayPathName(current().input.input_path)}
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
            <dl class="my-6 grid gap-4 text-sm">
              <div>
                <dt class="text-muted">源文件</dt>
                <dd class="m-0 break-all">{current().input.input_path}</dd>
              </div>
              <Show when={current().input.organization}>
                {(organization) => (
                  <div>
                    <dt class="text-muted">
                      目标目录 · {organization().placement}
                    </dt>
                    <dd class="m-0 break-all">
                      {organization().destination_path}
                    </dd>
                  </div>
                )}
              </Show>
            </dl>
            <Show when={current().state === "awaiting_confirmation"}>
              <ReviewPanel jobId={jobId()} embedded />
            </Show>
            <Show when={current().state === "planning"}>
              <PlanPanel jobId={jobId()} embedded />
            </Show>
            <Show when={current().execution}>
              {(execution) => (
                <section class="my-8 border-y border-line py-6">
                  <h2 class="text-xl font-semibold">整理结果</h2>
                  <p>
                    已完成 {execution().completed_operations} 项操作 · 失败{" "}
                    {execution().failed_operations} 项
                  </p>
                  <p class="text-sm text-muted">
                    这是已保存的执行汇总。此版本尚未保存逐文件历史及完整元数据快照。
                  </p>
                </section>
              )}
            </Show>
            <details class="my-8 border-y border-line py-4">
              <summary class="cursor-pointer text-sm font-semibold">
                技术详情 · 处理流水线
              </summary>
              <ProgressTimeline
                state={current().state}
                progress={current().progress}
              />
            </details>
            <Show when={current().execution?.failure}>
              {(failure) => (
                <section
                  class="mt-8 border-l-4 border-rust bg-rust/8 px-5 py-4"
                  role="alert"
                  aria-labelledby="execution-failure-title"
                >
                  <p
                    id="execution-failure-title"
                    class="m-0 font-bold text-rust"
                  >
                    Output operation failed
                    <Show when={failure().operation_index !== undefined}>
                      {` at step ${failure().operation_index! + 1}`}
                    </Show>
                  </p>
                  <p class="mt-2 mb-0">{failure().message}</p>
                  <code class="mt-3 inline-block text-xs text-muted">
                    {failure().code}
                  </code>
                </section>
              )}
            </Show>
            <section
              class="mt-12 flex items-start gap-4 border-t border-line pt-8 max-[640px]:flex-col"
              aria-label="Job actions"
            >
              <Show when={current().state === "interrupted"}>
                <div class="grid max-w-[290px] gap-2">
                  <Button
                    class="justify-self-start"
                    type="button"
                    disabled={retry.isPending}
                    onClick={() => {
                      retry.mutate();
                    }}
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
                    onClick={() => {
                      cancel.mutate();
                    }}
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
