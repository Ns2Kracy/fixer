import type { JSX } from "@solidjs/web";
import { omit } from "solid-js";

export type NoticeTone = "danger" | "success" | "warning";

const toneClasses: Record<NoticeTone, string> = {
  danger: "border-danger bg-danger-surface",
  success: "border-success bg-success-surface",
  warning: "border-warning bg-warning-surface",
};

type NoticeProps = Omit<JSX.HTMLAttributes<HTMLDivElement>, "class"> & {
  tone: NoticeTone;
  class?: string;
};

export function Notice(props: NoticeProps): JSX.Element {
  const noticeProps = omit(props, "tone", "class", "children");

  return (
    <div
      {...noticeProps}
      class={`border px-4 py-3 text-sm text-ink ${toneClasses[props.tone]} ${props.class ?? ""}`}
    >
      {props.children}
    </div>
  );
}
