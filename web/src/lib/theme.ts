export const THEME_STORAGE_KEY = "fixer-theme";
export const LIGHT_THEME_COLOR = "#f3f0e8";
export const DARK_THEME_COLOR = "#171a16";

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = Exclude<ThemePreference, "system">;

export interface ThemeState {
  preference: ThemePreference;
  resolved: ResolvedTheme;
}

interface ThemeStorageReader {
  getItem(key: string): string | null;
}

interface ThemeStorage extends ThemeStorageReader {
  setItem(key: string, value: string): void;
}

interface ThemeMediaQuery {
  readonly matches: boolean;
  addEventListener(
    type: "change",
    listener: (event: MediaQueryListEvent) => void,
  ): void;
  removeEventListener(
    type: "change",
    listener: (event: MediaQueryListEvent) => void,
  ): void;
}

export interface ThemeEnvironment {
  storage: ThemeStorage;
  mediaQuery: ThemeMediaQuery;
  root: HTMLElement;
  themeColor: HTMLMetaElement | null;
}

export interface ThemeController {
  setPreference(preference: ThemePreference): void;
  dispose(): void;
}

export function readThemePreference(
  storage: ThemeStorageReader,
): ThemePreference {
  try {
    const value = storage.getItem(THEME_STORAGE_KEY);
    return value === "light" || value === "dark" || value === "system"
      ? value
      : "system";
  } catch {
    return "system";
  }
}

export function resolveTheme(
  preference: ThemePreference,
  systemDark: boolean,
): ResolvedTheme {
  if (preference !== "system") return preference;
  return systemDark ? "dark" : "light";
}

export function applyTheme(
  theme: ResolvedTheme,
  root: HTMLElement,
  themeColor: HTMLMetaElement | null,
): void {
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
  themeColor?.setAttribute(
    "content",
    theme === "dark" ? DARK_THEME_COLOR : LIGHT_THEME_COLOR,
  );
}

export function browserThemeEnvironment(): ThemeEnvironment {
  return {
    storage: {
      getItem: (key) => window.localStorage.getItem(key),
      setItem: (key, value) => window.localStorage.setItem(key, value),
    },
    mediaQuery: window.matchMedia("(prefers-color-scheme: dark)"),
    root: document.documentElement,
    themeColor: document.querySelector<HTMLMetaElement>('meta[name="theme-color"]'),
  };
}

export function createThemeController(
  onChange: (state: ThemeState) => void,
  environment: ThemeEnvironment = browserThemeEnvironment(),
): ThemeController {
  let preference = readThemePreference(environment.storage);

  const sync = () => {
    const resolved = resolveTheme(preference, environment.mediaQuery.matches);
    applyTheme(resolved, environment.root, environment.themeColor);
    onChange({ preference, resolved });
  };
  const handleSystemChange = () => {
    if (preference === "system") sync();
  };

  environment.mediaQuery.addEventListener("change", handleSystemChange);
  sync();

  return {
    setPreference(nextPreference) {
      preference = nextPreference;
      try {
        environment.storage.setItem(THEME_STORAGE_KEY, nextPreference);
      } catch {
        // The theme still applies when storage is unavailable.
      }
      sync();
    },
    dispose() {
      environment.mediaQuery.removeEventListener("change", handleSystemChange);
    },
  };
}
