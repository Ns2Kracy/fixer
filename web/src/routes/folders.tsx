import { useMutation, useQuery, useQueryClient } from "@tanstack/solid-query";
import { Link, createFileRoute } from "@tanstack/solid-router";
import { For, Show, createMemo, createSignal } from "solid-js";

import { DirectoryPicker } from "../components/directory-picker";
import { RequestError } from "../components/request-error";
import { Button } from "../components/ui/button";
import { CountBadge } from "../components/ui/count-badge";
import { EmptyState } from "../components/ui/empty-state";
import { FormField } from "../components/ui/form-field";
import { LoadingState } from "../components/ui/loading-state";
import { PageHeader } from "../components/ui/page-header";
import { SectionHeader } from "../components/ui/section-header";
import {
  api,
  isMediaKind,
  type DirectoryRef,
  type IngestionPlacement,
  type IngestionRuleDto,
  type IngestionRuleRequest,
  type MediaKind,
} from "../lib/api";

export const Route = createFileRoute("/folders")({
  component: FoldersPage,
});

type MediaModeValue = "auto" | MediaKind;

interface RuleDraft {
  id?: number;
  name: string;
  source: DirectoryRef | null;
  destination: DirectoryRef | null;
  mediaMode: MediaModeValue;
  placement: IngestionPlacement | "";
  customTemplate: boolean;
  pathTemplate: string;
  enabled: boolean;
}

const mediaKinds: Array<{ value: MediaKind; label: string }> = [
  { value: "movie", label: "Movies" },
  { value: "television", label: "Television" },
  { value: "anime", label: "Anime" },
  { value: "music", label: "Music" },
  { value: "book", label: "Books" },
];

const placements: Array<{ value: IngestionPlacement; label: string }> = [
  { value: "move", label: "Move" },
  { value: "copy", label: "Copy" },
  { value: "hardlink", label: "Hard link" },
  { value: "symlink", label: "Symbolic link" },
  { value: "reflink", label: "Reflink" },
];

function isIngestionPlacement(value: string): value is IngestionPlacement {
  return placements.some((placement) => placement.value === value);
}

const statusLabels = {
  watching: "Watching",
  processing: "Processing",
  needs_review: "Needs review",
  paused: "Paused",
  error: "Error",
} as const;

const presets: Record<MediaModeValue, string> = {
  auto: "Preset chosen after media detection",
  movie: "{{ title | sanitize }} ({{ year }})",
  television: "{{ title | sanitize }}/Season {{ season }}",
  anime: "{{ title | sanitize }}/Season {{ season }}",
  music: "{{ artist | sanitize }}/{{ title | sanitize }}",
  book: "{{ author | sanitize }}/{{ title | sanitize }}",
};

function emptyDraft(): RuleDraft {
  return {
    name: "",
    source: null,
    destination: null,
    mediaMode: "auto",
    placement: "",
    customTemplate: false,
    pathTemplate: "",
    enabled: true,
  };
}

function draftFromRule(rule: IngestionRuleDto): RuleDraft {
  return {
    id: rule.id,
    name: rule.name,
    source: rule.source,
    destination: rule.destination,
    mediaMode:
      rule.media_kind_mode === "auto" ? "auto" : rule.media_kind_mode.fixed,
    placement: rule.placement,
    customTemplate: rule.path_template_override !== null,
    pathTemplate: rule.path_template_override ?? "",
    enabled: rule.enabled,
  };
}

function requestFromDraft(draft: RuleDraft): IngestionRuleRequest | null {
  if (
    !draft.name.trim() ||
    !draft.source ||
    !draft.destination ||
    !draft.placement ||
    (draft.customTemplate && !draft.pathTemplate.trim())
  ) {
    return null;
  }
  return {
    name: draft.name.trim(),
    source: draft.source,
    destination: draft.destination,
    media_kind_mode:
      draft.mediaMode === "auto" ? "auto" : { fixed: draft.mediaMode },
    placement: draft.placement,
    path_template_override: draft.customTemplate
      ? draft.pathTemplate.trim()
      : null,
    enabled: draft.enabled,
  };
}

