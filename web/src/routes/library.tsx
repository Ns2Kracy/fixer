import { For, Show, createEffect, createSignal } from "solid-js";
import { useQuery } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";

import { RequestError } from "../components/request-error";
import { api } from "../lib/api";

export const Route = createFileRoute("/library")({
  component: LibraryPage,
});

function LibraryPage() {
  const [rootId, setRootId] = createSignal("");
  const [path, setPath] = createSignal("");
  const roots = useQuery(() => ({
    queryKey: ["library-roots"],
    queryFn: () => api.libraryRoots(),
  }));
  const listing = useQuery(() => ({
    queryKey: ["library", rootId(), path()],
    queryFn: () => api.listLibrary({ rootId: rootId(), path: path() }),
    enabled: rootId().length > 0,
  }));

  createEffect(
    () => ({ available: roots.data?.roots ?? [], currentRootId: rootId() }),
    ({ available, currentRootId }) => {
      if (
        available.length > 0 &&
        !available.some((root) => root.id === currentRootId)
      ) {
        setRootId(available[0]!.id);
        setPath("");
      }
    },
  );

  const segments = () => path().split("/").filter(Boolean);

  function chooseRoot(id: string) {
    setRootId(id);
    setPath("");
  }

  function segmentPath(index: number) {
    setPath(
      segments()
        .slice(0, index + 1)
        .join("/"),
    );
  }

  return (
    <div class="workspace-page library-page">
      <header class="workspace-heading">
        <div>
          <p class="eyebrow">Library / Safe browser</p>
          <h1>Browse configured roots</h1>
        </div>
        <p>
          Navigate only the folders the server issued. Absolute paths and parent
          traversal never enter this interface.
        </p>
      </header>

      <section class="library-browser" aria-labelledby="library-browser-title">
        <div class="library-toolbar">
          <label>
            <span>Library root</span>
            <select
              value={rootId()}
              disabled={
                roots.isPending || (roots.data?.roots.length ?? 0) === 0
              }
              onChange={(event) => chooseRoot(event.currentTarget.value)}
            >
              <For each={roots.data?.roots ?? []}>
                {(root) => <option value={root.id}>{root.label}</option>}
              </For>
            </select>
          </label>
          <nav class="path-crumbs" aria-label="Current library path">
            <button
              type="button"
              class={path() ? "" : "current"}
              aria-current={path() ? undefined : "location"}
              onClick={() => setPath("")}
            >
              Root
            </button>
            <For each={segments()}>
              {(segment, index) => (
                <>
                  <span aria-hidden="true">/</span>
                  <button
                    type="button"
                    class={index() === segments().length - 1 ? "current" : ""}
                    aria-current={
                      index() === segments().length - 1
                        ? "location"
                        : undefined
                    }
                    onClick={() => segmentPath(index())}
                  >
                    {segment}
                  </button>
                </>
              )}
            </For>
          </nav>
        </div>

        <div class="section-heading library-section-heading">
          <div>
            <p class="eyebrow">Opaque root / {rootId() || "none"}</p>
            <h2 id="library-browser-title">{path() || "Root contents"}</h2>
          </div>
          <Show when={listing.isSuccess}>
            <span class="count">
              {listing.data?.entries.length ?? 0} entries
            </span>
          </Show>
        </div>

        <Show when={roots.isPending || listing.isPending}>
          <p class="loading-line">Reading configured root…</p>
        </Show>
        <Show when={roots.isError}>
          <RequestError error={roots.error} />
        </Show>
        <Show when={listing.isError}>
          <RequestError error={listing.error} />
        </Show>
        <Show when={roots.isSuccess && roots.data?.roots.length === 0}>
          <div class="empty-inline">
            <div>
              <h3>No roots configured</h3>
              <p>
                Add media roots to the server configuration before browsing.
              </p>
            </div>
          </div>
        </Show>
        <Show when={listing.isSuccess && listing.data?.entries.length === 0}>
          <div class="empty-inline">
            <div>
              <h3>This folder is empty</h3>
              <p>No browsable files or directories were returned.</p>
            </div>
          </div>
        </Show>

        <div class="library-list">
          <For each={listing.data?.entries ?? []}>
            {(entry, index) => (
              <article>
                <span class="library-entry-index">
                  {String(index() + 1).padStart(2, "0")}
                </span>
                <span class={`entry-kind ${entry.kind}`} aria-hidden="true">
                  {entry.kind === "directory" ? "DIR" : "FILE"}
                </span>
                <div>
                  <h3>{entry.name}</h3>
                  <p>{entry.path}</p>
                </div>
                <Show
                  when={entry.kind === "directory"}
                  fallback={<small>{formatBytes(entry.size_bytes)}</small>}
                >
                  <button
                    class="text-link"
                    type="button"
                    aria-label={`Open ${entry.name}`}
                    onClick={() => setPath(entry.path)}
                  >
                    Open <span aria-hidden="true">→</span>
                  </button>
                </Show>
              </article>
            )}
          </For>
        </div>
        <Show when={listing.data?.truncated}>
          <p class="truncation-note">
            This directory was capped at the server browsing limit.
          </p>
        </Show>
      </section>
    </div>
  );
}

function formatBytes(value?: number) {
  if (value === undefined) return "Size unavailable";
  if (value < 1024) return `${value} B`;
  return `${(value / 1024).toFixed(1)} KB`;
}
