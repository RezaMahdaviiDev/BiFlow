import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { DirectRulesDocument } from "../api/models";
import { useAppStore } from "../store/app";
import { DirectRules } from "./DirectRules";

vi.mock("../api/desktop", () => ({
  desktop: {
    testRoute: vi.fn().mockResolvedValue({
      target: "example.ir",
      outbound: "direct",
      reason: "custom_rule",
      matched_rule: "example.ir",
      reachable: true,
      tested_at: new Date().toISOString(),
    }),
  },
}));

const rules: DirectRulesDocument = {
  revision: 1,
  rules: [
    {
      target: { kind: "domain", value: "example.ir" },
      resolved_ips: ["203.0.113.8"],
      created_at: new Date().toISOString(),
      refreshed_at: new Date().toISOString(),
    },
  ],
  vpn_rules: [
    {
      target: { kind: "domain", value: "pinned.ir" },
      resolved_ips: ["203.0.113.20"],
      created_at: new Date().toISOString(),
      refreshed_at: new Date().toISOString(),
    },
  ],
  openvpn_rules: [
    {
      target: { kind: "ip", value: "10.20.5.5" },
      resolved_ips: ["10.20.5.5"],
      created_at: new Date().toISOString(),
      refreshed_at: new Date().toISOString(),
    },
  ],
};

describe("DirectRules", () => {
  it("shows cloud domain and IP counts and last sync", () => {
    useAppStore.setState({
      rules,
      actionPending: false,
      cloudRules: {
        domain_count: 62829,
        ip_count: 2899,
        last_synced_at: "2026-08-12T12:00:00.000Z",
        source: "devlifeX/BiFlow",
        snapshot_revision: "767ef8bf5673",
        sets: [],
      },
    });
    render(<DirectRules rules={rules} />);
    expect(screen.getByText(/62[,\u00a0\s]?829/)).toBeVisible();
    expect(screen.getByText(/2[,\u00a0\s]?899/)).toBeVisible();
    expect(
      screen.getByRole("button", { name: /update from cloud/i }),
    ).toBeEnabled();
  });

  it("adds a custom direct rule without leaving the page", async () => {
    const addRule = vi.fn().mockResolvedValue(undefined);
    useAppStore.setState({
      rules,
      actionPending: false,
      cloudRules: {
        domain_count: 1,
        ip_count: 1,
        last_synced_at: null,
        source: "bundled",
        snapshot_revision: null,
        sets: [],
      },
      addRule,
    });
    render(<DirectRules rules={rules} />);
    await userEvent.type(screen.getByLabelText("Domain or IP"), "aparat.com");
    await userEvent.click(screen.getByRole("button", { name: /^Add rule$/ }));
    expect(addRule).toHaveBeenCalledWith("aparat.com");
  });

  it("keeps the input when adding a rule fails", async () => {
    const addRule = vi.fn().mockRejectedValue(new Error("rules changed"));
    useAppStore.setState({
      rules,
      actionPending: false,
      cloudRules: {
        domain_count: 1,
        ip_count: 1,
        last_synced_at: null,
        source: "bundled",
        snapshot_revision: null,
        sets: [],
      },
      addRule,
    });
    render(<DirectRules rules={rules} />);
    await userEvent.type(screen.getByLabelText("Domain or IP"), "aparat.com");
    await userEvent.click(screen.getByRole("button", { name: /^Add rule$/ }));
    expect(addRule).toHaveBeenCalledWith("aparat.com");
    expect(screen.getByLabelText("Domain or IP")).toHaveValue("aparat.com");
  });

  it("lists side-tunnel pins and can move a host onto any of the three routes", async () => {
    const pinRoute = vi.fn().mockResolvedValue(undefined);
    useAppStore.setState({
      rules,
      actionPending: false,
      cloudRules: {
        domain_count: 1,
        ip_count: 1,
        last_synced_at: null,
        source: "bundled",
        snapshot_revision: null,
        sets: [],
      },
      pinRoute,
    });
    render(<DirectRules rules={rules} />);

    // The private address pinned to the side tunnel is a legitimate rule, and
    // it has to be visible beside the DIRECT and VPN pins.
    const sideRoute = screen.getByLabelText("Route for 10.20.5.5");
    expect(sideRoute).toHaveValue("openvpn");

    await userEvent.selectOptions(
      screen.getByLabelText("Route for example.ir"),
      "openvpn",
    );
    expect(pinRoute).toHaveBeenCalledWith("example.ir", "openvpn");
  });
});
