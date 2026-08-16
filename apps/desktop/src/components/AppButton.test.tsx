import { render, screen } from "@testing-library/react";
import { Power } from "lucide-react";
import { describe, expect, it } from "vitest";
import { AppButton, BUTTON_ICON_PX, IconOnlyButton } from "./AppButton";

describe("AppButton", () => {
  it("renders a consistent icon beside the label", () => {
    render(
      <AppButton icon={<Power data-testid="button-icon" aria-hidden />}>
        Connect
      </AppButton>,
    );
    expect(screen.getByRole("button", { name: "Connect" })).toBeEnabled();
    expect(screen.getByTestId("button-icon")).toBeInTheDocument();
    expect(BUTTON_ICON_PX).toBe(18);
  });

  it("gives icon-only buttons a tooltip and accessible name", () => {
    render(
      <IconOnlyButton label="Use light theme">
        <span aria-hidden>sun</span>
      </IconOnlyButton>,
    );
    const button = screen.getByRole("button", { name: "Use light theme" });
    expect(button).toHaveAttribute("title", "Use light theme");
  });
});
