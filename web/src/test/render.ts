import { getQueriesForElement, screen, waitFor } from "@testing-library/dom";
import { render as solidRender } from "@solidjs/web";
import type { JSX } from "@solidjs/web";

export { screen, waitFor };

const disposers = new Set<() => void>();

export function render(ui: () => JSX.Element) {
  const container = document.createElement("div");
  document.body.append(container);
  const dispose = solidRender(ui, container);
  disposers.add(dispose);

  return {
    container,
    unmount() {
      dispose();
      disposers.delete(dispose);
      container.remove();
    },
    ...getQueriesForElement(container),
  };
}

export function cleanup() {
  for (const dispose of disposers) dispose();
  disposers.clear();
  document.body.replaceChildren();
}
