import type { JSX } from "@solidjs/web";
import { Show } from "solid-js";

interface PageHeaderProps {
  eyebrow?: JSX.Element;
  title: JSX.Element;
  titleId?: string;
  description?: JSX.Element;
  aside?: JSX.Element;
  class?: string;
}

export function PageHeader(props: PageHeaderProps): JSX.Element {
  return (
    <header
      class={`flex flex-col items-start justify-between gap-8 border-b border-line py-4 pb-12 md:flex-row md:items-end md:gap-12 ${props.class ?? ""}`}
    >
      <div>
        <Show when={props.eyebrow}>
          <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            {props.eyebrow}
          </p>
        </Show>
        <h1
          id={props.titleId}
          class="m-0 max-w-[900px] font-serif text-[clamp(3rem,6vw,5.6rem)] font-medium leading-[0.94] tracking-[-0.04em] text-ink text-balance"
        >
          {props.title}
        </h1>
        <Show when={props.description}>
          <p class="mt-6 max-w-[680px] text-muted text-pretty">
            {props.description}
          </p>
        </Show>
      </div>
      <Show when={props.aside}>
        <div class="max-w-[65ch] text-sm text-muted">{props.aside}</div>
      </Show>
    </header>
  );
}
