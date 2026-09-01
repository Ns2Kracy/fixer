import { describe, expect, it, vi } from "vitest";

import {
  applyTheme,
  createThemeController,
  readThemePreference,
  resolveTheme,
  type ThemeEnvironment,
  type ThemeState,
} from "./theme";

function createEnvironment(options?: {
  stored?: string | null;
  systemDark?: boolean;
}) {
  let stored = options?.stored ?? null;
  let systemDark = options?.systemDark ?? false;
  let mediaListener: ((event: MediaQueryListEvent) => void) | undefined;
  const root = document.createElement("html");
  const themeColor = document.createElement("meta");
  const environment: ThemeEnvironment = {
    storage: {
      getItem: vi.fn(() => stored),
      setItem: vi.fn((_key, value) => {
        stored = value;
      }),
    },
    mediaQuery: {
      get matches() {
        return systemDark;
      },
      addEventListener: vi.fn((_type, listener) => {
        mediaListener = listener as (event: MediaQueryListEvent) => void;
      }),
      removeEventListener: vi.fn(),
    },
    root,
    themeColor,
  };

  return {
    environment,
    setSystemDark(value: boolean) {
      systemDark = value;
      mediaListener?.({ matches: value } as MediaQueryListEvent);
    },
  };
}

describe("theme preferences", () => {
  it("accepts only supported stored values", () => {
    expect(readThemePreference({ getItem: () => "dark" })).toBe("dark");
    expect(readThemePreference({ getItem: () => "sepia" })).toBe("system");
    expect(
      readThemePreference({
        getItem() {
          throw new DOMException("Storage unavailable");
        },
      }),
    ).toBe("system");
  });

  it("resolves explicit preferences before the system preference", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });

  it("applies the resolved theme to the document and browser chrome", () => {
    const root = document.createElement("html");
    const themeColor = document.createElement("meta");

    applyTheme("dark", root, themeColor);

    expect(root.dataset.theme).toBe("dark");
    expect(root.style.colorScheme).toBe("dark");
    expect(themeColor).toHaveAttribute("content", "#171a16");
  });

  it("persists explicit choices and follows system changes only in system mode", () => {
    const { environment, setSystemDark } = createEnvironment({ systemDark: false });
    const states: ThemeState[] = [];
    const controller = createThemeController((state) => states.push(state), environment);

    expect(states.at(-1)).toEqual({ preference: "system", resolved: "light" });

    setSystemDark(true);
    expect(states.at(-1)).toEqual({ preference: "system", resolved: "dark" });

    controller.setPreference("light");
    expect(environment.storage.setItem).toHaveBeenCalledWith("fixer-theme", "light");
    expect(states.at(-1)).toEqual({ preference: "light", resolved: "light" });

    setSystemDark(false);
    expect(states.at(-1)).toEqual({ preference: "light", resolved: "light" });

    controller.dispose();
    expect(environment.mediaQuery.removeEventListener).toHaveBeenCalledOnce();
  });
});
