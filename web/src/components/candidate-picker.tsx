import { For, Show } from "solid-js";

import type { CandidateArtifact } from "../lib/api";

const evidencePointClasses = {
  positive: "text-success",
  negative: "text-danger",
} as const;

export function CandidatePicker(props: {
  candidates: CandidateArtifact[];
  selectedIndex: number;
  onSelect: (index: number) => void;
}) {
  return (
    <fieldset class="my-12 border-0 p-0 pb-8">
      <legend class="w-full border-b-2 border-ink pb-4 font-serif text-3xl font-medium">
        Candidate matches
      </legend>
      <For each={props.candidates}>
        {(candidate) => {
          const selected = () => candidate.index === props.selectedIndex;

          return (
            <label
              class={`grid cursor-pointer grid-cols-[24px_minmax(0,1fr)_80px] gap-4 border-b border-line px-4 py-6 transition-colors hover:bg-surface-muted max-[640px]:grid-cols-[24px_minmax(0,1fr)] ${selected() ? "bg-surface-muted" : "bg-transparent"}`}
            >
              <input
                class="mt-[0.15rem] size-[1.05rem] accent-moss"
                type="radio"
                name="candidate"
                value={candidate.index}
                checked={selected()}
                aria-label={`Select ${candidate.title} from ${candidate.provider}`}
                onChange={() => props.onSelect(candidate.index)}
              />
              <span class="min-w-0">
                <span class="flex items-baseline gap-3">
                  <strong class="font-serif text-[1.35rem] font-medium">
                    {candidate.title}
                  </strong>
                  <Show when={candidate.year}>
                    {(year) => (
                      <span class="text-xs text-muted">{year()}</span>
                    )}
                  </Show>
                </span>
                <span class="mt-1 block text-[0.7rem] text-muted">
                  {candidate.provider} · {candidate.external_id.namespace}:
                  {candidate.external_id.value}
                </span>
                <ul
                  class="mt-4 flex list-none flex-wrap gap-x-4 gap-y-2 p-0 text-xs text-muted"
                  aria-label={`Evidence for ${candidate.title}`}
                >
                  <For each={candidate.evidence}>
                    {(evidence) => (
                      <li>
                        <span
                          class={`mr-1 inline-block min-w-8 font-extrabold ${evidence.points >= 0 ? evidencePointClasses.positive : evidencePointClasses.negative}`}
                        >
                          {evidence.points >= 0 ? "+" : ""}
                          {evidence.points}
                        </span>
                        {evidence.detail}
                      </li>
                    )}
                  </For>
                </ul>
                <Show when={candidate.evidence_truncated}>
                  <span class="mt-2 block text-xs text-danger">
                    Additional matching evidence was omitted.
                  </span>
                </Show>
              </span>
              <span
                class="justify-self-end font-serif text-3xl font-medium max-[640px]:col-start-2 max-[640px]:row-start-1"
                aria-label={`Score ${candidate.score}`}
              >
                {candidate.score}
              </span>
            </label>
          );
        }}
      </For>
    </fieldset>
  );
}
