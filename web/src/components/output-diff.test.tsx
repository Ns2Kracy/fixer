import { describe, expect, it } from "vitest";

import { render, screen } from "../test/render";
import { OutputDiff } from "./output-diff";

describe("OutputDiff", () => {
  it("exposes filesystem operations as a named region", () => {
    render(() => (
      <OutputDiff
        outputRoot="/media/movie"
        operations={[
          {
            index: 0,
            kind: "write",
            source: null,
            target: "movie.json",
            content_bytes: 128,
          },
        ]}
      />
    ));

    const region = screen.getByRole("region", {
      name: "Filesystem operations",
    });
    expect(region).toHaveTextContent("/media/movie");
    expect(region).toHaveTextContent("Write metadata");
    expect(region).toHaveTextContent("movie.json");
    expect(region).toHaveTextContent("128 bytes prepared");
  });

  it("labels destructive media moves explicitly", () => {
    render(() => (
      <OutputDiff
        outputRoot="/media/library"
        operations={[
          {
            index: 0,
            kind: "move",
            source: "/media/incoming/movie.mkv",
            target: "Movie/movie.mkv",
          },
        ]}
      />
    ));

    expect(screen.getByText("Move media")).toBeInTheDocument();
    expect(
      screen.getByText("/media/incoming/movie.mkv → Movie/movie.mkv"),
    ).toBeInTheDocument();
  });
});
