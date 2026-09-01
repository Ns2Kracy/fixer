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
});
