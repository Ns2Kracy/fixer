import { useMutation } from "@tanstack/solid-query";
import { createFileRoute } from "@tanstack/solid-router";
import { createSignal } from "solid-js";

import { TemplatePreview } from "../components/template-preview";
import { PageHeader } from "../components/ui/page-header";
import { api, type TemplatePreviewRequest } from "../lib/api";

export const Route = createFileRoute("/templates")({
  component: TemplatesPage,
});

function TemplatesPage() {
  const [pathTemplate, setPathTemplate] = createSignal(
    "{{title|sanitize}} ({{year}})/metadata.json",
  );
  const [contentTemplate, setContentTemplate] = createSignal(
    "title={{title}}\nid={{id}}",
  );
  const [title, setTitle] = createSignal("Example title");
  const [id, setId] = createSignal("example-title");
  const [year, setYear] = createSignal("2024");
  const [edition, setEdition] = createSignal("");
  const preview = useMutation(() => ({
    mutationFn: (request: TemplatePreviewRequest) =>
      api.previewTemplate(request),
  }));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    preview.mutate({
      path_template: pathTemplate(),
      content_template: contentTemplate(),
      sample: {
        title: title(),
        id: id(),
        year: year() ? Number(year()) : null,
        edition: edition() || null,
      },
    });
  }

  return (
    <div class="mx-auto max-w-[1180px]">
      <PageHeader
        eyebrow="Templates / Dry render"
        title="Template studio"
        description="Shape relative paths and text sidecars against a sample. Every preview is validated and no-write."
      />
      <TemplatePreview
        pathTemplate={pathTemplate()}
        contentTemplate={contentTemplate()}
        title={title()}
        id={id()}
        year={year()}
        edition={edition()}
        pending={preview.isPending}
        error={preview.error}
        preview={preview.data}
        onPathTemplate={setPathTemplate}
        onContentTemplate={setContentTemplate}
        onTitle={setTitle}
        onId={setId}
        onYear={setYear}
        onEdition={setEdition}
        onSubmit={submit}
      />
    </div>
  );
}
