import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute } from "@tanstack/solid-router";
import { Show, createSignal } from "solid-js";

import { OutputDiff } from "../../../components/output-diff";
import { RequestError } from "../../../components/request-error";
import { Button } from "../../../components/ui/button";
import { LoadingState } from "../../../components/ui/loading-state";
import { PageHeader } from "../../../components/ui/page-header";
import { api } from "../../../lib/api";

export const Route = createFileRoute("/jobs/$jobId/plan")({
  component: PlanPage,
});

function PlanPage() {
  const params = Route.useParams();
  const queryClient = useQueryClient();
  const jobId = () => Number(params().jobId);
  const [approved, setApproved] = createSignal(false);
  const executionNonce = crypto.randomUUID();

  const job = useQuery(() => ({
    queryKey: ["job", jobId()],
    queryFn: () => api.getJob(jobId()),
  }));

  const plan = useQuery(() => ({
    queryKey: ["job-plan", jobId()],
    queryFn: () => api.getJobPlan(jobId()),
  }));

  const execute = useMutation(() => ({
    mutationFn: () =>
      api.executeJob(jobId(), `job-${jobId()}-${executionNonce}`),
    onSuccess: (result) => queryClient.setQueryData(["job", jobId()], result),
  }));

  return (
    <div class="mx-auto max-w-[1180px]">
      <Link
        class="mb-8 inline-block text-xs font-bold uppercase tracking-[0.06em] text-muted underline decoration-1 underline-offset-4 hover:text-moss"
        to="/jobs/$jobId/review"
        params={{ jobId: params().jobId }}
      >
        ← Metadata review
      </Link>
      <PageHeader
        variant="detail"
        eyebrow={<>Job / #{params().jobId} / Filesystem</>}
        title="Output plan"
        description="Review targets and operation types. File contents stay server-owned and are never returned here."
      />
      <Show when={plan.isPending || job.isPending}>
        <LoadingState>Loading output operations…</LoadingState>
      </Show>
      <Show when={plan.isError}>
        <RequestError error={plan.error} />
      </Show>
      <Show when={job.isError}>
        <RequestError error={job.error} />
      </Show>
      <Show when={plan.data}>
        {(data) => (
          <>
            <OutputDiff
              operations={data().operations}
              outputRoot={data().output_root}
            />
            <Show when={data().operations_truncated}>
              <p
                class="my-4 border-l-[3px] border-coral bg-danger-surface px-4 py-3"
                role="alert"
              >
                The operation list is incomplete. Execution is disabled.
              </p>
            </Show>
            <Show when={job.data && !job.data.job.input.apply}>
              <p class="mt-8 border-l-[3px] border-success bg-success-surface px-4 py-3">
                This is a dry-run job. The plan cannot be executed.
              </p>
            </Show>
            <Show
              when={
                job.data &&
                job.data.job.input.apply &&
                job.data.job.state !== "planning"
              }
            >
              <p class="mt-8 border-l-[3px] border-success bg-success-surface px-4 py-3">
                Execution is unavailable while the job is {job.data?.job.state}.
              </p>
            </Show>
            <div class="mt-16 grid grid-cols-[minmax(0,1fr)_auto] gap-x-12 gap-y-6 border-t-4 border-coral bg-code p-8 text-code-ink max-[640px]:grid-cols-1 max-[640px]:p-6">
              <div>
                <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-code-muted">
                  Final gate
                </p>
                <h2 class="m-0 font-serif text-3xl font-medium">
                  Filesystem approval
                </h2>
                <p class="mt-2 mb-0 text-code-muted">
                  This authorizes only the bounded server-owned plan shown here.
                </p>
              </div>
              <label class="flex max-w-[340px] self-center items-start gap-3 text-sm">
                <input
                  class="mt-[0.15rem] size-[1.05rem] shrink-0 accent-coral"
                  type="checkbox"
                  checked={approved()}
                  disabled={
                    !data().requires_approval ||
                    data().operations_truncated ||
                    !job.data?.job.input.apply ||
                    job.data?.job.state !== "planning" ||
                    execute.isSuccess
                  }
                  onChange={(event) => setApproved(event.currentTarget.checked)}
                />
                I approve these filesystem operations
              </label>
              <Button
                class="col-start-2 justify-self-end max-[640px]:col-start-1 max-[640px]:w-full"
                variant="danger"
                type="button"
                disabled={
                  !approved() ||
                  data().operations_truncated ||
                  !job.data?.job.input.apply ||
                  job.data?.job.state !== "planning" ||
                  execute.isPending ||
                  execute.isSuccess
                }
                onClick={() => execute.mutate()}
              >
                {execute.isPending
                  ? "Executing…"
                  : execute.isSuccess
                    ? "Execution requested"
                    : "Execute approved plan"}
              </Button>
            </div>
            <Show when={execute.isSuccess}>
              <p
                class="mt-8 border-l-[3px] border-success bg-success-surface px-4 py-3"
                role="status"
              >
                Execution was accepted. Live progress is available on the job
                page.
              </p>
            </Show>
            <Show when={execute.isError}>
              <RequestError error={execute.error} />
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}
