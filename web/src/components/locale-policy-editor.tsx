import type { JSX } from "@solidjs/web";

interface LocalePolicyEditorProps {
  value: string[];
  disabled?: boolean;
  onChange: (locales: string[]) => void;
}

export function LocalePolicyEditor(
  props: LocalePolicyEditorProps,
): JSX.Element {
  function update(value: string) {
    const seen = new Set<string>();
    props.onChange(
      value
        .split(",")
        .map((locale) => locale.trim())
        .filter((locale) => {
          if (locale.length === 0 || seen.has(locale)) return false;
          seen.add(locale);
          return true;
        }),
    );
  }

  return (
    <label class="grid gap-2 text-sm font-medium text-muted">
      <span>Preferred locales</span>
      <input
        class="min-h-11 border border-line bg-surface px-3 py-2.5 text-ink outline-none transition-colors focus-visible:border-coral"
        type="text"
        value={props.value.join(", ")}
        disabled={props.disabled}
        placeholder="zh-Hans, ja, en, und"
        aria-describedby="locale-policy-help"
        onInput={(event) => {
          update(event.currentTarget.value);
        }}
      />
      <small
        class="font-normal leading-relaxed text-muted"
        id="locale-policy-help"
      >
        Ordered BCP 47 tags, separated by commas. Earlier locales win when
        metadata overlaps.
      </small>
    </label>
  );
}
