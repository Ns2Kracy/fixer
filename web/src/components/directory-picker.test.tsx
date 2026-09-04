import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { render, screen, waitFor } from "../test/render";
import type { ListLibraryRequest } from "../lib/api";
import { DirectoryPicker } from "./directory-picker";

function browserApi() {
  return {
    libraryRoots: vi.fn(async () => ({
      schema_version: 1 as const,
      roots: [{ id: "root-media", label: "Media" }],
    })),
    listLibrary: vi.fn(async (request: ListLibraryRequest) => {
      const entries =
        request.path === ""
          ? [
              {
                name: "Downloads",
                path: "Downloads",
                kind: "directory" as const,
              },
              { name: "notes.txt", path: "notes.txt", kind: "file" as const },
            ]
          : request.path === "Downloads"
            ? [
                {
                  name: "Nested",
                  path: "Downloads/Nested",
                  kind: "directory" as const,
                },
                {
                  name: "partial.mkv",
                  path: "Downloads/partial.mkv",
                  kind: "file" as const,
                },
              ]
            : [];
      return {
        schema_version: 1 as const,
        root_id: request.rootId,
        path: request.path ?? "",
        entries,
        truncated: false,
      };
    }),
  };
}

describe("DirectoryPicker", () => {
  it("navigates opaque roots and breadcrumbs and selects directories only", async () => {
    const user = userEvent.setup();
    const api = browserApi();
    const onSelect = vi.fn();
    render(() => (
      <DirectoryPicker
        label="Source folder"
        buttonLabel="Choose source"
        onSelect={onSelect}
        api={api}
      />
    ));

    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    const trigger = screen.getByRole("button", { name: "Choose source" });
    await user.click(trigger);
    await user.click(await screen.findByRole("button", { name: "Media" }));
    expect(api.listLibrary).toHaveBeenLastCalledWith({
      rootId: "root-media",
      path: "",
    });
    expect(
      screen.queryByRole("button", { name: "notes.txt" }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Downloads" }));
    expect(api.listLibrary).toHaveBeenLastCalledWith({
      rootId: "root-media",
      path: "Downloads",
    });
    expect(
      screen.queryByRole("button", { name: "partial.mkv" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("navigation", { name: "Folder path" }),
    ).toHaveTextContent("Media/Downloads");

    await user.click(
      screen.getByRole("button", { name: "Select this folder" }),
    );
    expect(onSelect).toHaveBeenCalledWith({
      root_id: "root-media",
      path: "Downloads",
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("supports breadcrumb, Back, and Escape navigation", async () => {
    const user = userEvent.setup();
    const api = browserApi();
    render(() => (
      <DirectoryPicker
        label="Destination folder"
        buttonLabel="Choose destination"
        onSelect={() => {}}
        api={api}
      />
    ));

    const trigger = screen.getByRole("button", { name: "Choose destination" });
    await user.click(trigger);
    await user.click(await screen.findByRole("button", { name: "Media" }));
    await user.click(screen.getByRole("button", { name: "Downloads" }));
    await user.click(screen.getByRole("button", { name: "Nested" }));
    await user.click(screen.getByRole("button", { name: "Downloads" }));
    expect(api.listLibrary).toHaveBeenLastCalledWith({
      rootId: "root-media",
      path: "Downloads",
    });

    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(api.listLibrary).toHaveBeenLastCalledWith({
      rootId: "root-media",
      path: "",
    });
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("shows loading and empty-root states", async () => {
    const user = userEvent.setup();
    let resolveRoots!: (value: { schema_version: 1; roots: [] }) => void;
    const roots = new Promise<{ schema_version: 1; roots: [] }>((resolve) => {
      resolveRoots = resolve;
    });
    const api = {
      libraryRoots: vi.fn(() => roots),
      listLibrary: vi.fn(),
    };
    render(() => (
      <DirectoryPicker
        label="Source folder"
        buttonLabel="Choose source"
        onSelect={() => {}}
        api={api}
      />
    ));

    await user.click(screen.getByRole("button", { name: "Choose source" }));
    expect(screen.getByRole("status")).toHaveTextContent("Loading folders");
    resolveRoots({ schema_version: 1, roots: [] });
    expect(
      await screen.findByRole("status", { name: "No folders available" }),
    ).toBeVisible();
  });

  it("offers retry after errors and reports an empty directory", async () => {
    const user = userEvent.setup();
    const api = {
      libraryRoots: vi
        .fn()
        .mockRejectedValueOnce(new Error("offline"))
        .mockResolvedValue({
          schema_version: 1 as const,
          roots: [{ id: "root-media", label: "Media" }],
        }),
      listLibrary: vi.fn(async () => ({
        schema_version: 1 as const,
        root_id: "root-media",
        path: "",
        entries: [],
        truncated: false,
      })),
    };
    render(() => (
      <DirectoryPicker
        label="Source folder"
        buttonLabel="Choose source"
        onSelect={() => {}}
        api={api}
      />
    ));

    await user.click(screen.getByRole("button", { name: "Choose source" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Folders could not be loaded",
    );
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await user.click(await screen.findByRole("button", { name: "Media" }));
    expect(
      await screen.findByRole("status", { name: "This folder is empty" }),
    ).toBeVisible();
  });
});
