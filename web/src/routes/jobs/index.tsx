import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute, useNavigate } from "@tanstack/solid-router";
import { For, Show, createSignal } from "solid-js";

import { DirectoryPicker } from "../../components/directory-picker";
import { JobStatus } from "../../components/job-status";
import { RequestError } from "../../components/request-error";
import { Button } from "../../components/ui/button";
import { CountBadge } from "../../components/ui/count-badge";
import { EmptyState } from "../../components/ui/empty-state";
import { FormField } from "../../components/ui/form-field";
import { LoadingState } from "../../components/ui/loading-state";
import { PageHeader } from "../../components/ui/page-header";
import { SectionHeader } from "../../components/ui/section-header";
import { displayPathName } from "../../lib/path";
import {
  ApiError,
  api,
  isJobState,
  isMediaKind,
  type CreateJobRequest,
  type DirectoryRef,
  type IngestionPlacement,
  type IngestionRuleDto,
  type IngestionRuleRequest,
  type JobState,
  type MediaKind,
} from "../../lib/api";

export const Route = createFileRoute("/jobs/")({
  component: () => <JobsPage inbox={false} />,
});

const mediaKinds: Array<{ value: MediaKind; label: string }> = [
  { value: "anime", label: "Anime" },
  { value: "book", label: "Book" },
  { value: "movie", label: "Movie" },
  { value: "music", label: "Music" },
  { value: "television", label: "Television" },
];

const placements: Array<{ value: IngestionPlacement; label: string }> = [
  { value: "move", label: "Move" },
  { value: "copy", label: "Copy" },
  { value: "hardlink", label: "Hard link" },
  { value: "symlink", label: "Symbolic link" },
  { value: "reflink", label: "Reflink" },
];

function isPlacement(value: string): value is IngestionPlacement {
  return placements.some((placement) => placement.value === value);
}

function normalizedPath(path: string): string {
  return path.split("/").filter(Boolean).join("/");
}

function closestRule(
  rules: IngestionRuleDto[],
  selected: DirectoryRef,
): IngestionRuleDto | undefined {
  const selectedPath = normalizedPath(selected.path);
  return rules
    .filter((rule) => {
      if (!rule.enabled || rule.source.root_id !== selected.root_id)
        return false;
      const sourcePath = normalizedPath(rule.source.path);
      return (
        sourcePath === "" ||
        selectedPath === sourcePath ||
        selectedPath.startsWith(`${sourcePath}/`)
      );
    })
    .sort(
      (left, right) =>
        normalizedPath(right.source.path).length -
        normalizedPath(left.source.path).length,
    )[0];
}

const terminalStates = new Set<JobState>([
  "completed",
  "failed",
  "cancelled",
  "interrupted",
]);

const states: Array<{ value: JobState | ""; label: string }> = [
  { value: "", label: "All states" },
  { value: "queued", label: "Queued" },
  { value: "scanning", label: "Scanning" },
  { value: "searching", label: "Searching" },
  { value: "resolving", label: "Resolving" },
  { value: "awaiting_confirmation", label: "Awaiting review" },
  { value: "planning", label: "Plan ready" },
  { value: "writing", label: "Writing" },
  { value: "completed", label: "Completed" },
  { value: "failed", label: "Failed" },
  { value: "cancelled", label: "Cancelled" },
  { value: "interrupted", label: "Interrupted" },
];

interface JobCreation {
  request: CreateJobRequest;
  remember?: {
    source: DirectoryRef;
    rule: IngestionRuleRequest;
  };
}

