import { For, Show } from 'solid-js'

import type { OperationArtifact } from '../lib/api'

const labels = {
  create_directory: 'Create directory',
  write: 'Write metadata',
  copy: 'Copy media',
  symlink: 'Create symlink',
  hardlink: 'Create hardlink',
  reflink: 'Create Reflink',
} as const

export function OutputDiff(props: { operations: OperationArtifact[]; outputRoot: string }) {
  return (
    <div class="output-diff">
      <p class="output-root"><span>Output root</span><code>{props.outputRoot}</code></p>
      <ol>
        <For each={props.operations}>
          {(operation) => (
            <li>
              <span class="operation-index">{String(operation.index + 1).padStart(2, '0')}</span>
              <span class="operation-kind">{labels[operation.kind]}</span>
              <code>
                {operation.source ? `${operation.source} → ${operation.target}` : operation.target}
              </code>
              <Show when={operation.content_bytes !== undefined}>
                <small>{operation.content_bytes} bytes prepared</small>
              </Show>
            </li>
          )}
        </For>
      </ol>
    </div>
  )
}
