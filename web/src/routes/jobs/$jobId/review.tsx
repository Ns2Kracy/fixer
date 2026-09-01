import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute, useNavigate } from "@tanstack/solid-router";
import { For, Show, createSignal } from "solid-js";

import { CandidatePicker } from "../../../components/candidate-picker";
import { FieldConflict } from "../../../components/field-conflict";
import { RequestError } from "../../../components/request-error";
import { Button } from "../../../components/ui/button";
import { CountBadge } from "../../../components/ui/count-badge";
import { LoadingState } from "../../../components/ui/loading-state";
import { PageHeader } from "../../../components/ui/page-header";
import { SectionHeader } from "../../../components/ui/section-header";
import { api, type ReviewJobRequest } from "../../../lib/api";

export const Route = createFileRoute("/jobs/$jobId/review")({
  component: ReviewPage,
});

function ReviewPage() {
  const params = Route.useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const jobId = () => Number(params().jobId);
  const [requestedIndex, setRequestedIndex] = createSignal<number>();
  const [acknowledged, setAcknowledged] = createSignal<Set<number>>(new Set());

  const review = useQuery(() => ({
    queryKey: ["job-review", jobId(), requestedIndex() ?? "persisted"],
    queryFn: () => api.getJobReview(jobId(), requestedIndex()),
  }));

  const accept = useMutation(() => ({
    mutationFn: (request: ReviewJobRequest) => api.reviewJob(jobId(), request),
    onSuccess: (result) => {
      queryClient.setQueryData(["job", jobId()], result);
      void navigate({
        to: "/jobs/$jobId/plan",
        params: { jobId: params().jobId },
      });
    },
  }));

  const selectedIndex = () =>
    review.data?.selected_candidate_index ?? requestedIndex() ?? 0;

  function selectCandidate(index: number) {
    setRequestedIndex(index);
    setAcknowledged(new Set<number>());
  }

  function toggleConflict(index: number, checked: boolean) {
    setAcknowledged((current) => {
      const next = new Set(current);
      if (checked) next.add(index);
      else next.delete(index);
      return next;
    });
  }

  const allConflictsAcknowledged = () =>
    (review.data?.conflicts ?? []).every((conflict) =>
      acknowledged().has(conflict.index),
    );

  return (
    <div class="mx-auto max-w-[1180px]">
      <Link
        class="mb-8 inline-block text-xs font-bold uppercase tracking-[0.06em] text-muted underline decoration-1 underline-offset-4 hover:text-moss"
        to="/jobs/$jobId"
        params={{ jobId: params().jobId }}
      >
        ← Job #{params().jobId}
      </Link>
      <PageHeader
        variant="detail"
        eyebrow={<>Job / #{params().jobId} / Evidence</>}
        title="Review metadata"
        description="Choose a candidate from scored evidence, then acknowledge every sourced conflict."
      />
      <Show when={review.isPending}>
        <LoadingState>Loading candidate evidence…</LoadingState>
      </Show>
      <Show when={review.isError}>
        <RequestError error={review.error} />
      </Show>
      <Show when={review.data}>
        {(data) => (
          <>
            <Show when={data().warnings.length > 0}>
              <section
                class="my-12 grid grid-cols-[minmax(180px,0.45fr)_minmax(0,1.55fr)] gap-8 border-l-4 border-coral bg-warning-surface p-6 max-[900px]:grid-cols-1"
                aria-labelledby="warnings-title"
              >
                <div>
                  <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
                    Partial result
                  </p>
                  <h2
                    class="m-0 font-serif text-2xl font-medium"
                    id="warnings-title"
                  >
                    Provider warnings
                  </h2>
                </div>
                <ul class="m-0 list-none p-0">
                  <For each={data().warnings}>
                    {(warning) => (
                      <li class="grid gap-1 border-b border-line py-2.5">
                        <strong class="text-[0.67rem] uppercase tracking-[0.08em]">
                          {warning.code}
                        </strong>
                        <span class="text-muted">{warning.message}</span>
                      </li>
                    )}
                  </For>
                </ul>
                <Show when={data().warnings_truncated}>
                  <p>Additional warnings were omitted by the server.</p>
                </Show>
              </section>
            </Show>
            <CandidatePicker
              candidates={data().candidates}
              selectedIndex={selectedIndex()}
              onSelect={selectCandidate}
            />
            <Show when={data().candidates_truncated}>
              <p class="my-4 border-l-[3px] border-coral bg-danger-surface px-4 py-3">
                Additional candidates were omitted by the server. Compare and
                select only from the bounded candidates shown.
              </p>
            </Show>
            <section class="mb-28" aria-labelledby="conflicts-title">
              <SectionHeader
                eyebrow="Decision record"
                title="Sourced conflicts"
                titleId="conflicts-title"
                meta={<CountBadge>{data().conflicts.length} open</CountBadge>}
              />
              <Show when={data().conflicts.length === 0}>
                <p class="mt-8 min-h-36 border border-dashed border-line p-8 text-sm text-muted">
                  No field conflicts require acknowledgement.
                </p>
              </Show>
              <For each={data().conflicts}>
                {(conflict) => (
                  <FieldConflict
                    conflict={conflict}
                    acknowledged={acknowledged().has(conflict.index)}
                    onToggle={(checked) =>
                      toggleConflict(conflict.index, checked)
                    }
                  />
                )}
              </For>
              <Show when={data().conflicts_truncated}>
                <p class="my-4 border-l-[3px] border-coral bg-danger-surface px-4 py-3">
                  The server omitted additional conflicts; execution remains
                  blocked until a complete review is available.
                </p>
              </Show>
            </section>
            <div class="sticky bottom-4 z-5 flex items-center justify-between gap-8 border border-ink bg-overlay px-5 py-4 shadow-[0_10px_30px_var(--color-shadow)] max-[640px]:static max-[640px]:flex-col max-[640px]:items-stretch">
              <p class="m-0">
                <strong>Candidate {selectedIndex() + 1}</strong>
                <span class="block text-xs text-muted">
                  {acknowledged().size} of {data().conflicts.length} conflicts
                  acknowledged
                </span>
              </p>
              <Button
                type="button"
                disabled={
                  accept.isPending ||
                  !allConflictsAcknowledged() ||
                  data().conflicts_truncated
                }
                onClick={() =>
                  accept.mutate({
                    candidate_index: selectedIndex(),
                    accepted_conflict_indexes: [...acknowledged()].sort(
                      (a, b) => a - b,
                    ),
                  })
                }
              >
                {accept.isPending
                  ? "Building plan…"
                  : "Accept candidate and build plan"}
              </Button>
            </div>
            <Show when={accept.isError}>
              <RequestError error={accept.error} />
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}
