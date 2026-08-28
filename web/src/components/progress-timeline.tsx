import { For, Show } from "solid-js";

import type { JobState, ProgressSummary } from "../lib/api";
import { jobStateLabel } from "./job-status";

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
    <div class="progress-timeline" aria-label="Job progress">
      <ol>
        <For each={stages}>
          {([state, label], index) => (
            <li
              data-stage={state}
              class={
                index() < currentIndex()
                  ? "complete"
                  : index() === currentIndex() &&
                      !terminalStates.has(props.state)
                    ? "current"
                    : ""
              }
            >
              <span class="timeline-marker" aria-hidden="true" />
              <span>{label}</span>
            </li>
          )}
        </For>
      </ol>
      <Show when={terminalStates.has(props.state)}>
        <p class="terminal-note" role="status">
          <strong>{jobStateLabel(props.state)}.</strong>{" "}
          {props.state === "interrupted"
            ? "No writes resume automatically; retry starts the scan again."
            : props.state === "cancelled"
              ? "Processing stopped before another stage could begin."
              : "The job stopped without claiming successful completion."}
        </p>
      </Show>
    </div>
  );
}
