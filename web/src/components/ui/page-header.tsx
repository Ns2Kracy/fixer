import type { JSX } from "@solidjs/web";
import { Show } from "solid-js";

interface PageHeaderProps {
  eyebrow?: JSX.Element;
  title: JSX.Element;
  titleId?: string;
  description?: JSX.Element;
  aside?: JSX.Element;
  variant?: "workspace" | "detail";
  class?: string;
}

const variants = {
  workspace:
    "grid items-end gap-8 border-b border-line py-4 pb-16 md:grid-cols-[minmax(0,1.45fr)_minmax(260px,0.55fr)] md:gap-[clamp(2rem,6vw,6rem)]",
  detail:
    "flex flex-col items-start justify-between gap-8 border-b border-line py-4 pb-12 md:flex-row md:items-end md:gap-12",
} as const;

export function PageHeader(props: PageHeaderProps): JSX.Element {
  const variant = () => props.variant ?? "workspace";

  return (
    <header class={`${variants[variant()]} ${props.class ?? ""}`}>
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
        <Show when={variant() === "detail" && props.description}>
          <p class="mt-6 max-w-[680px] text-muted text-pretty">
            {props.description}
          </p>
        </Show>
      </div>
      <Show when={variant() === "workspace" || props.aside}>
        <div class="max-w-[65ch] text-sm text-muted">
          <Show when={variant() === "workspace" && props.description}>
            <p class="m-0 text-pretty">{props.description}</p>
          </Show>
          {props.aside}
        </div>
      </Show>
    </header>
  );
}
