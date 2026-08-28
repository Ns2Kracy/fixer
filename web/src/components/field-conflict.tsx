import { For, Show } from 'solid-js'

import type { ConflictArtifact } from '../lib/api'

export function FieldConflict(props: {
  conflict: ConflictArtifact
  acknowledged: boolean
  onToggle: (checked: boolean) => void
}) {
  return (
    <article class="field-conflict">
      <div class="conflict-heading">
        <div>
          <p class="eyebrow">Field / {props.conflict.field_path}</p>
          <h3>{props.conflict.message}</h3>
        </div>
        <span>{props.conflict.providers.length} sources</span>
      </div>
      <Show
        when={props.conflict.sources.length > 0}
        fallback={<p class="source-note">Providers: {props.conflict.providers.join(', ')}</p>}
      >
        <ul class="source-list" aria-label={`Sources for ${props.conflict.field_path}`}>
          <For each={props.conflict.sources}>
            {(source) => (
              <li>
                {source.locale ? `${source.locale} · ` : ''}{source.provider}
                <Show when={source.external_id}>
                  {(id) => <small>{id().namespace}:{id().value}</small>}
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <Show when={props.conflict.providers_truncated}>
        <p class="truncation-inline">Additional provider context was omitted by the server.</p>
      </Show>
      <Show when={props.conflict.sources_truncated}>
        <p class="truncation-inline">Additional source context was omitted by the server.</p>
      </Show>
      <label class="conflict-acknowledgement">
        <input
          type="checkbox"
          checked={props.acknowledged}
          aria-label={`Acknowledge conflict ${props.conflict.field_path}`}
          onChange={(event) => props.onToggle(event.currentTarget.checked)}
        />
        I acknowledge this conflict after reviewing the available source and locale context.
      </label>
    </article>
  )
}
