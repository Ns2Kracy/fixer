import {
  API_BASE,
  API_SCHEMA_VERSION,
  type ExecutionSummary,
  type JobState,
  type ProgressSummary,
} from "./api";

export type JobEvent =
  | { type: "state"; cursor: string; job_id: number; state: JobState }
  | {
      type: "progress";
      cursor: string;
      job_id: number;
      progress: ProgressSummary;
    }
  | {
      type: "review";
      cursor: string;
      job_id: number;
      candidate_count: number;
      conflict_count: number;
    }
  | {
      type: "completion";
      cursor: string;
      job_id: number;
      execution: ExecutionSummary;
    };

export type JobEventConnectionState =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed";
export type JobMessageListener = (event: MessageEvent<string>) => void;

export interface EventSourceLike {
  onopen: ((event: Event) => void) | null;
  onerror: ((event: Event) => void) | null;
  addEventListener(type: string, listener: JobMessageListener): void;
  removeEventListener(type: string, listener: JobMessageListener): void;
  close(): void;
}

export interface EventSourceConstructor {
  new (
    url: string | URL,
    eventSourceInitDict?: EventSourceInit,
  ): EventSourceLike;
}

export interface ConnectJobEventsOptions {
  baseUrl?: string;
  eventSource?: EventSourceConstructor;
  onEvent: (event: JobEvent) => void;
  reconcile?: () => void | Promise<void>;
  onConnectionChange?: (state: JobEventConnectionState) => void;
  onReconcileError?: (error: unknown) => void;
}

export interface JobEventConnection {
  close(): void;
}

const eventTypes = ["state", "progress", "review", "completion"] as const;
const jobStates = new Set<JobState>([
  "queued",
  "scanning",
  "searching",
  "resolving",
  "awaiting_confirmation",
  "planning",
  "writing",
  "completed",
  "failed",
  "cancelled",
  "interrupted",
]);

export function connectJobEvents(
  jobId: number,
  options: ConnectJobEventsOptions,
): JobEventConnection {
  // SAFETY: native EventSource implements this minimal surface; the cast only narrows
  // its overloaded listener signatures so the same constructor can be injected in tests.
  const EventSourceClass =
    options.eventSource ?? (EventSource as unknown as EventSourceConstructor);
  const source = new EventSourceClass(
    `${options.baseUrl ?? API_BASE}/jobs/${jobId}/events`,
    {
      withCredentials: true,
    },
  );
  let closed = false;
  let reconciling = false;
  let reconcilePending = false;
  const reconcile = options.reconcile;

  options.onConnectionChange?.("connecting");

  const requestReconcile = () => {
    if (!reconcile || closed) return;
    if (reconciling) {
      reconcilePending = true;
      return;
    }
    reconciling = true;
    Promise.resolve(reconcile())
      .catch((error: unknown) => options.onReconcileError?.(error))
      .finally(() => {
        reconciling = false;
        if (reconcilePending) {
          reconcilePending = false;
          requestReconcile();
        }
      });
  };

  source.onopen = () => {
    options.onConnectionChange?.("connected");
    requestReconcile();
  };
  source.onerror = () => {
    if (closed) return;
    options.onConnectionChange?.("reconnecting");
    requestReconcile();
  };

  const listeners = new Map<string, JobMessageListener>();
  for (const type of eventTypes) {
    const listener: JobMessageListener = (message) => {
      const event = parseJobEvent(type, message, jobId);
      if (!event) return;
      options.onEvent(event);
      requestReconcile();
    };
    listeners.set(type, listener);
    source.addEventListener(type, listener);
  }

  return {
    close() {
      if (closed) return;
      closed = true;
      for (const [type, listener] of listeners)
        source.removeEventListener(type, listener);
      source.close();
      options.onConnectionChange?.("closed");
    },
  };
}

function parseJobEvent(
  type: (typeof eventTypes)[number],
  message: MessageEvent<string>,
  expectedJobId: number,
): JobEvent | undefined {
  let value: unknown;
  try {
    value = JSON.parse(message.data);
  } catch {
    return undefined;
  }
  if (
    !isRecord(value) ||
    value.schema_version !== API_SCHEMA_VERSION ||
    value.job_id !== expectedJobId
  ) {
    return undefined;
  }
  const base = { cursor: message.lastEventId, job_id: expectedJobId };
  switch (type) {
    case "state":
      return typeof value.state === "string" &&
        jobStates.has(value.state as JobState)
        ? { ...base, type, state: value.state as JobState }
        : undefined;
    case "progress":
      return isProgress(value.progress)
        ? { ...base, type, progress: value.progress }
        : undefined;
    case "review":
      return isCount(value.candidate_count) && isCount(value.conflict_count)
        ? {
            ...base,
            type,
            candidate_count: value.candidate_count,
            conflict_count: value.conflict_count,
          }
        : undefined;
    case "completion":
      return isExecution(value.execution)
        ? { ...base, type, execution: value.execution }
        : undefined;
    default:
      return undefined;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isCount(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isProgress(value: unknown): value is ProgressSummary {
  return (
    isRecord(value) &&
    value.schema_version === API_SCHEMA_VERSION &&
    typeof value.stage === "string" &&
    isCount(value.completed_items) &&
    (value.total_items === null || isCount(value.total_items))
  );
}

function isExecution(value: unknown): value is ExecutionSummary {
  return (
    isRecord(value) &&
    value.schema_version === API_SCHEMA_VERSION &&
    isCount(value.completed_operations) &&
    isCount(value.failed_operations)
  );
}
