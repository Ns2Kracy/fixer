import { For, Show } from "solid-js";

import type { JobState, ProgressSummary } from "../lib/api";
import { jobStateLabel } from "./job-status";
import { Notice } from "./ui/notice";

const stages = [
  ["queued", "Queued"],
  ["scanning", "Scan files"],
  ["searching", "Search providers"],
  ["resolving", "Resolve metadata"],
  ["awaiting_confirmation", "Review"],
  ["planning", "Plan output"],
  ["writing", "Write files"],
  ["completed", "Complete"],
] as const;

const terminalStates = new Set<JobState>([
  "failed",
  "cancelled",
  "interrupted",
]);

const stageClasses = {
  complete: "font-bold text-ink",
  current: "font-bold text-ink",
  upcoming: "text-muted",
} as const;

const markerClasses = {
  complete: "border-moss bg-moss",
  current: "border-[3px] border-coral bg-paper",
  upcoming: "border-muted bg-paper",
} as const;

export function ProgressTimeline(props: {
  state: JobState;
  progress: ProgressSummary | undefined;
}) {
  const currentIndex = () => {
    const index = stages.findIndex(([state]) => state === props.state);
    if (index >= 0) return index;
    const progressIndex = stages.findIndex(
      ([state]) => state === props.progress?.stage,
    );
    return progressIndex >= 0 ? progressIndex : 0;
  };

  return (
    <div class="mt-10" aria-label="Job progress">
      <ol class="m-0 grid list-none grid-cols-8 p-0 max-[900px]:grid-cols-4 max-[900px]:gap-y-6 max-[640px]:grid-cols-2">
        <For each={stages}>
          {([state, label], index) => {
            const stageState = () =>
              index() < currentIndex()
                ? "complete"
                : index() === currentIndex() &&
                    !terminalStates.has(props.state)
                  ? "current"
                  : "upcoming";

            return (
              <li
                data-stage={state}
                class={`relative grid gap-3 pr-3 text-[0.7rem] before:absolute before:top-[5px] before:right-0 before:left-3 before:h-px before:bg-line before:content-[''] ${stageClasses[stageState()]}`}
              >
                <span
                  class={`z-1 size-[11px] rounded-full border ${markerClasses[stageState()]}`}
                  aria-hidden="true"
                />
                <span>{label}</span>
              </li>
            );
          }}
        </For>
      </ol>
      <Show when={terminalStates.has(props.state)}>
        <Notice class="mt-8" tone="danger" role="status">
          <strong>{jobStateLabel(props.state)}.</strong>{" "}
          {props.state === "interrupted"
            ? "No writes resume automatically; retry starts the scan again."
            : props.state === "cancelled"
              ? "Processing stopped before another stage could begin."
              : "The job stopped without claiming successful completion."}
        </Notice>
      </Show>
    </div>
  );
}
