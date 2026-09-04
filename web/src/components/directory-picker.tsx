import type { JSX } from "@solidjs/web";
import { For, Show, createMemo, createSignal, createUniqueId } from "solid-js";

import {
  api as defaultApi,
  type ApiClient,
  type DirectoryRef,
  type LibraryEntry,
  type RootSummary,
} from "../lib/api";
import { Button } from "./ui/button";
import { EmptyState } from "./ui/empty-state";
import { LoadingState } from "./ui/loading-state";

export interface DirectoryPickerProps {
  label: string;
  buttonLabel: string;
  value?: DirectoryRef | null;
  onSelect: (directory: DirectoryRef) => void;
  api?: Pick<ApiClient, "libraryRoots" | "listLibrary">;
}

export function DirectoryPicker(props: DirectoryPickerProps): JSX.Element {
  const client = () => props.api ?? defaultApi;
  const [open, setOpen] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<Error | null>(null);
  const [roots, setRoots] = createSignal<RootSummary[]>([]);
  const [root, setRoot] = createSignal<RootSummary | null>(null);
  const [path, setPath] = createSignal("");
  const [entries, setEntries] = createSignal<LibraryEntry[]>([]);
  const [trigger, setTrigger] = createSignal<HTMLButtonElement>();
  const [dialog, setDialog] = createSignal<HTMLElement>();
  const titleId = createUniqueId();
  let requestVersion = 0;

  const directories = createMemo(() =>
    entries().filter((entry) => entry.kind === "directory"),
  );
  const segments = createMemo(() =>
    path()
      .split("/")
      .filter((segment) => segment !== "")
      .map((name, index, values) => ({
        name,
        path: values.slice(0, index + 1).join("/"),
      })),
  );

  async function openPicker() {
    setOpen(true);
    setRoot(null);
    setPath("");
    setEntries([]);
    queueMicrotask(() => dialog()?.focus());
    await loadRoots();
  }

  function closePicker() {
    requestVersion += 1;
    setOpen(false);
    queueMicrotask(() => trigger()?.focus());
  }

  async function loadRoots() {
    const request = ++requestVersion;
    setLoading(true);
    setError(null);
    try {
      const response = await client().libraryRoots();
      if (request !== requestVersion) return;
      setRoots(response.roots);
    } catch (cause) {
      if (request !== requestVersion) return;
      setError(asError(cause));
    } finally {
      if (request === requestVersion) setLoading(false);
    }
  }

  async function loadDirectory(
    selectedRoot: RootSummary,
    selectedPath: string,
  ) {
    const request = ++requestVersion;
    setRoot(selectedRoot);
    setLoading(true);
    setError(null);
    try {
      const response = await client().listLibrary({
        rootId: selectedRoot.id,
        path: selectedPath,
      });
      if (request !== requestVersion) return;
      setPath(response.path);
      setEntries(response.entries);
    } catch (cause) {
      if (request !== requestVersion) return;
      setError(asError(cause));
    } finally {
      if (request === requestVersion) setLoading(false);
    }
  }

  async function goBack() {
    const selectedRoot = root();
    if (!selectedRoot) return;
    const values = segments();
    if (values.length === 0) {
      setRoot(null);
      setEntries([]);
      return;
    }
    await loadDirectory(
      selectedRoot,
      values
        .slice(0, -1)
        .map((segment) => segment.name)
        .join("/"),
    );
  }

  function selectCurrent() {
    const selectedRoot = root();
    if (!selectedRoot) return;
    props.onSelect({ root_id: selectedRoot.id, path: path() });
    closePicker();
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    closePicker();
  }

  return (
    <div class="grid gap-2">
      <span class="text-xs font-bold uppercase tracking-[0.12em] text-muted">
        {props.label}
      </span>
      <div class="flex flex-wrap items-center gap-3">
        <Button
          ref={setTrigger}
          type="button"
          variant="secondary"
          aria-haspopup="dialog"
          aria-expanded={open() ? "true" : "false"}
          onClick={() => void openPicker()}
        >
          {props.buttonLabel}
        </Button>
        <Show when={props.value}>
          {(value) => (
            <span class="font-mono text-xs text-muted">
              {value().path || "Root folder"}
            </span>
          )}
        </Show>
      </div>

      <Show when={open()}>
        <div
          class="fixed inset-0 z-50 grid place-items-center bg-ink/45 p-4"
          role="presentation"
        >
          <section
            role="dialog"
            aria-modal="true"
            ref={(element) => {
              setDialog(element);
              element.addEventListener("keydown", handleKeyDown);
            }}
            tabindex={-1}
            aria-labelledby={titleId}
            class="grid max-h-[min(720px,90vh)] w-full max-w-2xl grid-rows-[auto_auto_minmax(0,1fr)_auto] border-2 border-ink bg-paper text-ink shadow-[12px_12px_0_var(--color-coral)]"
          >
            <header class="flex items-center justify-between border-b border-line px-5 py-4">
              <h2 id={titleId} class="m-0 font-serif text-2xl font-medium">
                Choose {props.label.toLowerCase()}
              </h2>
              <button
                type="button"
                class="min-h-11 min-w-11 text-2xl hover:text-coral"
                aria-label="Close directory picker"
                onClick={closePicker}
              >
                ×
              </button>
            </header>

            <Show when={root()}>
              {(selectedRoot) => (
                <nav
                  class="flex min-h-12 items-center gap-2 overflow-x-auto border-b border-line px-5 text-sm"
                  aria-label="Folder path"
                >
                  <button
                    type="button"
                    class="font-bold hover:text-moss"
                    onClick={() => void loadDirectory(selectedRoot(), "")}
                  >
                    {selectedRoot().label}
                  </button>
                  <For each={segments()}>
                    {(segment) => (
                      <>
                        <span aria-hidden="true">/</span>
                        <button
                          type="button"
                          class="whitespace-nowrap hover:text-moss"
                          onClick={() =>
                            void loadDirectory(selectedRoot(), segment.path)
                          }
                        >
                          {segment.name}
                        </button>
                      </>
                    )}
                  </For>
                </nav>
              )}
            </Show>

            <div class="min-h-56 overflow-y-auto p-5">
              <Show when={loading()}>
                <LoadingState class="my-4">Loading folders...</LoadingState>
              </Show>
              <Show when={!loading() && error()}>
                {(currentError) => (
                  <div role="alert" class="border border-coral p-4">
                    <strong>Folders could not be loaded.</strong>
                    <p class="mt-1 text-sm text-muted">
                      {currentError().message}
                    </p>
                    <Button
                      class="mt-4"
                      type="button"
                      variant="secondary"
                      onClick={() =>
                        void (root()
                          ? loadDirectory(root()!, path())
                          : loadRoots())
                      }
                    >
                      Retry
                    </Button>
                  </div>
                )}
              </Show>
              <Show when={!loading() && !error() && !root()}>
                <Show
                  when={roots().length > 0}
                  fallback={<EmptyState title="No folders available" />}
                >
                  <ul class="m-0 grid list-none gap-2 p-0">
                    <For each={roots()}>
                      {(item) => (
                        <li>
                          <button
                            type="button"
                            class="flex min-h-12 w-full items-center justify-between border border-line px-4 text-left font-medium hover:border-moss hover:bg-surface-muted"
                            onClick={() => void loadDirectory(item, "")}
                          >
                            <span>{item.label}</span>
                            <span aria-hidden="true">→</span>
                          </button>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </Show>
              <Show when={!loading() && !error() && root()}>
                <Show
                  when={directories().length > 0}
                  fallback={<EmptyState title="This folder is empty" />}
                >
                  <ul class="m-0 grid list-none gap-2 p-0">
                    <For each={directories()}>
                      {(entry) => (
                        <li>
                          <button
                            type="button"
                            class="flex min-h-12 w-full items-center justify-between border-b border-line px-3 text-left hover:bg-surface-muted"
                            onClick={() =>
                              void loadDirectory(root()!, entry.path)
                            }
                          >
                            <span>{entry.name}</span>
                            <span aria-hidden="true">/</span>
                          </button>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </Show>
            </div>

            <footer class="flex items-center justify-between gap-3 border-t border-line px-5 py-4">
              <Button
                type="button"
                variant="secondary"
                disabled={!root()}
                onClick={() => void goBack()}
              >
                Back
              </Button>
              <Button
                type="button"
                disabled={!root() || loading() || Boolean(error())}
                onClick={selectCurrent}
              >
                Select this folder
              </Button>
            </footer>
          </section>
        </div>
      </Show>
    </div>
  );
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error("Unknown request error");
}
