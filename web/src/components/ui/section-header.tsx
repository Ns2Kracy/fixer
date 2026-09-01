import type { JSX } from "@solidjs/web";
import { Show } from "solid-js";

interface SectionHeaderProps {
  eyebrow?: JSX.Element;
  title: JSX.Element;
  titleId?: string;
  meta?: JSX.Element;
  class?: string;
}

export function SectionHeader(props: SectionHeaderProps): JSX.Element {
  return (
    <div
      class={`flex flex-wrap items-end justify-between gap-4 ${props.class ?? ""}`}
    >
      <div>
        <Show when={props.eyebrow}>
          <p class="mb-2 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            {props.eyebrow}
          </p>
        </Show>
        <h2
          id={props.titleId}
          class="m-0 font-serif text-3xl font-medium tracking-[-0.02em] text-ink text-balance"
        >
          {props.title}
        </h2>
      </div>
      {props.meta}
    </div>
  );
}
