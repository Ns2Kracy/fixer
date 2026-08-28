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
        .filter(
          (locale) =>
            locale.length > 0 && !seen.has(locale) && seen.add(locale),
        ),
    );
  }

  return (
    <label class="locale-policy-editor">
      <span>Preferred locales</span>
      <input
        type="text"
        value={props.value.join(", ")}
        disabled={props.disabled}
        placeholder="zh-Hans, ja, en, und"
        aria-describedby="locale-policy-help"
        onInput={(event) => update(event.currentTarget.value)}
      />
      <small id="locale-policy-help">
        Ordered BCP 47 tags, separated by commas. Earlier locales win when
        metadata overlaps.
      </small>
    </label>
  );
}
