import type { JSX } from "@solidjs/web";
import { Show } from "solid-js";

import type { TemplatePreviewEnvelope } from "../lib/api";
import { RequestError } from "./request-error";
import { Button } from "./ui/button";
import { CountBadge } from "./ui/count-badge";
import { EmptyState } from "./ui/empty-state";
import { FormField } from "./ui/form-field";
import { SectionHeader } from "./ui/section-header";

interface TemplatePreviewProps {
  pathTemplate: string;
  contentTemplate: string;
  title: string;
  id: string;
  year: string;
  edition: string;
  pending: boolean;
  error: Error | null;
  preview: TemplatePreviewEnvelope | undefined;
  onPathTemplate: (value: string) => void;
  onContentTemplate: (value: string) => void;
  onTitle: (value: string) => void;
  onId: (value: string) => void;
  onYear: (value: string) => void;
  onEdition: (value: string) => void;
  onSubmit: (event: SubmitEvent) => void;
}

export function TemplatePreview(props: TemplatePreviewProps): JSX.Element {
  return (
    <div class="mt-12 grid grid-cols-[minmax(0,1.15fr)_minmax(300px,0.85fr)] gap-[clamp(2rem,6vw,6rem)] max-[1000px]:grid-cols-1">
      <form class="grid gap-10" onSubmit={props.onSubmit}>
        <div class="border-t-2 border-ink pt-4">
          <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            01 / Output path
          </p>
          <FormField
            label="Path template"
            hint="Variables: title, id, year, edition. Filters: sanitize, lower, upper."
          >
            <input
              type="text"
              value={props.pathTemplate}
              required
              disabled={props.pending}
              onInput={(event) => {
                props.onPathTemplate(event.currentTarget.value);
              }}
            />
          </FormField>
        </div>

        <div class="border-t-2 border-ink pt-4">
          <p class="mb-3 text-[0.68rem] font-bold uppercase tracking-[0.15em] text-muted">
            02 / Sidecar content
          </p>
          <FormField label="Content template">
            <textarea
              class="w-full resize-y font-mono text-sm leading-relaxed"
              rows="8"
              value={props.contentTemplate}
              required
              disabled={props.pending}
              onInput={(event) => {
                props.onContentTemplate(event.currentTarget.value);
              }}
            />
          </FormField>
        </div>

        <fieldset
          class="grid grid-cols-2 gap-4 border-0 p-0 max-[700px]:grid-cols-1"
          disabled={props.pending}
        >
          <legend class="col-span-full mb-4 w-full border-b border-line pb-3 font-serif text-lg font-medium">
            Preview sample
          </legend>
          <FormField label="Sample title">
            <input
              type="text"
              value={props.title}
              required
              onInput={(event) => {
                props.onTitle(event.currentTarget.value);
              }}
            />
          </FormField>
          <FormField label="Sample ID">
            <input
              type="text"
              value={props.id}
              required
              onInput={(event) => {
                props.onId(event.currentTarget.value);
              }}
            />
          </FormField>
          <FormField label="Sample year">
            <input
              type="number"
              min="0"
              max="65535"
              value={props.year}
              onInput={(event) => {
                props.onYear(event.currentTarget.value);
              }}
            />
          </FormField>
          <FormField label="Sample edition">
            <input
              type="text"
              value={props.edition}
              placeholder="Director's cut"
              onInput={(event) => {
                props.onEdition(event.currentTarget.value);
              }}
            />
          </FormField>
        </fieldset>

        <Button
          class="justify-self-start"
          type="submit"
          disabled={props.pending}
        >
          {props.pending ? "Rendering…" : "Preview template"}
        </Button>
        <Show when={props.error}>
          {(error) => <RequestError error={error()} />}
        </Show>
      </form>

      <section
        class="sticky top-4 self-start border-t-2 border-ink pt-4 max-[1000px]:static"
        aria-labelledby="template-output-title"
        aria-live="polite"
      >
        <SectionHeader
          eyebrow="No-write render"
          title="Preview"
          titleId="template-output-title"
          meta={
            <Show when={props.preview}>
              {(preview) => (
                <CountBadge>{preview().content_bytes} bytes</CountBadge>
              )}
            </Show>
          }
        />
        <Show
          when={props.preview}
          fallback={
            <EmptyState
              class="min-h-[220px]"
              title="Nothing rendered yet"
              description="Preview validates the relative path and content without touching disk."
            />
          }
        >
          {(preview) => (
            <div class="mt-8 border-t border-line">
              <div class="grid gap-3 border-b border-line py-5">
                <span class="text-[0.65rem] font-bold uppercase tracking-[0.1em] text-muted">
                  Relative output path
                </span>
                <code class="wrap-anywhere text-sm">{preview().path}</code>
              </div>
              <div class="grid gap-3 border-b border-line py-5">
                <span class="text-[0.65rem] font-bold uppercase tracking-[0.1em] text-muted">
                  Rendered content
                </span>
                <pre class="m-0 min-h-[150px] whitespace-pre-wrap bg-code p-4 font-mono text-sm leading-relaxed wrap-anywhere text-code-ink">
                  {preview().content}
                </pre>
              </div>
            </div>
          )}
        </Show>
      </section>
    </div>
  );
}
