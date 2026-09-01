import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { render, screen } from "../../test/render";
import { Button, buttonStyles } from "./button";
import { CountBadge } from "./count-badge";
import { EmptyState } from "./empty-state";
import { FormField } from "./form-field";
import { LoadingState } from "./loading-state";
import { Notice } from "./notice";
import { PageHeader } from "./page-header";
import { SectionHeader } from "./section-header";

describe("shared UI components", () => {
  it("renders button variants without losing native button behavior", async () => {
    const onClick = vi.fn();
    const user = userEvent.setup();

    render(() => (
      <Button variant="danger" type="button" onClick={onClick}>
        Delete output
      </Button>
    ));

    const button = screen.getByRole("button", { name: "Delete output" });
    expect(button).toHaveClass("bg-coral");
    expect(buttonStyles("secondary")).toContain("bg-transparent");
    await user.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("renders reusable page and section heading structures", () => {
    render(() => (
      <>
        <PageHeader
          eyebrow="Settings / Scraper policy"
          title="Workspace settings"
          description="Control provider and output policy."
          aside={<span>Saved</span>}
        />
        <SectionHeader
          eyebrow="Providers"
          title="Readiness ledger"
          titleId="providers-title"
          meta={<CountBadge>4 ready</CountBadge>}
        />
      </>
    ));

    expect(
      screen.getByRole("heading", { level: 1, name: "Workspace settings" }),
    ).toBeVisible();
    expect(screen.getByText("Control provider and output policy.")).toBeVisible();
    expect(screen.getByText("Saved")).toBeVisible();
    expect(
      screen.getByRole("heading", { level: 2, name: "Readiness ledger" }),
    ).toHaveAttribute("id", "providers-title");
    expect(screen.getByText("4 ready")).toBeVisible();
  });

  it("associates form labels and hints with native controls", () => {
    render(() => (
      <FormField label="Proxy URL" hint="HTTP, HTTPS, or SOCKS proxy.">
        <input type="url" />
      </FormField>
    ));

    expect(screen.getByRole("textbox", { name: "Proxy URL" })).toBeVisible();
    expect(screen.getByText("HTTP, HTTPS, or SOCKS proxy.")).toBeVisible();
  });

  it("renders reusable notices with semantic tone and roles", () => {
    render(() => (
      <Notice tone="danger" role="alert">
        The bounded response was truncated.
      </Notice>
    ));

    const notice = screen.getByRole("alert");
    expect(notice).toHaveTextContent("The bounded response was truncated.");
    expect(notice).toHaveClass("border-danger", "bg-danger-surface");
  });

  it("announces loading and empty states", () => {
    render(() => (
      <>
        <LoadingState>Loading workspace policy…</LoadingState>
        <EmptyState
          title="No jobs yet"
          description="New scans and review sessions will appear here."
          glyph="◇"
        />
      </>
    ));

    expect(screen.getByText("Loading workspace policy…")).toHaveAttribute(
      "role",
      "status",
    );
    expect(screen.getByRole("status", { name: "No jobs yet" })).toBeVisible();
  });
});
