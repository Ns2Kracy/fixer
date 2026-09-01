import type { JSX } from "@solidjs/web";
import { Show } from "solid-js";

interface EmptyStateProps {
  title: string;
  description?: JSX.Element;
  glyph?: JSX.Element;
  variant?: "inline" | "page";
  class?: string;
}

const variants = {
  inline:
    "mt-8 flex min-h-36 items-center gap-4 border border-dashed border-line p-8",
  page: "max-w-3xl pt-[8vh]",
} as const;

export function EmptyState(props: EmptyStateProps): JSX.Element {
  const variant = () => props.variant ?? "inline";

  return (
    <section
      aria-label={props.title}
      role="status"
      class={`${variants[variant()]} ${props.class ?? ""}`}
    >
      <Show when={props.glyph}>
        <span class="text-3xl text-moss" aria-hidden="true">
          {props.glyph}
        </span>
      </Show>
      <div>
        <h3
          class={
            variant() === "page"
              ? "m-0 font-serif text-[clamp(3rem,8vw,6rem)] font-medium tracking-[-0.04em] text-ink"
              : "m-0 font-serif text-lg font-medium text-ink"
          }
        >
          {props.title}
        </h3>
        <Show when={props.description}>
          <p class="mt-1 max-w-[65ch] text-sm text-muted">
            {props.description}
          </p>
        </Show>
      </div>
    </section>
  );
}
