import { For, Show, createSignal } from "solid-js";
import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute, useNavigate } from "@tanstack/solid-router";

import { CandidatePicker } from "../../../components/candidate-picker";
import { FieldConflict } from "../../../components/field-conflict";
import { RequestError } from "../../../components/request-error";
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
    <div class="review-page">
      <Link
        class="back-link"
        to="/jobs/$jobId"
        params={{ jobId: params().jobId }}
      >
        ← Job #{params().jobId}
      </Link>
      <header class="page-heading">
        <div>
          <p class="eyebrow">Job / #{params().jobId} / Evidence</p>
          <h1>Review metadata</h1>
          <p>
            Choose a candidate from scored evidence, then acknowledge every
            sourced conflict.
          </p>
        </div>
      </header>
      <Show when={review.isPending}>
        <p class="loading-line">Loading candidate evidence…</p>
      </Show>
      <Show when={review.isError}>
        <RequestError error={review.error} />
      </Show>
      <Show when={review.data}>
        {(data) => (
          <>
            <Show when={data().warnings.length > 0}>
              <section class="warning-panel" aria-labelledby="warnings-title">
                <div>
                  <p class="eyebrow">Partial result</p>
                  <h2 id="warnings-title">Provider warnings</h2>
                </div>
                <ul>
                  <For each={data().warnings}>
                    {(warning) => (
                      <li>
                        <strong>{warning.code}</strong>
                        <span>{warning.message}</span>
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
              <p class="truncation-note">
                Additional candidates were omitted by the server. Compare and
                select only from the bounded candidates shown.
              </p>
            </Show>
            <section class="conflicts-panel" aria-labelledby="conflicts-title">
              <div class="section-heading">
                <div>
                  <p class="eyebrow">Decision record</p>
                  <h2 id="conflicts-title">Sourced conflicts</h2>
                </div>
                <span class="count">{data().conflicts.length} open</span>
              </div>
              <Show when={data().conflicts.length === 0}>
                <p class="empty-inline">
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
                <p class="truncation-note">
                  The server omitted additional conflicts; execution remains
                  blocked until a complete review is available.
                </p>
              </Show>
            </section>
            <div class="approval-bar">
              <p>
                <strong>Candidate {selectedIndex() + 1}</strong>
                <span>
                  {acknowledged().size} of {data().conflicts.length} conflicts
                  acknowledged
                </span>
              </p>
              <button
                class="button primary"
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
              </button>
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
