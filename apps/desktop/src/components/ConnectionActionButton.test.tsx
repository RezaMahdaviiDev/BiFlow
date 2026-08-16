import { render, screen } from "@testing-library/react";
import { Power } from "lucide-react";
import { describe, expect, it } from "vitest";
import type { StackSnapshot } from "../api/models";
import { BUTTON_ICON_PX } from "./AppButton";
import { ConnectionActionButton } from "./ConnectionActionButton";

const now = new Date().toISOString();
const stopped: StackSnapshot = {
  revision: 1,
  phase: "stopped",
  busy: null,
  operation_stage: null,
  operation_id: null,
  helper: { phase: "running", message: null, since: now },
  hiddify: { phase: "stopped", message: null, since: now },
  mihomo: { phase: "stopped", message: null, since: now },
  tun: { phase: "stopped", message: null, since: now },
  dns: { phase: "stopped", message: null, since: now },
  providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
  exit_ip: null,
  backend: "external_hiddify",
  last_error: null,
  updated_at: now,
};

describe("ConnectionActionButton", () => {
  it("renders the idle label and an empty fill", () => {
    render(
      <ConnectionActionButton
        action="connect"
        snapshot={stopped}
        disabled={false}
        onClick={() => undefined}
        icon={<Power size={BUTTON_ICON_PX} aria-hidden />}
        variant="primary"
      />,
    );
    const button = screen.getByRole("button", { name: "Connect" });
    expect(button).toHaveAttribute("data-progress", "0");
    expect(button).toHaveAttribute("data-processing", "false");
    expect(button).toHaveAttribute("data-connect-glow", "available");
    expect(button.className).toMatch(/connect-button-glow/);
    expect(button.querySelector(".connection-action-label")?.className).toMatch(
      /break-words/,
    );
    expect(
      button.querySelector(".connection-action-label")?.className,
    ).not.toMatch(/truncate|whitespace-nowrap/);
  });

  it("shows the current stage and fill while processing", () => {
    render(
      <ConnectionActionButton
        action="connect"
        snapshot={{
          ...stopped,
          phase: "starting_core",
          busy: "connecting",
          operation_stage: "starting_core",
          operation_id: "op-1",
        }}
        disabled
        onClick={() => undefined}
        icon={<Power size={BUTTON_ICON_PX} aria-hidden />}
        variant="primary"
      />,
    );
    const button = screen.getByRole("button", { name: "Start Mihomo" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("data-progress", "70");
    expect(button).toHaveAttribute("aria-busy", "true");
    expect(button).toHaveAttribute("data-connect-glow", "off");
    expect(button.className).toMatch(/connection-action-processing/);
    expect(button.className).not.toMatch(/connect-button-glow/);
    const fill = button.querySelector(".connection-action-fill");
    expect(fill).toHaveStyle({ width: "70%" });
  });
});
