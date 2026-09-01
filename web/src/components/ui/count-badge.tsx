import type { JSX } from "@solidjs/web";

interface CountBadgeProps {
  children: JSX.Element;
  class?: string;
}

export function CountBadge(props: CountBadgeProps): JSX.Element {
  return (
    <span
      class={`inline-flex items-center rounded-full border border-line px-2.5 py-1 text-[0.68rem] font-medium uppercase tracking-wide text-muted ${props.class ?? ""}`}
    >
      {props.children}
    </span>
  );
}
