import type { JSX } from "@solidjs/web";
import { omit } from "solid-js";

export type ButtonVariant = "primary" | "secondary" | "danger";

const buttonVariants: Record<ButtonVariant, string> = {
  primary:
    "border-ink bg-ink text-paper hover:border-moss hover:bg-moss hover:text-white",
  secondary:
    "border-ink bg-transparent text-ink hover:border-moss hover:bg-moss hover:text-white",
  danger:
    "border-coral bg-coral text-white hover:border-ink hover:bg-ink hover:text-paper",
};

export function buttonStyles(
  variant: ButtonVariant = "primary",
  className = "",
): string {
  return [
    "inline-flex min-h-11 items-center justify-center border px-5 py-3 font-medium no-underline transition-colors duration-200 focus-visible:outline focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-coral disabled:cursor-not-allowed disabled:opacity-50",
    buttonVariants[variant],
    className,
  ]
    .filter(Boolean)
    .join(" ");
}

type ButtonProps = Omit<
  JSX.ButtonHTMLAttributes<HTMLButtonElement>,
  "class"
> & {
  variant?: ButtonVariant;
  class?: string;
};

export function Button(props: ButtonProps): JSX.Element {
  const buttonProps = omit(props, "variant", "class", "children");

  return (
    <button
      {...buttonProps}
      class={buttonStyles(props.variant, props.class ?? "")}
    >
      {props.children}
    </button>
  );
}