export function JobsPage(props: { inbox: boolean }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [stateFilter, setStateFilter] = createSignal<JobState | "">("");
  const [mediaKind, setMediaKind] = createSignal<MediaKind | "">("");
  const [source, setSource] = createSignal<DirectoryRef | null>(null);
  const [destination, setDestination] = createSignal<DirectoryRef | null>(null);
  const [placement, setPlacement] = createSignal<IngestionPlacement | "">("");
  const [matchedRule, setMatchedRule] = createSignal<IngestionRuleDto | null>(
    null,
  );
  const [ruleRetry, setRuleRetry] = createSignal(false);
  const [needsSetup, setNeedsSetup] = createSignal(false);
  const [remember, setRemember] = createSignal(true);
  const canCreate = () =>
    mediaKind() !== "" &&
    source() !== null &&
    (ruleRetry() || (destination() !== null && placement() !== ""));

  const jobs = useQuery(() => ({
    queryKey: ["jobs", stateFilter()],
    queryFn: () => {
      const state = stateFilter();
      return api.listJobs({
        limit: 50,
        ...(state === "" ? {} : { state }),
      });
    },
  }));

  const rules = useQuery(() => ({
    queryKey: ["ingestion-rules"],
    queryFn: () => api.listIngestionRules(),
    enabled: props.inbox,
  }));

  const stateOptions = () =>
    states.filter(({ value }) => {
      if (value === "") return true;
      return props.inbox !== terminalStates.has(value);
    });

  const visibleJobs = () =>
    (jobs.data?.jobs ?? []).filter((job) =>
      props.inbox
        ? !terminalStates.has(job.state)
        : terminalStates.has(job.state),
    );

  const createJob = useMutation(() => ({
    mutationFn: async (creation: JobCreation) => {
      if (!creation.remember) return api.createJob(creation.request);
      await api.createIngestionRule(creation.remember.rule);
      return api.createJob({
        rule: "matching",
        source: creation.remember.source,
      });
    },
    onSuccess: (result) => {
      void navigate({
        to: "/jobs/$jobId",
        params: { jobId: String(result.job.id) },
      });
      setMediaKind("");
      setSource(null);
      setDestination(null);
      setPlacement("");
      setMatchedRule(null);
      setRuleRetry(false);
      setNeedsSetup(false);
      setRemember(true);
      void queryClient.invalidateQueries({ queryKey: ["jobs"] });
      void queryClient.invalidateQueries({ queryKey: ["ingestion-rules"] });
    },
    onError: (error, creation) => {
      if (!(error instanceof ApiError) || !("rule" in creation.request)) return;
      if (error.code === "ambiguous_media_kind") {
        setMediaKind("");
        setRuleRetry(true);
        setNeedsSetup(true);
      } else if (error.code === "no_matching_folder_rule") {
        setMediaKind("");
        setDestination(null);
        setPlacement("");
        setMatchedRule(null);
        setRuleRetry(false);
        setNeedsSetup(true);
      }
    },
  }));

  function organize(selected: DirectoryRef) {
    if (createJob.isPending) return;
    createJob.reset();
    const rule = closestRule(rules.data?.rules ?? [], selected);
    setSource(selected);
    setMatchedRule(rule ?? null);
    setRuleRetry(false);
    setNeedsSetup(false);
    setRemember(true);
    setMediaKind(
      rule && rule.media_kind_mode !== "auto" ? rule.media_kind_mode.fixed : "",
    );
    setDestination(rule?.destination ?? null);
    setPlacement(rule?.placement ?? "");
    createJob.mutate({
      request: { rule: "matching", source: selected },
    });
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const kind = mediaKind();
    const selectedSource = source();
    if (kind === "" || selectedSource === null) return;
    if (ruleRetry()) {
      createJob.mutate({
        request: {
          rule: "matching",
          source: selectedSource,
          media_kind: kind,
        },
      });
      return;
    }

    const selectedDestination = destination();
    const method = placement();
    if (selectedDestination === null || method === "") return;
    const request: CreateJobRequest = {
      media_kind: kind,
      source: selectedSource,
      destination: selectedDestination,
      placement: method,
      apply: true,
    };
    createJob.mutate({
      request,
      ...(remember()
        ? {
            remember: {
              source: selectedSource,
              rule: {
                name: displayPathName(selectedSource.path) || "Selected folder",
                source: selectedSource,
                destination: selectedDestination,
                media_kind_mode: { fixed: kind },
                placement: method,
                path_template_override: null,
                enabled: true,
              },
            },
          }
        : {}),
    });
  }

  return (
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        title={props.inbox ? "整理" : "整理记录"}
        description={
          props.inbox
            ? "选择源文件夹，识别作品，确认文件去向后再执行整理。"
            : "查看已结束的整理及失败记录；正在处理的作品在「整理」中。"
        }
        aside={
          <FormField label="State filter">
            <select
              value={stateFilter()}
              onChange={(event) => {
                const value = event.currentTarget.value;
                if (value === "" || isJobState(value)) setStateFilter(value);
              }}
            >
              <For each={stateOptions()}>
                {(state) => <option value={state.value}>{state.label}</option>}
              </For>
            </select>
          </FormField>
        }
      />

      <Show when={props.inbox}>
        <section
          class="my-12 grid grid-cols-[minmax(180px,0.45fr)_minmax(0,1.55fr)] gap-[clamp(2rem,5vw,5rem)] border-y border-line border-t-2 border-t-ink py-8 max-[900px]:grid-cols-1 max-[900px]:gap-6"
          aria-labelledby="create-job-title"
        >
          <h2 class="m-0 font-serif text-3xl font-medium" id="create-job-title">
            新建整理
          </h2>
          <div class="grid gap-5">
            <Show when={rules.isPending}>
              <LoadingState>正在读取整理规则…</LoadingState>
            </Show>
            <Show when={rules.isSuccess}>
              <DirectoryPicker
                label="待整理目录"
                buttonLabel="选择目录并整理"
                value={source()}
                onSelect={organize}
              />
              <p class="m-0 max-w-2xl text-sm leading-6 text-muted">
                选定后会自动套用最具体的 Folder
                规则并开始识别；真正写入前仍需确认。
              </p>
            </Show>

            <Show when={rules.isError}>
              <RequestError error={rules.error} />
            </Show>
            <Show when={createJob.isPending}>
              <p role="status" class="m-0 border-l-4 border-moss pl-4 text-sm">
                正在按「{matchedRule()?.name ?? "当前设置"}」识别所选目录…
              </p>
            </Show>

            <Show when={needsSetup()}>
              <div class="border-t border-line pt-5">
                <p class="mt-0 mb-5 text-sm leading-6 text-muted">
                  {ruleRetry()
                    ? "规则已匹配，但无法唯一识别作品；补充媒体类型后重试。"
                    : "这个目录还没有匹配的 Folder 规则，只需补充一次整理设置。"}
                </p>
                <form
                  class="grid grid-cols-2 gap-5 max-[640px]:grid-cols-1"
                  onSubmit={submit}
                >
                  <FormField label="Media kind">
                    <select
                      required
                      value={mediaKind()}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        if (value === "" || isMediaKind(value)) {
                          setMediaKind(value);
                        }
                      }}
                    >
                      <option value="">Choose a media kind</option>
                      <For each={mediaKinds}>
                        {(kind) => (
                          <option value={kind.value}>{kind.label}</option>
                        )}
                      </For>
                    </select>
                  </FormField>
                  <Show when={!ruleRetry()}>
                    <FormField label="Organization method">
                      <select
                        required
                        value={placement()}
                        onChange={(event) => {
                          const value = event.currentTarget.value;
                          if (value === "" || isPlacement(value)) {
                            setPlacement(value);
                          }
                        }}
                      >
                        <option value="">Choose a method</option>
                        <For each={placements}>
                          {(item) => (
                            <option value={item.value}>{item.label}</option>
                          )}
                        </For>
                      </select>
                    </FormField>
                    <DirectoryPicker
                      label="Destination folder"
                      buttonLabel="Choose destination"
                      value={destination()}
                      onSelect={setDestination}
                    />
                    <label class="col-span-full flex items-center gap-3 text-sm text-muted max-[640px]:col-span-1">
                      <input
                        class="size-[1.05rem] shrink-0 accent-moss"
                        type="checkbox"
                        checked={remember()}
                        onChange={(event) =>
                          setRemember(event.currentTarget.checked)
                        }
                      />
                      <span>
                        <strong class="text-ink">以后沿用这套设置</strong> ·
                        保存为 Folder 规则，下次只需选目录。
                      </span>
                    </label>
                  </Show>
                  <Button
                    class="col-start-2 justify-self-end max-[640px]:col-start-1 max-[640px]:w-full"
                    type="submit"
                    disabled={createJob.isPending || !canCreate()}
                  >
                    {createJob.isPending ? "正在识别…" : "开始识别"}
                  </Button>
                </form>
              </div>
            </Show>

            <Show when={createJob.isError}>
              <RequestError error={createJob.error} />
            </Show>
          </div>
        </section>
      </Show>
      <div class="my-6 flex flex-wrap gap-6 text-sm">
        <Link to="/folders">配置自动整理目录 →</Link>
        <Link to="/library">浏览文件 →</Link>
      </div>
      <section
        class="border-t border-line pt-8"
        aria-labelledby="job-list-title"
      >
        <SectionHeader
          title={props.inbox ? "待处理作品" : "已结束的整理"}
          titleId="job-list-title"
          meta={<CountBadge>{visibleJobs().length} 项</CountBadge>}
        />
        <Show when={jobs.isPending}>
          <LoadingState>Loading jobs…</LoadingState>
        </Show>
        <Show when={jobs.isError}>
          <RequestError error={jobs.error} />
        </Show>
        <Show when={jobs.isSuccess && visibleJobs().length === 0}>
          <EmptyState
            title={
              props.inbox
                ? "暂无待处理作品，请选择源文件夹开始识别"
                : "暂无匹配的整理记录"
            }
          />
        </Show>
        <div class="mt-8 border-t-2 border-ink">
          <p class="text-sm text-muted">
            显示最近 50 条任务中的{props.inbox ? "待处理作品" : "已结束记录"}
            。可按状态筛选查看；这不是完整历史总数。
          </p>
          <For each={visibleJobs()}>
            {(job) => (
              <article class="grid min-h-[98px] grid-cols-[70px_minmax(0,1fr)_auto_90px] items-center gap-5 border-b border-line max-[640px]:grid-cols-[52px_minmax(0,1fr)] max-[640px]:gap-3 max-[640px]:py-4">
                <div class="font-serif text-lg font-medium text-muted">
                  #{job.id}
                </div>
                <div class="min-w-0">
                  <p class="m-0 overflow-hidden text-ellipsis whitespace-nowrap font-serif text-base font-medium">
                    {displayPathName(job.input.input_path)}
                  </p>
                  <p class="mt-1 mb-0 text-xs capitalize text-muted">
                    {job.input.media_kind} · updated{" "}
                    {new Date(job.updated_at_ms).toLocaleString()}
                  </p>
                </div>
                <div class="max-[640px]:col-start-2">
                  <JobStatus state={job.state} />
                </div>
                <Link
                  class="text-xs font-bold no-underline hover:text-moss max-[640px]:col-start-2"
                  to="/jobs/$jobId"
                  params={{ jobId: String(job.id) }}
                >
                  Inspect <span aria-hidden="true">→</span>
                </Link>
              </article>
            )}
          </For>
        </div>
      </section>
    </div>
  );
}
