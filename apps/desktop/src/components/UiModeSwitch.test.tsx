import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readUiMode, UI_MODE_STORAGE_KEY, writeUiMode } from "../lib/uiMode";
import { UiModeSwitch } from "./UiModeSwitch";

describe("UiModeSwitch", () => {
  beforeEach(() => {
    localStorage.removeItem(UI_MODE_STORAGE_KEY);
  });

  it("defaults a first launch to Basic when no preference is stored", () => {
    expect(readUiMode()).toBe("basic");
  });

  it("persists Basic and Advanced selections", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<UiModeSwitch mode="advanced" onChange={onChange} />);

    await user.click(screen.getByRole("radio", { name: "Basic" }));
    expect(onChange).toHaveBeenCalledWith("basic");
    writeUiMode("basic");
    expect(localStorage.getItem(UI_MODE_STORAGE_KEY)).toBe("basic");
  });

  it("supports keyboard navigation between modes", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<UiModeSwitch mode="basic" onChange={onChange} />);
    screen.getByRole("radio", { name: "Basic" }).focus();

    await user.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenCalledWith("advanced");
  });

  it("disables the sliding-pill transition when motion is reduced", () => {
    render(<UiModeSwitch mode="advanced" onChange={vi.fn()} />);
    const pill = document.querySelector('[aria-hidden="true"]');
    expect(pill?.className).toMatch(/motion-reduce:transition-none/);
  });
});
