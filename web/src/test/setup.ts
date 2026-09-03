import "@testing-library/jest-dom/vitest";

import { afterEach, vi } from "vitest";

import { cleanup } from "./render";

const localStorageValues = new Map<string, string>();

Object.defineProperty(window, "localStorage", {
  value: {
    get length() {
      return localStorageValues.size;
    },
    clear() {
      localStorageValues.clear();
    },
    getItem(key: string) {
      return localStorageValues.get(key) ?? null;
    },
    key(index: number) {
      return [...localStorageValues.keys()][index] ?? null;
    },
    removeItem(key: string) {
      localStorageValues.delete(key);
    },
    setItem(key: string, value: string) {
      localStorageValues.set(key, value);
    },
  } satisfies Storage,
  configurable: true,
});
Object.defineProperty(window, "scrollTo", { value: vi.fn(), writable: true });
Object.defineProperty(window, "matchMedia", {
  value: vi.fn((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
  writable: true,
});

afterEach(() => {
  cleanup();
  window.localStorage.clear();
  vi.unstubAllGlobals();
});
