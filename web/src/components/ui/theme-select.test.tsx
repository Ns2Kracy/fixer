import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { render, screen } from "../../test/render";
import { ThemeSelect } from "./theme-select";

describe("ThemeSelect", () => {
  it("offers system, light, and dark preferences", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(() => <ThemeSelect value="system" onChange={onChange} />);

    const select = screen.getByRole("combobox", { name: "Theme" });
    expect(select).toHaveValue("system");
    expect(
      screen.getAllByRole("option").map((option) => option.textContent),
    ).toEqual(["System", "Light", "Dark"]);

    await user.selectOptions(select, "dark");
    expect(onChange).toHaveBeenCalledWith("dark");
  });
});
