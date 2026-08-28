import { For, Show, createSignal } from "solid-js";
import { useMutation } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";

import { RequestError } from "../components/request-error";
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
    <div class="workspace-page search-page">
      <header class="workspace-heading">
        <div>
          <p class="eyebrow">Search / All media</p>
          <h1>Search every collection</h1>
        </div>
        <p>
          Find local candidates across every supported media domain before
          opening a scrape job.
        </p>
      </header>

      <form class="search-console" onSubmit={submit}>
        <label class="media-kind-control">
          <span>Media kind</span>
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
        </label>
        <label class="search-terms-control">
          <span>Search terms</span>
          <input
            type="search"
            value={terms()}
            placeholder="Title, artist, author, or filename"
            required
            onInput={(event) => setTerms(event.currentTarget.value)}
          />
        </label>
        <button
          class="button primary"
          type="submit"
          disabled={search.isPending || !terms().trim()}
        >
          {search.isPending ? "Searching…" : "Search library"}
        </button>
      </form>

      <div class="media-ledger" aria-label="Supported search domains">
        <For each={mediaKinds}>
          {(kind, index) => (
            <div class={kind.value === mediaKind() ? "active" : ""}>
              <span>0{index() + 1}</span>
              <strong>{kind.label}</strong>
              <small>{kind.note}</small>
            </div>
          )}
        </For>
      </div>

      <section
        class="search-results"
        aria-labelledby="search-results-title"
        aria-live="polite"
      >
        <div class="section-heading">
          <div>
            <p class="eyebrow">Root-relative index</p>
            <h2 id="search-results-title">Matches</h2>
          </div>
          <Show when={search.isSuccess}>
            <span class="count">{search.data?.results.length ?? 0} found</span>
          </Show>
        </div>
        <Show when={search.isError}>
          <RequestError error={search.error} />
        </Show>
        <Show when={!hasSearched()}>
          <div class="empty-inline">
            <div>
              <h3>Start with a known fragment</h3>
              <p>
                Search inspects configured roots only. It never accepts a host
                filesystem path.
              </p>
            </div>
          </div>
        </Show>
        <Show when={search.isSuccess && search.data?.results.length === 0}>
          <div class="empty-inline">
            <div>
              <h3>No local matches</h3>
              <p>Try a shorter title fragment or another media kind.</p>
            </div>
          </div>
        </Show>
        <div class="match-list">
          <For each={search.data?.results ?? []}>
            {(match, index) => (
              <article>
                <span class="match-number">
                  {String(index() + 1).padStart(2, "0")}
                </span>
                <div>
                  <h3>{match.name}</h3>
                  <p>{match.path}</p>
                </div>
                <code>{match.root_id}</code>
              </article>
            )}
          </For>
        </div>
        <Show when={search.data?.truncated}>
          <p class="truncation-note">
            Results were capped. Refine the search to inspect a smaller set.
          </p>
        </Show>
      </section>
    </div>
  );
}
