import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute } from "@tanstack/solid-router";
import { For, Show, createSignal } from "solid-js";

import { JobStatus } from "../../components/job-status";
import { RequestError } from "../../components/request-error";
import { Button } from "../../components/ui/button";
import { CountBadge } from "../../components/ui/count-badge";
import { EmptyState } from "../../components/ui/empty-state";
import { FormField } from "../../components/ui/form-field";
import { LoadingState } from "../../components/ui/loading-state";
import { PageHeader } from "../../components/ui/page-header";
import { SectionHeader } from "../../components/ui/section-header";
import {
  api,
  type CreateJobRequest,
  type JobState,
  type MediaKind,
} from "../../lib/api";

export const Route = createFileRoute("/jobs/")({
  component: JobsPage,
});

const mediaKinds: Array<{ value: MediaKind; label: string }> = [
  { value: "anime", label: "Anime" },
  { value: "book", label: "Book" },
  { value: "movie", label: "Movie" },
  { value: "music", label: "Music" },
  { value: "television", label: "Television" },
];

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

function JobsPage() {
  const queryClient = useQueryClient();
  const [stateFilter, setStateFilter] = createSignal<JobState | "">("");
  const [mediaKind, setMediaKind] = createSignal<MediaKind>("movie");
  const [mediaPath, setMediaPath] = createSignal("");
  const [apply, setApply] = createSignal(false);

  const jobs = useQuery(() => ({
    queryKey: ["jobs", stateFilter()],
    queryFn: () =>
      api.listJobs({
        limit: 50,
        ...(stateFilter() ? { state: stateFilter() as JobState } : {}),
      }),
  }));

  const createJob = useMutation(() => ({
    mutationFn: (request: CreateJobRequest) => api.createJob(request),
    onSuccess: () => {
      setMediaPath("");
      setApply(false);
      void queryClient.invalidateQueries({ queryKey: ["jobs"] });
    },
  }));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const inputPath = mediaPath().trim();
    if (!inputPath) return;
    createJob.mutate({
      media_kind: mediaKind(),
      input_path: inputPath,
      apply: apply(),
    });
  }

  return (
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        variant="detail"
        eyebrow="Jobs / Local queue"
        title="Scrape jobs"
        description="Scan media, compare metadata evidence, and stage output without silent writes."
        aside={
          <FormField label="State filter">
            <select
              value={stateFilter()}
              onChange={(event) =>
                setStateFilter(event.currentTarget.value as JobState | "")
              }
            >
              <For each={states}>
                {(state) => <option value={state.value}>{state.label}</option>}
              </For>
            </select>
          </FormField>
        }
      />

      <section
        class="my-12 grid grid-cols-[minmax(180px,0.45fr)_minmax(0,1.55fr)] gap-[clamp(2rem,5vw,5rem)] border-y border-line border-t-2 border-t-ink py-8 max-[900px]:grid-cols-1 max-[900px]:gap-6"
        aria-labelledby="create-job-title"
      >
        <div>
          <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            New scan
          </p>
          <h2 class="m-0 font-serif text-3xl font-medium" id="create-job-title">
            Create a bounded job
          </h2>
        </div>
        <form
          class="grid grid-cols-[minmax(140px,0.45fr)_minmax(260px,1.4fr)] gap-4 max-[640px]:grid-cols-1"
          onSubmit={submit}
        >
          <FormField label="Media kind">
            <select
              value={mediaKind()}
              onChange={(event) =>
                setMediaKind(event.currentTarget.value as MediaKind)
              }
            >
              <For each={mediaKinds}>
                {(kind) => <option value={kind.value}>{kind.label}</option>}
              </For>
            </select>
          </FormField>
          <FormField
            class="col-start-2 row-start-1 max-[640px]:col-start-1 max-[640px]:row-auto"
            label="Media path"
          >
            <input
              type="text"
              value={mediaPath()}
              placeholder="/media/Title.mkv"
              required
              onInput={(event) => setMediaPath(event.currentTarget.value)}
            />
          </FormField>
          <label class="col-span-full flex items-start gap-3 pt-2 text-sm text-muted max-[640px]:col-span-1">
            <input
              class="mt-[0.15rem] size-[1.05rem] shrink-0 accent-moss"
              type="checkbox"
              aria-label="Allow approved writes"
              checked={apply()}
              onChange={(event) => setApply(event.currentTarget.checked)}
            />
            <span>
              <strong class="block text-ink">Allow approved writes</strong>
              <small class="mt-1 block text-muted">
                A plan still requires review.
              </small>
            </span>
          </label>
          <Button
            class="col-start-2 justify-self-end max-[640px]:col-start-1 max-[640px]:w-full"
            type="submit"
            disabled={createJob.isPending || !mediaPath().trim()}
          >
            {createJob.isPending ? "Creating…" : "Create job"}
          </Button>
        </form>
        <Show when={createJob.isError}>
          <div class="col-start-2 max-[900px]:col-start-1">
            <RequestError error={createJob.error} />
          </div>
        </Show>
      </section>

      <section
        class="border-t border-line pt-8"
        aria-labelledby="job-list-title"
      >
        <SectionHeader
          eyebrow="Queue"
          title="Recent jobs"
          titleId="job-list-title"
          meta={<CountBadge>{jobs.data?.jobs.length ?? 0} shown</CountBadge>}
        />
        <Show when={jobs.isPending}>
          <LoadingState>Loading jobs…</LoadingState>
        </Show>
        <Show when={jobs.isError}>
          <RequestError error={jobs.error} />
        </Show>
        <Show when={jobs.isSuccess && jobs.data?.jobs.length === 0}>
          <EmptyState
            title="No matching jobs"
            description="Change the filter or create a new scan."
          />
        </Show>
        <div class="mt-8 border-t-2 border-ink">
          <For each={jobs.data?.jobs ?? []}>
            {(job) => (
              <article class="grid min-h-[98px] grid-cols-[70px_minmax(0,1fr)_auto_90px] items-center gap-5 border-b border-line max-[640px]:grid-cols-[52px_minmax(0,1fr)] max-[640px]:gap-3 max-[640px]:py-4">
                <div class="font-serif text-lg font-medium text-muted">
                  #{job.id}
                </div>
                <div class="min-w-0">
                  <p class="m-0 overflow-hidden text-ellipsis whitespace-nowrap font-serif text-base font-medium">
                    {job.input.input_path}
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
