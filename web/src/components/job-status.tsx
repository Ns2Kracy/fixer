import type { JobState } from "../lib/api";

const labels: Record<JobState, string> = {
  queued: "Queued",
  scanning: "Scanning",
  searching: "Searching",
  resolving: "Resolving",
  awaiting_confirmation: "Awaiting review",
  planning: "Plan ready",
  writing: "Writing",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
  interrupted: "Interrupted",
};

const statusClasses: Record<JobState, string> = {
  queued: "text-muted",
  scanning: "text-muted",
  searching: "text-muted",
  resolving: "text-muted",
  awaiting_confirmation: "text-moss",
  planning: "text-moss",
  writing: "text-muted",
  completed: "text-success",
  failed: "text-danger",
  cancelled: "text-muted",
  interrupted: "text-danger",
};

const markerClasses: Record<JobState, string> = {
  queued: "border-current bg-transparent",
  scanning: "border-current bg-transparent",
  searching: "border-current bg-transparent",
  resolving: "border-current bg-transparent",
  awaiting_confirmation: "border-moss bg-moss",
  planning: "border-moss bg-moss",
  writing: "border-current bg-transparent",
  completed: "border-success bg-success",
  failed: "border-coral bg-coral",
  cancelled: "border-current bg-transparent",
  interrupted: "border-coral bg-coral",
};

export function jobStateLabel(state: JobState) {
  return labels[state];
}

export function JobStatus(props: { state: JobState }) {
  return (
    <span
      class={`inline-flex w-max items-center gap-2 text-[0.68rem] font-bold uppercase tracking-[0.07em] ${statusClasses[props.state]}`}
    >
      <span
        class={`size-2 rounded-full border ${markerClasses[props.state]}`}
        aria-hidden="true"
      />
      {jobStateLabel(props.state)}
    </span>
  );
}
