import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, useNavigate } from "@tanstack/solid-router";
import { For, Show, createSignal } from "solid-js";

import {
  api,
  isMediaKind,
  type CreateScrapeRunRequest,
  type DirectoryRef,
  type MediaKind,
  type ScrapeRunStatus,
} from "../lib/api";
import { DirectoryPicker } from "./directory-picker";
import { RequestError } from "./request-error";
import { Button } from "./ui/button";
import { CountBadge } from "./ui/count-badge";
import { EmptyState } from "./ui/empty-state";
import { FormField } from "./ui/form-field";
import { LoadingState } from "./ui/loading-state";
import { PageHeader } from "./ui/page-header";
import { SectionHeader } from "./ui/section-header";

const mediaKinds: Array<{ value: MediaKind; label: string }> = [
  { value: "movie", label: "电影" },
  { value: "television", label: "剧集" },
  { value: "anime", label: "动画" },
  { value: "music", label: "音乐" },
  { value: "book", label: "书籍" },
];

const runLabels: Record<ScrapeRunStatus, string> = {
  queued: "等待中",
  running: "刮削中",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

function statusClass(status: ScrapeRunStatus): string {
  if (status === "succeeded") return "border-moss text-moss";
  if (status === "failed") return "border-rust text-rust";
  return "border-line text-muted";
}

export function ScrapeRunsPage(props: { create: boolean }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [mediaKind, setMediaKind] = createSignal<MediaKind>("movie");
  const [source, setSource] = createSignal<DirectoryRef | null>(null);
  const [exact, setExact] = createSignal(false);
  const [provider, setProvider] = createSignal("tmdb");
  const [externalId, setExternalId] = createSignal("");
  const exactTargetValid = () => {
    if (!exact()) return true;
    const selectedProvider = provider().trim();
    const id = externalId().trim();
    if (!selectedProvider || !id) return false;
    if (selectedProvider !== "tmdb") return true;
    return (
      (mediaKind() === "movie" || mediaKind() === "television") &&
      /^\d+$/u.test(id) &&
      /[1-9]/u.test(id)
    );
  };

  const runs = useQuery(() => ({
    queryKey: ["scrape-runs"],
    queryFn: () => api.listScrapeRuns(50),
    refetchInterval: 1500,
  }));

  const createRun = useMutation(() => ({
    mutationFn: (request: CreateScrapeRunRequest) =>
      api.createScrapeRun(request),
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: ["scrape-runs"] });
      await navigate({
        to: "/scrapes/$runId",
        params: { runId: String(result.run.id) },
      });
    },
  }));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const selected = source();
    if (!selected) return;
    const request: CreateScrapeRunRequest = {
      media_kind: mediaKind(),
      source: selected,
    };
    if (exact()) {
      const selectedProvider = provider().trim();
      const id = externalId().trim();
      if (!selectedProvider || !id) return;
      request.target = {
        media_kind: mediaKind(),
        provider: selectedProvider,
        external_id: { namespace: selectedProvider, value: id },
      };
    }
    createRun.mutate(request);
  }

  return (
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        eyebrow="Metadata audit"
        title={props.create ? "刮削，然后等结果。" : "刮削审计"}
        description={
          props.create
            ? "选择一个作品目录即可开始。Fixer 会在后台完成匹配、合并和写入；每次结果都会留下可追溯的审计记录。"
            : "按时间查看每次刮削选择了哪个数据源、写入了哪些文件，以及之后发生的修正。"
        }
        aside={
          <div class="border-l-2 border-coral pl-4 font-serif text-lg text-ink">
            不再管理任务，只审计结果。
          </div>
        }
      />

      <Show when={props.create}>
        <section class="my-12 grid grid-cols-[minmax(180px,0.42fr)_minmax(0,1.58fr)] gap-[clamp(2rem,6vw,6rem)] border-y border-line border-t-2 border-t-ink py-8 max-[820px]:grid-cols-1">
          <div>
            <p class="m-0 font-mono text-xs uppercase tracking-[0.14em] text-coral">
              01 / New scrape
            </p>
            <h2 class="mt-3 mb-0 font-serif text-3xl font-medium">选择作品</h2>
          </div>
          <form class="grid gap-6" onSubmit={submit}>
            <div class="grid grid-cols-2 gap-5 max-[640px]:grid-cols-1">
              <FormField label="媒体类型">
                <select
                  value={mediaKind()}
                  onChange={(event) => {
                    const value = event.currentTarget.value;
                    if (isMediaKind(value)) setMediaKind(value);
                  }}
                >
                  <For each={mediaKinds}>
                    {(kind) => <option value={kind.value}>{kind.label}</option>}
                  </For>
                </select>
              </FormField>
              <DirectoryPicker
                label="作品目录"
                buttonLabel="选择目录"
                value={source()}
                onSelect={setSource}
              />
            </div>

            <label class="flex items-center gap-3 border-t border-line pt-5 text-sm text-muted">
              <input
                class="size-[1.05rem] accent-coral"
                type="checkbox"
                checked={exact()}
                onChange={(event) => setExact(event.currentTarget.checked)}
              />
              <span>
                <strong class="text-ink">指定数据源 ID</strong> ·
                跳过搜索，直接按 TMDB 等 provider ID 刮削
              </span>
            </label>

            <Show when={exact()}>
              <div class="grid grid-cols-2 gap-5 max-[640px]:grid-cols-1">
                <FormField label="Provider" hint="电影和剧集通常使用 tmdb">
                  <input
                    required
                    value={provider()}
                    onInput={(event) => setProvider(event.currentTarget.value)}
                  />
                </FormField>
                <FormField
                  label="Provider ID"
                  hint="例如 329865，不是搜索关键词"
                >
                  <input
                    required
                    value={externalId()}
                    onInput={(event) =>
                      setExternalId(event.currentTarget.value)
                    }
                  />
                </FormField>
              </div>
              <Show when={!exactTargetValid()}>
                <p class="m-0 text-sm text-rust" role="alert">
                  TMDB 仅支持电影或剧集，并且 ID 必须是大于零的数字。
                </p>
              </Show>
            </Show>

            <div class="flex items-center justify-between gap-5 max-[640px]:items-stretch">
              <p class="m-0 max-w-xl text-sm leading-6 text-muted">
                提交后可以离开此页；刮削会继续运行，结果写入审计记录。
              </p>
              <Button
                class="shrink-0 max-[640px]:w-full"
                type="submit"
                disabled={
                  createRun.isPending ||
                  source() === null ||
                  !exactTargetValid()
                }
              >
                {createRun.isPending ? "正在提交…" : "开始刮削"}
              </Button>
            </div>
            <Show when={createRun.isError}>
              <RequestError error={createRun.error} />
            </Show>
          </form>
        </section>
      </Show>

      <section
        class="mt-12 border-t border-line pt-8"
        aria-labelledby="runs-title"
      >
        <SectionHeader
          title="最近的刮削"
          titleId="runs-title"
          meta={<CountBadge>{runs.data?.runs.length ?? 0} 次</CountBadge>}
        />
        <Show when={runs.isPending}>
          <LoadingState>正在读取刮削记录…</LoadingState>
        </Show>
        <Show when={runs.isError}>
          <RequestError error={runs.error} />
        </Show>
        <Show when={runs.isSuccess && runs.data.runs.length === 0}>
          <EmptyState title="还没有刮削记录" />
        </Show>
        <div class="mt-8 border-t-2 border-ink">
          <For each={runs.data?.runs ?? []}>
            {(run) => (
              <article class="grid min-h-[104px] grid-cols-[64px_minmax(0,1fr)_auto_auto] items-center gap-5 border-b border-line max-[680px]:grid-cols-[48px_minmax(0,1fr)] max-[680px]:py-4">
                <span class="font-mono text-xs text-muted">
                  {String(run.id).padStart(3, "0")}
                </span>
                <div class="min-w-0">
                  <h3 class="m-0 truncate font-serif text-lg font-medium">
                    {run.item_name}
                  </h3>
                  <p class="mt-1 mb-0 text-xs text-muted">
                    {run.media_kind} ·{" "}
                    {run.selected_target?.provider ?? "等待选择数据源"} ·{" "}
                    {new Date(run.updated_at_ms).toLocaleString()}
                  </p>
                </div>
                <span
                  class={`border px-2.5 py-1 text-[0.68rem] font-bold tracking-[0.08em] ${statusClass(run.status)}`}
                >
                  {runLabels[run.status]}
                </span>
                <Link
                  class="text-xs font-bold no-underline hover:text-coral max-[680px]:col-start-2"
                  to="/scrapes/$runId"
                  params={{ runId: String(run.id) }}
                >
                  审计 <span aria-hidden="true">→</span>
                </Link>
              </article>
            )}
          </For>
        </div>
      </section>
    </div>
  );
}
