import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { StackSnapshot } from "../api/models";
import { BasicDashboard } from "./BasicDashboard";

const now = new Date().toISOString();
const stopped: StackSnapshot = {
  revision: 1,
  phase: "stopped",
  busy: null,
  operation_stage: null,
  operation_id: null,
  helper: { phase: "running", message: null, since: now },
  hiddify: { phase: "stopped", message: null, since: now },
  openvpn: { phase: "stopped", message: null, since: now },
  mihomo: { phase: "stopped", message: null, since: now },
  tun: { phase: "stopped", message: null, since: now },
  dns: { phase: "stopped", message: null, since: now },
  providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
  exit_ip: null,
  backend: "external_hiddify",
  last_error: null,
  updated_at: now,
};

describe("BasicDashboard", () => {
  it("puts Connect progress on the button instead of a status card", () => {
    render(
      <BasicDashboard
        snapshot={{
          ...stopped,
          phase: "starting_hiddify",
          busy: "connecting",
          operation_stage: "starting_hiddify",
          operation_id: "op-1",
        }}
      />,
    );
    const connect = screen.getByRole("button", { name: "Start Hiddify" });
    expect(connect).toBeDisabled();
    expect(connect).toHaveAttribute("data-progress", "25");
    expect(connect).toHaveAttribute("data-connect-glow", "off");
    expect(screen.queryByText("%")).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  });
});