function requestFromRule(
  rule: IngestionRuleDto,
  enabled: boolean,
): IngestionRuleRequest {
  return {
    name: rule.name,
    source: rule.source,
    destination: rule.destination,
    media_kind_mode: rule.media_kind_mode,
    placement: rule.placement,
    path_template_override: rule.path_template_override,
    enabled,
  };
}

function FoldersPage() {
  const queryClient = useQueryClient();
  const [draft, setDraft] = createSignal<RuleDraft | null>(null);
  const [previewPath, setPreviewPath] = createSignal<string>();
  const rules = useQuery(() => ({
    queryKey: ["ingestion-rules"],
    queryFn: () => api.listIngestionRules(),
  }));
  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: ["ingestion-rules"] });

  const save = useMutation(() => ({
    mutationFn: (variables: { id?: number; request: IngestionRuleRequest }) =>
      variables.id === undefined
        ? api.createIngestionRule(variables.request)
        : api.updateIngestionRule(variables.id, variables.request),
    onSuccess: async () => {
      setDraft(null);
      setPreviewPath();
      await refresh();
    },
  }));
  const toggle = useMutation(() => ({
    mutationFn: (variables: { rule: IngestionRuleDto; enabled: boolean }) =>
      api.updateIngestionRule(
        variables.rule.id,
        requestFromRule(variables.rule, variables.enabled),
      ),
    onSuccess: refresh,
  }));
  const scan = useMutation(() => ({
    mutationFn: (id: number) => api.rescanIngestionRule(id),
    onSuccess: refresh,
  }));
  const remove = useMutation(() => ({
    mutationFn: (id: number) => api.deleteIngestionRule(id),
    onSuccess: refresh,
  }));
  const preview = useMutation(() => ({
    mutationFn: (pathTemplate: string) =>
      api.previewTemplate({
        path_template: pathTemplate,
        content_template: "{{ title }}",
        sample: {
          title: "Example Title",
          id: "example-id",
          year: 2024,
          edition: null,
        },
      }),
    onSuccess: (response) => setPreviewPath(response.path),
  }));

  const canSave = createMemo(() => {
    const current = draft();
    return current ? requestFromDraft(current) !== null : false;
  });

  function patch<K extends keyof RuleDraft>(key: K, value: RuleDraft[K]) {
    setDraft((current) => (current ? { ...current, [key]: value } : current));
    if (key === "pathTemplate" || key === "mediaMode") setPreviewPath();
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const current = draft();
    if (!current) return;
    const request = requestFromDraft(current);
    if (request) {
      save.mutate(
        current.id === undefined ? { request } : { id: current.id, request },
      );
    }
  }

  return (
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        title="Folders"
        aside={
          <Button
            type="button"
            onClick={() => {
              setDraft(emptyDraft());
              setPreviewPath();
            }}
          >
            Add folder rule
          </Button>
        }
      />

      <Show when={draft()}>
        {(form) => (
          <section
            class="my-10 border-2 border-ink bg-surface-muted p-[clamp(1rem,4vw,2.5rem)] shadow-[8px_8px_0_var(--color-moss)]"
            aria-labelledby="folder-editor-title"
          >
            <SectionHeader
              eyebrow={
                form().id === undefined ? "New rule" : `Rule #${form().id}`
              }
              title={
                form().id === undefined ? "Add folder rule" : "Edit folder rule"
              }
              titleId="folder-editor-title"
            />
            <form class="mt-8 grid gap-7" onSubmit={submit}>
              <fieldset
                class="grid grid-cols-2 gap-5 border-0 p-0 max-[720px]:grid-cols-1"
                disabled={save.isPending}
              >
                <legend class="sr-only">Folder rule fields</legend>
                <FormField label="Rule name">
                  <input
                    type="text"
                    required
                    value={form().name}
                    onInput={(event) => {
                      patch("name", event.currentTarget.value);
                    }}
                  />
                </FormField>
                <FormField label="Media detection">
                  <select
                    value={form().mediaMode}
                    onChange={(event) => {
                      const value = event.currentTarget.value;
                      if (value === "auto" || isMediaKind(value)) {
                        patch("mediaMode", value);
                      }
                    }}
                  >
                    <option value="auto">Automatic</option>
                    <For each={mediaKinds}>
                      {(kind) => (
                        <option value={kind.value}>{kind.label}</option>
                      )}
                    </For>
                  </select>
                </FormField>
                <DirectoryPicker
                  label="Source folder"
                  buttonLabel="Choose source"
                  value={form().source}
                  onSelect={(directory) => {
                    patch("source", directory);
                  }}
                />
                <DirectoryPicker
                  label="Destination folder"
                  buttonLabel="Choose destination"
                  value={form().destination}
                  onSelect={(directory) => {
                    patch("destination", directory);
                  }}
                />
                <FormField label="Organization method">
                  <select
                    required
                    value={form().placement}
                    onChange={(event) => {
                      const value = event.currentTarget.value;
                      if (value === "" || isIngestionPlacement(value)) {
                        patch("placement", value);
                      }
                    }}
                  >
                    <option value="">Choose a method</option>
                    <For each={placements}>
                      {(placement) => (
                        <option value={placement.value}>
                          {placement.label}
                        </option>
                      )}
                    </For>
                  </select>
                </FormField>
                <div class="border-t border-line pt-4">
                  <span class="text-xs font-bold uppercase tracking-[0.12em] text-muted">
                    Built-in template
                  </span>
                  <code class="mt-2 block text-xs wrap-anywhere">
                    {presets[form().mediaMode]}
                  </code>
                </div>
                <label class="col-span-full flex items-center gap-3 border-t border-line pt-5 text-sm max-[720px]:col-span-1">
                  <input
                    class="size-4 accent-moss"
                    type="checkbox"
                    checked={form().customTemplate}
                    onChange={(event) => {
                      patch("customTemplate", event.currentTarget.checked);
                    }}
                  />
                  Use custom path template
                </label>
                <Show when={form().customTemplate}>
                  <FormField
                    class="col-span-full max-[720px]:col-span-1"
                    label="Path template override"
                  >
                    <input
                      type="text"
                      required
                      value={form().pathTemplate}
                      onInput={(event) => {
                        patch("pathTemplate", event.currentTarget.value);
                      }}
                    />
                  </FormField>
                  <div class="col-span-full flex flex-wrap items-center gap-4 max-[720px]:col-span-1">
                    <Button
                      type="button"
                      variant="secondary"
                      disabled={
                        !form().pathTemplate.trim() || preview.isPending
                      }
                      onClick={() => {
                        preview.mutate(form().pathTemplate.trim());
                      }}
                    >
                      {preview.isPending ? "Previewing…" : "Preview path"}
                    </Button>
                    <Show when={previewPath()}>
                      {(path) => (
                        <code aria-label="Template preview">{path()}</code>
                      )}
                    </Show>
                  </div>
                  <Show when={preview.isError}>
                    <div class="col-span-full max-[720px]:col-span-1">
                      <RequestError error={preview.error} />
                    </div>
                  </Show>
                </Show>
              </fieldset>

              <Show when={save.isError}>
                <RequestError error={save.error} />
              </Show>
              <div class="flex justify-end gap-3">
                <Button
                  type="button"
                  variant="secondary"
                  onClick={() => setDraft(null)}
                >
                  Cancel
                </Button>
                <Button type="submit" disabled={!canSave() || save.isPending}>
                  {save.isPending ? "Saving…" : "Save rule"}
                </Button>
              </div>
            </form>
          </section>
        )}
      </Show>

      <section
        class="mt-10 border-t-2 border-ink pt-7"
        aria-labelledby="folder-rules-title"
      >
        <SectionHeader
          title="Folder rules"
          titleId="folder-rules-title"
          meta={<CountBadge>{rules.data?.rules.length ?? 0} rules</CountBadge>}
        />
        <Show when={rules.isPending}>
          <LoadingState>Loading folder rules…</LoadingState>
        </Show>
        <Show when={rules.isError}>
          <RequestError error={rules.error} />
        </Show>
        <Show when={rules.isSuccess && rules.data?.rules.length === 0}>
          <EmptyState title="No folder rules" />
        </Show>
        <Show when={toggle.isError}>
          <RequestError error={toggle.error} />
        </Show>
        <Show when={scan.isError}>
          <RequestError error={scan.error} />
        </Show>
        <Show when={remove.isError}>
          <RequestError error={remove.error} />
        </Show>

        <div class="mt-7 border-t border-ink">
          <For each={rules.data?.rules ?? []}>
            {(rule) => (
              <article class="grid grid-cols-[minmax(0,1.5fr)_minmax(140px,0.55fr)_auto] items-center gap-5 border-b border-line py-6 max-[860px]:grid-cols-1 max-[860px]:gap-3">
                <div class="min-w-0">
                  <div class="flex flex-wrap items-center gap-3">
                    <h3 class="m-0 font-serif text-xl font-medium">
                      {rule.name}
                    </h3>
                    <span
                      class={`text-xs font-bold uppercase tracking-[0.08em] ${rule.status === "error" ? "text-danger" : rule.status === "processing" ? "text-moss" : "text-muted"}`}
                    >
                      {statusLabels[rule.status]}
                    </span>
                  </div>
                  <p class="my-2 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-sm">
                    {rule.source.path || "Root"} <span aria-label="to">→</span>{" "}
                    {rule.destination.path || "Root"}
                  </p>
                  <p class="m-0 text-xs capitalize text-muted">
                    {rule.media_kind_mode === "auto"
                      ? "Automatic media"
                      : rule.media_kind_mode.fixed}{" "}
                    · {rule.placement} · last run{" "}
                    {new Date(rule.updated_at_ms).toLocaleString()}
                  </p>
                  <Show when={rule.last_error}>
                    {(message) => (
                      <p class="mt-2 mb-0 text-xs text-danger">{message()}</p>
                    )}
                  </Show>
                </div>

                <Show when={rule.status === "needs_review"} fallback={<span />}>
                  <Link
                    class="text-sm font-bold no-underline hover:text-moss"
                    to="/jobs"
                  >
                    Review jobs <span aria-hidden="true">→</span>
                  </Link>
                </Show>

                <div class="flex flex-wrap justify-end gap-2 max-[860px]:justify-start">
                  <Button
                    class="min-h-9 px-3 py-2 text-xs"
                    type="button"
                    variant="secondary"
                    disabled={toggle.isPending}
                    onClick={() => {
                      toggle.mutate({ rule, enabled: !rule.enabled });
                    }}
                  >
                    {rule.enabled ? "Pause" : "Resume"}
                  </Button>
                  <Button
                    class="min-h-9 px-3 py-2 text-xs"
                    type="button"
                    variant="secondary"
                    disabled={scan.isPending}
                    onClick={() => {
                      scan.mutate(rule.id);
                    }}
                  >
                    Scan now
                  </Button>
                  <Button
                    class="min-h-9 px-3 py-2 text-xs"
                    type="button"
                    variant="secondary"
                    onClick={() => {
                      setDraft(draftFromRule(rule));
                      setPreviewPath();
                    }}
                  >
                    Edit
                  </Button>
                  <Button
                    class="min-h-9 px-3 py-2 text-xs"
                    type="button"
                    variant="danger"
                    disabled={remove.isPending}
                    onClick={() => {
                      remove.mutate(rule.id);
                    }}
                  >
                    Delete
                  </Button>
                </div>
              </article>
            )}
          </For>
        </div>
      </section>
    </div>
  );
}
