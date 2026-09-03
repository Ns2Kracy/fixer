import { useQuery } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";
import { For, Show, createEffect, createSignal } from "solid-js";

import { RequestError } from "../components/request-error";
import { CountBadge } from "../components/ui/count-badge";
import { EmptyState } from "../components/ui/empty-state";
import { FormField } from "../components/ui/form-field";
import { LoadingState } from "../components/ui/loading-state";
import { Notice } from "../components/ui/notice";
import { PageHeader } from "../components/ui/page-header";
import { SectionHeader } from "../components/ui/section-header";
import { api } from "../lib/api";

export const Route = createFileRoute("/library")({
  component: LibraryPage,
});

const entryKindClasses = {
  directory: "border-moss text-moss",
  file: "border-line text-muted",
} as const;

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
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        eyebrow="Library / Safe browser"
        title="Browse configured roots"
        description="Navigate only the folders the server issued. Absolute paths and parent traversal never enter this interface."
      />

      <section class="mt-12" aria-labelledby="library-browser-title">
        <div class="grid grid-cols-[minmax(180px,0.35fr)_minmax(0,1.65fr)] items-end gap-8 border-y border-line border-t-2 border-t-ink py-6 max-[700px]:grid-cols-1">
          <FormField label="Library root">
            <select
              value={rootId()}
              disabled={
                roots.isPending || (roots.data?.roots.length ?? 0) === 0
              }
              onChange={(event) => {
                chooseRoot(event.currentTarget.value);
              }}
            >
              <For each={roots.data?.roots ?? []}>
                {(root) => <option value={root.id}>{root.label}</option>}
              </For>
            </select>
          </FormField>
          <nav
            class="flex min-h-[46px] items-center gap-2 overflow-x-auto"
            aria-label="Current library path"
          >
            <button
              type="button"
              class={`cursor-pointer border-0 bg-transparent px-1 py-1.5 whitespace-nowrap ${path() ? "text-muted" : "font-bold text-ink"}`}
              aria-current={path() ? undefined : "location"}
              onClick={() => setPath("")}
            >
              Root
            </button>
            <For each={segments()}>
              {(segment, index) => (
                <>
                  <span class="text-line" aria-hidden="true">
                    /
                  </span>
                  <button
                    type="button"
                    class={`cursor-pointer border-0 bg-transparent px-1 py-1.5 whitespace-nowrap ${index() === segments().length - 1 ? "font-bold text-ink" : "text-muted"}`}
                    aria-current={
                      index() === segments().length - 1 ? "location" : undefined
                    }
                    onClick={() => {
                      segmentPath(index());
                    }}
                  >
                    {segment}
                  </button>
                </>
              )}
            </For>
          </nav>
        </div>

        <SectionHeader
          class="mt-12"
          eyebrow={<>Opaque root / {rootId() || "none"}</>}
          title={path() || "Root contents"}
          titleId="library-browser-title"
          meta={
            <Show when={listing.isSuccess}>
              <CountBadge>
                {listing.data?.entries.length ?? 0} entries
              </CountBadge>
            </Show>
          }
        />

        <Show when={roots.isPending || listing.isPending}>
          <LoadingState>Reading configured root…</LoadingState>
        </Show>
        <Show when={roots.isError}>
          <RequestError error={roots.error} />
        </Show>
        <Show when={listing.isError}>
          <RequestError error={listing.error} />
        </Show>
        <Show when={roots.isSuccess && roots.data?.roots.length === 0}>
          <EmptyState
            title="No roots configured"
            description="Add media roots to the server configuration before browsing."
          />
        </Show>
        <Show when={listing.isSuccess && listing.data?.entries.length === 0}>
          <EmptyState
            title="This folder is empty"
            description="No browsable files or directories were returned."
          />
        </Show>

        <div class="mt-8 border-t-2 border-ink">
          <For each={listing.data?.entries ?? []}>
            {(entry, index) => (
              <article class="grid min-h-[84px] grid-cols-[48px_58px_minmax(0,1fr)_auto] items-center gap-4 border-b border-line max-[700px]:grid-cols-[38px_52px_minmax(0,1fr)] max-[700px]:py-4">
                <span class="font-serif text-sm font-medium text-muted">
                  {String(index() + 1).padStart(2, "0")}
                </span>
                <span
                  class={`w-max border px-1.5 py-1 text-[0.58rem] font-extrabold tracking-[0.08em] ${entryKindClasses[entry.kind]}`}
                  aria-hidden="true"
                >
                  {entry.kind === "directory" ? "DIR" : "FILE"}
                </span>
                <div>
                  <h3 class="m-0 font-serif text-base font-medium">
                    {entry.name}
                  </h3>
                  <p class="mt-1 mb-0 wrap-anywhere text-xs text-muted">
                    {entry.path}
                  </p>
                </div>
                <Show
                  when={entry.kind === "directory"}
                  fallback={
                    <small class="text-xs text-muted max-[700px]:col-start-3 max-[700px]:justify-self-start">
                      {formatBytes(entry.size_bytes)}
                    </small>
                  }
                >
                  <button
                    class="cursor-pointer border-0 bg-transparent py-2 text-xs font-bold hover:text-moss max-[700px]:col-start-3 max-[700px]:justify-self-start"
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
          <Notice class="my-4" tone="danger">
            This directory was capped at the server browsing limit.
          </Notice>
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
