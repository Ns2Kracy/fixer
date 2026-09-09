import { For, Show } from "solid-js";

import type { CandidateArtifact } from "../lib/api";

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
              class={`grid cursor-pointer grid-cols-[24px_minmax(0,1fr)_72px] gap-4 border-b border-line px-4 py-6 transition-colors hover:bg-surface-muted max-[640px]:grid-cols-[24px_minmax(0,1fr)] ${selected() ? "bg-surface-muted" : "bg-transparent"}`}
            >
              <input
                class="mt-[0.15rem] size-[1.05rem] accent-moss"
                type="radio"
                name="candidate"
                value={candidate.index}
                checked={selected()}
                aria-label={`Select ${candidate.title} from ${candidate.provider}`}
                onChange={() => {
                  props.onSelect(candidate.index);
                }}
              />
              <span class="min-w-0">
                <span class="flex items-baseline gap-3">
                  <strong class="font-serif text-[1.35rem] font-medium">
                    {candidate.title}
                  </strong>
                  <Show when={candidate.year}>
                    {(year) => <span class="text-xs text-muted">{year()}</span>}
                  </Show>
                </span>
                <span class="mt-1 block text-[0.7rem] text-muted">
                  {candidate.provider} · {candidate.external_id.namespace}:
                  {candidate.external_id.value}
                </span>
                <Show when={candidate.sequence}>
                  {(sequence) => (
                    <span class="mt-3 block text-xs text-muted">
                      Sequence {sequence()}
                    </span>
                  )}
                </Show>
              </span>
              <span
                class="justify-self-end font-mono text-xs uppercase tracking-[0.16em] text-muted max-[640px]:col-start-2 max-[640px]:row-start-1"
                aria-label={`Candidate rank ${candidate.index + 1}`}
              >
                #{String(candidate.index + 1).padStart(2, "0")}
              </span>
            </label>
          );
        }}
      </For>
    </fieldset>
  );
}
