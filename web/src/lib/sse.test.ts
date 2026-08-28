import { describe, expect, it, vi } from "vitest";

import { connectJobEvents, type EventSourceLike } from "./sse";

class MockEventSource implements EventSourceLike {
  static instances: MockEventSource[] = [];
  readonly listeners = new Map<string, Set<(event: MessageEvent) => void>>();
  readonly url: string;
  readonly withCredentials: boolean;
  closed = false;
  onopen: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  constructor(url: string | URL, options?: EventSourceInit) {
    this.url = String(url);
    this.withCredentials = options?.withCredentials ?? false;
    MockEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: (event: MessageEvent) => void) {
    const listeners = this.listeners.get(type) ?? new Set();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: (event: MessageEvent) => void) {
    this.listeners.get(type)?.delete(listener);
  }

  emit(type: string, data: unknown, lastEventId = "runtime:1") {
    const event = new MessageEvent(type, {
      data: JSON.stringify(data),
      lastEventId,
    });
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }

  close() {
    this.closed = true;
  }
}

describe("connectJobEvents", () => {
  it("uses native reconnect semantics and reconciles typed events with fetched state", async () => {
    MockEventSource.instances = [];
    const onEvent = vi.fn();
    const reconcile = vi.fn().mockResolvedValue(undefined);
    const onConnectionChange = vi.fn();

    const connection = connectJobEvents(7, {
      eventSource: MockEventSource,
      onEvent,
      reconcile,
      onConnectionChange,
    });
    const source = MockEventSource.instances[0]!;

    expect(source.url).toBe("/api/v1/jobs/7/events");
    expect(source.withCredentials).toBe(true);
    source.onopen?.(new Event("open"));
    await vi.waitFor(() => expect(reconcile).toHaveBeenCalledTimes(1));
    source.emit("state", { schema_version: 1, job_id: 7, state: "searching" });
    await vi.waitFor(() => expect(reconcile).toHaveBeenCalledTimes(2));

    expect(onConnectionChange).toHaveBeenCalledWith("connected");
    expect(onEvent).toHaveBeenCalledWith(
      expect.objectContaining({
        type: "state",
        state: "searching",
        cursor: "runtime:1",
      }),
    );

    source.onerror?.(new Event("error"));
    expect(onConnectionChange).toHaveBeenLastCalledWith("reconnecting");
    await vi.waitFor(() => expect(reconcile).toHaveBeenCalledTimes(3));
    connection.close();
    expect(source.closed).toBe(true);
  });

  it("ignores malformed and cross-job events", () => {
    MockEventSource.instances = [];
    const onEvent = vi.fn();
    const reconcile = vi.fn();
    connectJobEvents(7, { eventSource: MockEventSource, onEvent, reconcile });
    const source = MockEventSource.instances[0]!;

    source.emit("state", { schema_version: 1, job_id: 8, state: "queued" });
    source.emit("progress", { nope: true });

    expect(onEvent).not.toHaveBeenCalled();
    expect(reconcile).not.toHaveBeenCalled();
  });
});
