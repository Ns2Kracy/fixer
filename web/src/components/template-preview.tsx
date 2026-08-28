import { Show } from "solid-js";
import type { JSX } from "@solidjs/web";

import type { TemplatePreviewEnvelope } from "../lib/api";
import { RequestError } from "./request-error";

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
    <div class="template-workbench">
      <form class="template-form" onSubmit={props.onSubmit}>
        <div class="template-block">
          <p class="eyebrow">01 / Output path</p>
          <label>
            <span>Path template</span>
            <input
              type="text"
              value={props.pathTemplate}
              required
              disabled={props.pending}
              onInput={(event) =>
                props.onPathTemplate(event.currentTarget.value)
              }
            />
          </label>
          <small>
            Variables: title, id, year, edition. Filters: sanitize, lower,
            upper.
          </small>
        </div>

        <div class="template-block">
          <p class="eyebrow">02 / Sidecar content</p>
          <label>
            <span>Content template</span>
            <textarea
              rows="8"
              value={props.contentTemplate}
              required
              disabled={props.pending}
              onInput={(event) =>
                props.onContentTemplate(event.currentTarget.value)
              }
            />
          </label>
        </div>

        <fieldset class="sample-fields" disabled={props.pending}>
          <legend>Preview sample</legend>
          <label>
            <span>Sample title</span>
            <input
              type="text"
              value={props.title}
              required
              onInput={(event) => props.onTitle(event.currentTarget.value)}
            />
          </label>
          <label>
            <span>Sample ID</span>
            <input
              type="text"
              value={props.id}
              required
              onInput={(event) => props.onId(event.currentTarget.value)}
            />
          </label>
          <label>
            <span>Sample year</span>
            <input
              type="number"
              min="0"
              max="65535"
              value={props.year}
              onInput={(event) => props.onYear(event.currentTarget.value)}
            />
          </label>
          <label>
            <span>Sample edition</span>
            <input
              type="text"
              value={props.edition}
              placeholder="Director's cut"
              onInput={(event) => props.onEdition(event.currentTarget.value)}
            />
          </label>
        </fieldset>

        <button class="button primary" type="submit" disabled={props.pending}>
          {props.pending ? "Rendering…" : "Preview template"}
        </button>
        <Show when={props.error}>
          {(error) => <RequestError error={error()} />}
        </Show>
      </form>

      <section
        class="template-output"
        aria-labelledby="template-output-title"
        aria-live="polite"
      >
        <div class="section-heading">
          <div>
            <p class="eyebrow">No-write render</p>
            <h2 id="template-output-title">Preview</h2>
          </div>
          <Show when={props.preview}>
            {(preview) => (
              <span class="count">{preview().content_bytes} bytes</span>
            )}
          </Show>
        </div>
        <Show
          when={props.preview}
          fallback={
            <div class="empty-inline template-empty">
              <div>
                <h3>Nothing rendered yet</h3>
                <p>
                  Preview validates the relative path and content without
                  touching disk.
                </p>
              </div>
            </div>
          }
        >
          {(preview) => (
            <div class="rendered-preview">
              <div>
                <span>Relative output path</span>
                <code>{preview().path}</code>
              </div>
              <div>
                <span>Rendered content</span>
                <pre>{preview().content}</pre>
              </div>
            </div>
          )}
        </Show>
      </section>
    </div>
  );
}
