import type { JSX } from "@solidjs/web";
import { Show } from "solid-js";

interface FormFieldProps {
  label: JSX.Element;
  children: JSX.Element;
  hint?: JSX.Element;
  for?: string;
  class?: string;
}

export function FormField(props: FormFieldProps): JSX.Element {
  return (
    <div class={`grid gap-2 ${props.class ?? ""}`}>
      <label
        for={props.for}
        class="grid gap-2 text-sm font-medium text-muted [&>input]:min-h-11 [&>input]:border [&>input]:border-line [&>input]:bg-surface [&>input]:px-3 [&>input]:py-2.5 [&>input]:text-ink [&>input]:outline-none [&>input]:transition-colors [&>input]:focus-visible:border-coral [&>select]:min-h-11 [&>select]:border [&>select]:border-line [&>select]:bg-surface [&>select]:px-3 [&>select]:py-2.5 [&>select]:text-ink [&>select]:outline-none [&>select]:transition-colors [&>select]:focus-visible:border-coral [&>textarea]:min-h-32 [&>textarea]:border [&>textarea]:border-line [&>textarea]:bg-surface [&>textarea]:px-3 [&>textarea]:py-2.5 [&>textarea]:text-ink [&>textarea]:outline-none [&>textarea]:transition-colors [&>textarea]:focus-visible:border-coral"
      >
        <span>{props.label}</span>
        {props.children}
      </label>
      <Show when={props.hint}>
        <small class="font-normal leading-relaxed text-muted">
          {props.hint}
        </small>
      </Show>
    </div>
  );
}
