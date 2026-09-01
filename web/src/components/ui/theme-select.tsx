import type { JSX } from "@solidjs/web";

import type { ThemePreference } from "../../lib/theme";

interface ThemeSelectProps {
  value: ThemePreference;
  onChange(preference: ThemePreference): void;
}

export function ThemeSelect(props: ThemeSelectProps): JSX.Element {
  return (
    <label class="flex items-center gap-2 text-sm text-muted">
      <span>Theme</span>
      <select
        aria-label="Theme"
        class="min-h-9 border border-line bg-surface px-2.5 py-1.5 text-sm text-ink outline-none transition-colors hover:border-moss focus-visible:border-coral disabled:cursor-not-allowed disabled:opacity-50"
        value={props.value}
        onChange={(event) => {
          const preference = event.currentTarget.value;
          if (
            preference === "system" ||
            preference === "light" ||
            preference === "dark"
          ) {
            props.onChange(preference);
          }
        }}
      >
        <option value="system">System</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
  );
}
