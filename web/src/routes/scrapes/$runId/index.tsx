import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute, useNavigate } from "@tanstack/solid-router";
import { For, Show, createSignal } from "solid-js";

import { RequestError } from "../../../components/request-error";
import { Button } from "../../../components/ui/button";
import { LoadingState } from "../../../components/ui/loading-state";
import {
  api,
  type ProviderTarget,
  type ScrapeRunStatus,
} from "../../../lib/api";

export const Route = createFileRoute("/scrapes/$runId/")({
  component: ScrapeRunDetailPage,
});

const labels: Record<ScrapeRunStatus, string> = {
  queued: "等待中",
  running: "刮削中",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

function targetLabel(target: ProviderTarget | undefined): string {
  if (!target) return "尚未选择";
  return `${target.provider}:${target.external_id.value}`;
}

function ScrapeRunDetailPage() {
  const params = Route.useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const runId = () => Number(params().runId);
  const [provider, setProvider] = createSignal("tmdb");
  const [externalId, setExternalId] = createSignal("");

  const run = useQuery(() => ({
    queryKey: ["scrape-run", runId()],
    queryFn: () => api.getScrapeRun(runId()),
    refetchInterval: 1200,
  }));

  const cancel = useMutation(() => ({
    mutationFn: () => api.cancelScrapeRun(runId()),
    onSuccess: (result) => {
      queryClient.setQueryData(["scrape-run", runId()], result);
      void queryClient.invalidateQueries({ queryKey: ["scrape-runs"] });
    },
  }));

  const retry = useMutation(() => ({
    mutationFn: () => api.retryScrapeRun(runId()),
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: ["scrape-runs"] });
      await navigate({
        to: "/scrapes/$runId",
        params: { runId: String(result.run.id) },
      });
    },
  }));

  const correct = useMutation(() => ({
    mutationFn: () => {
      const current = run.data?.run;
      if (!current) throw new Error("Scrape result is unavailable");
      const selectedProvider = provider().trim();
      const id = externalId().trim();
      if (!selectedProvider || !id)
        throw new Error("Provider and ID are required");
      return api.createScrapeRun({
        correction_of: current.id,
        target: {
          media_kind: current.media_kind,
          provider: selectedProvider,
          external_id: { namespace: selectedProvider, value: id },
        },
      });
    },
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: ["scrape-runs"] });
      await navigate({
        to: "/scrapes/$runId",
        params: { runId: String(result.run.id) },
      });
    },
  }));

  return (
    <div class="mx-auto max-w-[1080px]">
      <Link
        class="mb-8 inline-block text-xs font-bold uppercase tracking-[0.08em] text-muted underline underline-offset-4 hover:text-coral"
        to="/scrapes"
      >
        ← 刮削审计
      </Link>

      <Show when={run.isPending}>
        <LoadingState>正在读取刮削结果…</LoadingState>
      </Show>
      <Show when={run.isError}>
        <RequestError error={run.error} />
      </Show>

      <Show when={run.data?.run}>
        {(current) => (
          <>
            <header class="grid grid-cols-[minmax(0,1fr)_auto] items-end gap-10 border-b border-line pb-10 max-[700px]:grid-cols-1">
              <div>
                <p class="mb-3 font-mono text-xs uppercase tracking-[0.14em] text-coral">
                  Scrape audit / {String(current().id).padStart(3, "0")}
                </p>
                <h1 class="m-0 font-serif text-[clamp(2.6rem,6vw,5rem)] font-medium leading-[0.95] tracking-[-0.035em] text-balance">
                  {current().item_name}
                </h1>
                <p class="mt-5 mb-0 text-sm text-muted">
                  {current().media_kind} ·{" "}
                  {new Date(current().created_at_ms).toLocaleString()}
                </p>
              </div>
              <span class="border-2 border-ink px-4 py-2 text-xs font-bold tracking-[0.1em]">
                {labels[current().status]}
              </span>
            </header>

            <section class="grid grid-cols-3 border-b border-line max-[760px]:grid-cols-1">
              <div class="border-r border-line py-6 pr-6 max-[760px]:border-r-0 max-[760px]:border-b">
                <p class="m-0 text-[0.68rem] font-bold uppercase tracking-[0.13em] text-muted">
                  Requested
                </p>
                <p class="mt-2 mb-0 font-mono text-sm">
                  {targetLabel(current().requested_target)}
                </p>
              </div>
              <div class="border-r border-line px-6 py-6 max-[760px]:border-r-0 max-[760px]:border-b max-[760px]:px-0">
                <p class="m-0 text-[0.68rem] font-bold uppercase tracking-[0.13em] text-muted">
                  Selected
                </p>
                <p class="mt-2 mb-0 font-mono text-sm">
                  {targetLabel(current().selected_target)}
                </p>
              </div>
              <div class="py-6 pl-6 max-[760px]:pl-0">
                <p class="m-0 text-[0.68rem] font-bold uppercase tracking-[0.13em] text-muted">
                  Diagnostics
                </p>
                <p class="mt-2 mb-0 text-sm">
                  {current().candidate_count} 个候选 ·{" "}
                  {current().conflict_count} 个冲突
                </p>
              </div>
            </section>

            <Show when={current().correction_of !== undefined}>
              <p class="my-6 border-l-4 border-coral pl-4 text-sm">
                这是对第 {current().correction_of}{" "}
                次刮削的修正；只会替换该次刮削写入且之后未改变的文件。
              </p>
            </Show>
            <Show when={current().retry_of !== undefined}>
              <p class="my-6 border-l-4 border-moss pl-4 text-sm">
                这是第 {current().retry_of}{" "}
                次刮削的重试；只会接续替换该次已写入的文件。
              </p>
            </Show>

            <Show
              when={
                current().status === "queued" || current().status === "running"
              }
            >
              <section class="my-12 grid min-h-52 place-items-center border-y border-line bg-[repeating-linear-gradient(135deg,transparent,transparent_18px,color-mix(in_srgb,var(--color-line)_32%,transparent)_18px,color-mix(in_srgb,var(--color-line)_32%,transparent)_19px)] text-center">
                <div>
                  <p class="m-0 font-serif text-2xl">后台正在刮削</p>
                  <p class="mt-2 text-sm text-muted">
                    你可以离开这个页面，完成后记录会自动更新。
                  </p>
                </div>
              </section>
            </Show>

            <Show when={current().execution}>
              {(execution) => (
                <section class="mt-12" aria-labelledby="outputs-title">
                  <div class="flex items-end justify-between gap-5 border-b-2 border-ink pb-4">
                    <h2
                      id="outputs-title"
                      class="m-0 font-serif text-3xl font-medium"
                    >
                      写入审计
                    </h2>
                    <span class="text-xs text-muted">
                      {execution().completed_operations} 成功 ·{" "}
                      {execution().failed_operations} 失败
                    </span>
                  </div>
                  <Show when={execution().failure}>
                    {(failure) => (
                      <div
                        class="my-5 border-l-4 border-rust pl-4"
                        role="alert"
                      >
                        <strong>{failure().message}</strong>
                        <code class="mt-1 block text-xs text-muted">
                          {failure().phase === undefined
                            ? failure().code
                            : `${failure().phase} · ${failure().code}`}
                        </code>
                      </div>
                    )}
                  </Show>
                  <div>
                    <For each={execution().operations ?? []}>
                      {(operation) => (
                        <article class="grid grid-cols-[54px_minmax(0,1fr)_auto] gap-4 border-b border-line py-5 max-[640px]:grid-cols-[42px_minmax(0,1fr)]">
                          <span class="font-mono text-xs text-muted">
                            {String(operation.operation_index + 1).padStart(
                              2,
                              "0",
                            )}
                          </span>
                          <div class="min-w-0">
                            <p class="m-0 break-all text-sm">
                              {operation.destination}
                            </p>
                            <p class="mt-1 mb-0 font-mono text-[0.68rem] text-muted">
                              {operation.kind}
                              {operation.source === undefined
                                ? ""
                                : ` ← ${operation.source}`}
                            </p>
                            <Show when={operation.fingerprint}>
                              {(fingerprint) => (
                                <code class="mt-2 block overflow-hidden text-ellipsis text-[0.65rem] text-muted">
                                  sha256:{fingerprint()}
                                </code>
                              )}
                            </Show>
                          </div>
                          <span class="text-[0.68rem] font-bold uppercase tracking-[0.1em]">
                            {operation.outcome}
                          </span>
                        </article>
                      )}
                    </For>
                  </div>
                </section>
              )}
            </Show>

            <Show when={current().status === "succeeded"}>
              <section class="mt-14 grid grid-cols-[minmax(180px,0.5fr)_minmax(0,1.5fr)] gap-10 border-y border-line py-8 max-[760px]:grid-cols-1">
                <div>
                  <p class="m-0 font-mono text-xs uppercase tracking-[0.14em] text-coral">
                    Correction
                  </p>
                  <h2 class="mt-3 font-serif text-2xl font-medium">
                    结果不对？指定 ID 修正
                  </h2>
                </div>
                <form
                  class="grid grid-cols-2 gap-5 max-[600px]:grid-cols-1"
                  onSubmit={(event) => {
                    event.preventDefault();
                    correct.mutate();
                  }}
                >
                  <label class="grid gap-2 text-sm font-medium text-muted">
                    Provider
                    <input
                      class="min-h-11 border border-line bg-surface px-3 text-ink"
                      required
                      value={provider()}
                      onInput={(event) =>
                        setProvider(event.currentTarget.value)
                      }
                    />
                  </label>
                  <label class="grid gap-2 text-sm font-medium text-muted">
                    Provider ID
                    <input
                      class="min-h-11 border border-line bg-surface px-3 text-ink"
                      required
                      value={externalId()}
                      onInput={(event) =>
                        setExternalId(event.currentTarget.value)
                      }
                    />
                  </label>
                  <Button
                    class="col-start-2 justify-self-end max-[600px]:col-start-1 max-[600px]:w-full"
                    type="submit"
                    disabled={correct.isPending}
                  >
                    {correct.isPending ? "正在提交修正…" : "创建修正刮削"}
                  </Button>
                </form>
                <Show when={correct.isError}>
                  <RequestError error={correct.error} />
                </Show>
              </section>
            </Show>

            <section class="mt-10 flex gap-3 border-t border-line pt-6">
              <Show
                when={
                  current().status === "queued" ||
                  current().status === "running"
                }
              >
                <Button
                  variant="secondary"
                  type="button"
                  disabled={cancel.isPending}
                  onClick={() => {
                    cancel.mutate();
                  }}
                >
                  {cancel.isPending ? "正在取消…" : "取消"}
                </Button>
              </Show>
              <Show
                when={
                  current().status === "failed" ||
                  current().status === "cancelled"
                }
              >
                <Button
                  type="button"
                  disabled={retry.isPending}
                  onClick={() => {
                    retry.mutate();
                  }}
                >
                  {retry.isPending ? "正在重试…" : "重新刮削"}
                </Button>
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
