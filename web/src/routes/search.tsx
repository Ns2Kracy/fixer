import { useMutation } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";
import { For, Show, createSignal } from "solid-js";

import { RequestError } from "../components/request-error";
import { Button } from "../components/ui/button";
import { CountBadge } from "../components/ui/count-badge";
import { EmptyState } from "../components/ui/empty-state";
import { FormField } from "../components/ui/form-field";
import { PageHeader } from "../components/ui/page-header";
import { SectionHeader } from "../components/ui/section-header";
import { api, type MediaKind, type SearchRequest } from "../lib/api";

export const Route = createFileRoute("/search")({
  component: SearchPage,
});

const mediaKinds: Array<{ value: MediaKind; label: string; note: string }> = [
  { value: "movie", label: "Movie", note: "Features and film editions" },
  {
    value: "television",
    label: "Television",
    note: "Series, seasons, and episodes",
  },
  { value: "anime", label: "Anime", note: "Series and cours" },
  { value: "music", label: "Music", note: "Artists, albums, and tracks" },
  { value: "book", label: "Book", note: "Books and editions" },
];

function SearchPage() {
  const [mediaKind, setMediaKind] = createSignal<MediaKind>("movie");
  const [terms, setTerms] = createSignal("");
  const [hasSearched, setHasSearched] = createSignal(false);
  const search = useMutation(() => ({
    mutationFn: (request: SearchRequest) => api.search(request),
    onMutate: () => setHasSearched(true),
  }));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const query = terms().trim();
    if (!query) return;
    search.mutate({ mediaKind: mediaKind(), query, limit: 50 });
  }

  return (
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        eyebrow="Search / All media"
        title="Search every collection"
        description="Find local candidates across every supported media domain before opening a scrape job."
      />

      <form
        class="my-8 grid grid-cols-[minmax(150px,0.35fr)_minmax(280px,1.25fr)_auto] items-end gap-4 border-y border-line border-t-2 border-t-ink py-8 max-[700px]:grid-cols-1"
        onSubmit={submit}
      >
        <FormField label="Media kind">
          <select
            value={mediaKind()}
            onChange={(event) =>
              setMediaKind(event.currentTarget.value as MediaKind)
            }
          >
            <For each={mediaKinds}>
              {(kind) => <option value={kind.value}>{kind.label}</option>}
            </For>
          </select>
        </FormField>
        <FormField label="Search terms">
          <input
            type="search"
            value={terms()}
            placeholder="Title, artist, author, or filename"
            required
            onInput={(event) => setTerms(event.currentTarget.value)}
          />
        </FormField>
        <Button
          class="min-h-[46px] max-[700px]:w-full"
          type="submit"
          disabled={search.isPending || !terms().trim()}
        >
          {search.isPending ? "Searching…" : "Search library"}
        </Button>
      </form>

      <div
        class="mb-20 grid grid-cols-5 overflow-x-auto border-b border-line max-[800px]:grid-cols-[repeat(5,minmax(140px,1fr))]"
        aria-label="Supported search domains"
      >
        <For each={mediaKinds}>
          {(kind, index) => (
            <div
              class={`grid min-h-[118px] border-r border-line p-4 last:border-r-0 ${kind.value === mediaKind() ? "bg-success-surface text-ink" : "text-muted"}`}
            >
              <span class="font-serif text-sm font-medium">0{index() + 1}</span>
              <strong class="self-end font-serif text-lg font-medium">
                {kind.label}
              </strong>
              <small class="text-[0.66rem]">{kind.note}</small>
            </div>
          )}
        </For>
      </div>

      <section
        class="border-t border-line pt-8"
        aria-labelledby="search-results-title"
        aria-live="polite"
      >
        <SectionHeader
          eyebrow="Root-relative index"
          title="Matches"
          titleId="search-results-title"
          meta={
            <Show when={search.isSuccess}>
              <CountBadge>{search.data?.results.length ?? 0} found</CountBadge>
            </Show>
          }
        />
        <Show when={search.isError}>
          <RequestError error={search.error} />
        </Show>
        <Show when={!hasSearched()}>
          <EmptyState
            title="Start with a known fragment"
            description="Search inspects configured roots only. It never accepts a host filesystem path."
          />
        </Show>
        <Show when={search.isSuccess && search.data?.results.length === 0}>
          <EmptyState
            title="No local matches"
            description="Try a shorter title fragment or another media kind."
          />
        </Show>
        <div class="mt-8 border-t-2 border-ink">
          <For each={search.data?.results ?? []}>
            {(match, index) => (
              <article class="grid min-h-[92px] grid-cols-[54px_minmax(0,1fr)_auto] items-center gap-4 border-b border-line max-[700px]:grid-cols-[40px_minmax(0,1fr)] max-[700px]:py-4">
                <span class="font-serif text-sm font-medium text-muted">
                  {String(index() + 1).padStart(2, "0")}
                </span>
                <div>
                  <h3 class="m-0 font-serif text-lg font-medium">
                    {match.name}
                  </h3>
                  <p class="mt-1 mb-0 wrap-anywhere text-xs text-muted">
                    {match.path}
                  </p>
                </div>
                <code class="text-xs text-muted max-[700px]:col-start-2">
                  {match.root_id}
                </code>
              </article>
            )}
          </For>
        </div>
        <Show when={search.data?.truncated}>
          <p class="my-4 border-l-[3px] border-coral bg-danger-surface px-4 py-3">
            Results were capped. Refine the search to inspect a smaller set.
          </p>
        </Show>
      </section>
    </div>
  );
}
