import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppConfig } from "../api/models";
import { useAppStore } from "../store/app";
import { Settings } from "./Settings";

vi.mock("../api/desktop", () => ({
  desktop: {
    validateSettings: vi.fn().mockResolvedValue([]),
  },
}));

const settings: AppConfig = {
  schema_version: 1,
  revision: 0,
  hiddify: {
    host: "127.0.0.1",
    port: 12334,
    executable: "auto",
    start_timeout_seconds: 45,
    stop_with_stack: true,
  },
  mihomo: {
    controller_host: "127.0.0.1",
    controller_port: 19090,
    controller_secret: "redacted",
    mixed_port: 17890,
    dns_port: 1053,
    tun_name: "clash-iran",
    log_level: "info",
    direct_dns_preset: "fake_ip",
    direct_dns_servers: [],
  },
  rules: { refresh_interval_minutes: 15, upstream_refresh_hours: 24 },
  behavior: {
    launch_at_login: false,
    connect_at_launch: false,
    close_to_tray: true,
  },
  openvpn: {
    enabled: false,
    required: false,
    pull_routes: true,
    device: "biflow-ovpn",
    start_timeout_seconds: 45,
    routing_mark: 45552,
    routing_table: 178,
    profile: null,
    auth_file: null,
    executable: null,
    tunnel_routes: [],
  },
};

describe("Settings", () => {
  beforeEach(() => {
    useAppStore.setState({ actionPending: false });
  });

  it("rejects a remote Hiddify address before persistence", async () => {
    render(<Settings settings={settings} />);
    const host = screen.getByLabelText("Host");
    await userEvent.clear(host);
    await userEvent.type(host, "0.0.0.0");
    await userEvent.click(
      screen.getByRole("button", { name: "Save settings" }),
    );
    expect(await screen.findByText(/Invalid literal value/)).toBeVisible();
  });

  it("lets the operator pick a DIRECT DNS preset including Mokhaberat", async () => {
    const saveSettings = vi
      .fn<(draft: AppConfig) => Promise<void>>()
      .mockResolvedValue(undefined);
    useAppStore.setState({ saveSettings });
    render(<Settings settings={settings} />);
    await userEvent.click(screen.getByRole("tab", { name: "Mihomo" }));
    const dns = screen.getByLabelText("DIRECT DNS");
    expect(dns).toHaveValue("fake_ip");
    expect(screen.getByRole("option", { name: "Fake-ip" })).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: /Mokhaberat \(5\.200\.200\.200\)/ }),
    ).toBeInTheDocument();
    await userEvent.selectOptions(dns, "custom");
    expect(screen.getByLabelText("Custom resolvers")).toBeVisible();
    await userEvent.selectOptions(dns, "mokhaberat");
    expect(screen.queryByLabelText("Custom resolvers")).not.toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Save settings" }),
    );
    expect(saveSettings.mock.calls[0]?.[0].mihomo.direct_dns_preset).toBe(
      "mokhaberat",
    );
  });

  it("refuses a default route for the OpenVPN side tunnel", async () => {
    const saveSettings = vi
      .fn<(draft: AppConfig) => Promise<void>>()
      .mockResolvedValue(undefined);
    useAppStore.setState({ saveSettings });
    render(<Settings settings={settings} />);
    await userEvent.click(screen.getByRole("tab", { name: "OpenVPN" }));
    await userEvent.type(
      screen.getByLabelText("Extra networks through the tunnel"),
      "0.0.0.0/0",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Save settings" }),
    );
    expect(
      await screen.findByText(/whole system through OpenVPN/),
    ).toBeVisible();
    expect(saveSettings).not.toHaveBeenCalled();
  });

  it("saves an enabled side tunnel with its profile and scoped routes", async () => {
    const saveSettings = vi
      .fn<(draft: AppConfig) => Promise<void>>()
      .mockResolvedValue(undefined);
    useAppStore.setState({ saveSettings });
    render(<Settings settings={settings} />);
    await userEvent.click(screen.getByRole("tab", { name: "OpenVPN" }));
    await userEvent.click(screen.getByLabelText("Start OpenVPN with Connect"));
    await userEvent.type(
      screen.getByLabelText(".ovpn profile"),
      "/etc/openvpn/office.ovpn",
    );
    await userEvent.type(
      screen.getByLabelText("Extra networks through the tunnel"),
      "10.8.0.0/24, 192.168.44.0/24",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Save settings" }),
    );
    const draft = saveSettings.mock.calls[0]?.[0];
    expect(draft?.openvpn.enabled).toBe(true);
    expect(draft?.openvpn.profile).toBe("/etc/openvpn/office.ovpn");
    expect(draft?.openvpn.tunnel_routes).toEqual([
      "10.8.0.0/24",
      "192.168.44.0/24",
    ]);
    // Never on by default: a broken profile must not cost the user Connect.
    expect(draft?.openvpn.required).toBe(false);
  });

  it("requires a profile before the side tunnel can be enabled", async () => {
    const saveSettings = vi
      .fn<(draft: AppConfig) => Promise<void>>()
      .mockResolvedValue(undefined);
    useAppStore.setState({ saveSettings });
    render(<Settings settings={settings} />);
    await userEvent.click(screen.getByRole("tab", { name: "OpenVPN" }));
    await userEvent.click(screen.getByLabelText("Start OpenVPN with Connect"));
    await userEvent.click(
      screen.getByRole("button", { name: "Save settings" }),
    );
    expect(await screen.findByText(/Choose a .ovpn profile/)).toBeVisible();
    expect(saveSettings).not.toHaveBeenCalled();
  });
});
