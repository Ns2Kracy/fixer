import { For, Show } from "solid-js";

import type { OperationArtifact } from "../lib/api";

const labels = {
  create_directory: "Create directory",
  write: "Write metadata",
  copy: "Copy media",
  symlink: "Create symlink",
  hardlink: "Create hardlink",
  reflink: "Create Reflink",
} as const;

export function OutputDiff(props: {
  operations: OperationArtifact[];
  outputRoot: string;
}) {
  return (
    <section
      class="my-12 border-t-2 border-ink"
      aria-label="Filesystem operations"
    >
      <p class="m-0 grid grid-cols-[140px_minmax(0,1fr)] gap-4 border-b border-line py-4 max-[640px]:grid-cols-1">
        <span class="text-[0.68rem] font-bold uppercase tracking-[0.1em] text-muted">
          Output root
        </span>
        <code class="wrap-anywhere">{props.outputRoot}</code>
      </p>
      <ol class="m-0 list-none p-0">
        <For each={props.operations}>
          {(operation) => (
            <li class="grid min-h-[76px] grid-cols-[50px_130px_minmax(0,1fr)_auto] items-center gap-4 border-b border-line max-[640px]:grid-cols-[36px_minmax(0,1fr)] max-[640px]:py-4">
              <span class="font-serif text-sm font-medium text-muted">
                {String(operation.index + 1).padStart(2, "0")}
              </span>
              <span class="text-[0.68rem] font-bold uppercase tracking-[0.06em] max-[640px]:col-start-2">
                {labels[operation.kind]}
              </span>
              <code class="min-w-0 wrap-anywhere text-sm max-[640px]:col-start-2">
                {operation.source
                  ? `${operation.source} → ${operation.target}`
                  : operation.target}
              </code>
              <Show when={operation.content_bytes !== undefined}>
                <small class="text-[0.68rem] text-muted max-[640px]:col-start-2">
                  {operation.content_bytes} bytes prepared
                </small>
              </Show>
            </li>
          )}
        </For>
      </ol>
    </section>
  );
}
