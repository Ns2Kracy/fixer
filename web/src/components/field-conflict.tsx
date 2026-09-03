import { For, Show } from "solid-js";

import type { ConflictArtifact } from "../lib/api";

export function FieldConflict(props: {
  conflict: ConflictArtifact;
  acknowledged: boolean;
  onToggle: (checked: boolean) => void;
}) {
  return (
    <article class="border-t border-line py-6">
      <div class="flex items-start justify-between gap-8 max-[640px]:flex-col max-[640px]:gap-2">
        <div>
          <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            Field / {props.conflict.field_path}
          </p>
          <h3 class="m-0 font-serif text-[1.35rem] font-medium">
            {props.conflict.message}
          </h3>
        </div>
        <span class="text-[0.7rem] uppercase text-muted">
          {props.conflict.providers.length} sources
        </span>
      </div>
      <Show
        when={props.conflict.sources.length > 0}
        fallback={
          <p class="text-xs text-muted">
            Providers: {props.conflict.providers.join(", ")}
          </p>
        }
      >
        <ul
          class="my-4 flex list-none flex-wrap gap-2 p-0"
          aria-label={`Sources for ${props.conflict.field_path}`}
        >
          <For each={props.conflict.sources}>
            {(source) => (
              <li class="flex gap-2 border border-line px-3 py-1.5 text-xs text-muted">
                {source.locale !== undefined && source.locale !== ""
                  ? `${source.locale} · `
                  : ""}
                {source.provider}
                <Show when={source.external_id}>
                  {(id) => (
                    <small class="text-muted">
                      {id().namespace}:{id().value}
                    </small>
                  )}
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <Show when={props.conflict.providers_truncated}>
        <p class="mt-2 text-xs text-danger">
          Additional provider context was omitted by the server.
        </p>
      </Show>
      <Show when={props.conflict.sources_truncated}>
        <p class="mt-2 text-xs text-danger">
          Additional source context was omitted by the server.
        </p>
      </Show>
      <label class="flex max-w-[680px] gap-3 text-sm text-muted">
        <input
          class="mt-[0.15rem] size-[1.05rem] shrink-0 accent-moss"
          type="checkbox"
          checked={props.acknowledged}
          aria-label={`Acknowledge conflict ${props.conflict.field_path}`}
          onChange={(event) => {
            props.onToggle(event.currentTarget.checked);
          }}
        />
        I acknowledge this conflict after reviewing the available source and
        locale context.
      </label>
    </article>
  );
}
