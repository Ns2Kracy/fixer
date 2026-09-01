import type { JSX } from "@solidjs/web";

interface LoadingStateProps {
  children: JSX.Element;
  class?: string;
}

export function LoadingState(props: LoadingStateProps): JSX.Element {
  return (
    <p
      role="status"
      aria-live="polite"
      class={`my-8 text-sm text-muted ${props.class ?? ""}`}
    >
      {props.children}
    </p>
  );
}
