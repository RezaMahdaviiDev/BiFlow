import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { InputContextMenu } from "./InputContextMenu";

describe("InputContextMenu", () => {
  beforeEach(() => {
    Object.assign(navigator, {
      clipboard: {
        writeText: vi.fn(async () => undefined),
        readText: vi.fn(async () => "pasted"),
      },
    });
  });

  it("offers select all, copy, cut, and paste on a text field", async () => {
    render(
      <div>
        <input aria-label="Host" defaultValue="example.ir" />
        <InputContextMenu />
      </div>,
    );
    const field = screen.getByLabelText("Host");
    if (!(field instanceof HTMLInputElement)) {
      throw new Error("host field is missing");
    }
    field.focus();
    field.setSelectionRange(0, 0);
    fireEvent.contextMenu(field, { clientX: 12, clientY: 20 });
    const menu = screen.getByTestId("input-context-menu");
    expect(menu).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "Copy" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Cut" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Paste" })).toBeEnabled();
    await userEvent.click(screen.getByRole("menuitem", { name: "Select All" }));
    expect(field).toHaveProperty("selectionStart", 0);
    expect(field).toHaveProperty("selectionEnd", "example.ir".length);
  });

  it("pastes clipboard text into a controlled React field", async () => {
    function ControlledHost() {
      const [value, setValue] = useState("");
      return (
        <input
          aria-label="Host"
          value={value}
          onChange={(event) => setValue(event.target.value)}
        />
      );
    }
    render(
      <div>
        <ControlledHost />
        <InputContextMenu />
      </div>,
    );
    const field = screen.getByLabelText("Host");
    fireEvent.contextMenu(field);
    await userEvent.click(screen.getByRole("menuitem", { name: "Paste" }));
    expect(field).toHaveValue("pasted");
  });
});
