import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AppConfig } from "../api/models";
import { Settings } from "./Settings";

vi.mock("../api/desktop", () => ({
  desktop: {
    validateSettings: vi.fn().mockResolvedValue([]),
  },
}));

const settings: AppConfig = {
  schema_version: 1,
  revision: 0,
  hiddify: { host: "127.0.0.1", port: 12334, executable: "auto", start_timeout_seconds: 45, stop_with_stack: true },
  mihomo: { controller_host: "127.0.0.1", controller_port: 19090, controller_secret: "redacted", mixed_port: 17890, dns_port: 1053, tun_name: "clash-iran", log_level: "info" },
  rules: { refresh_interval_minutes: 15, upstream_refresh_hours: 24 },
  behavior: { launch_at_login: false, connect_at_launch: false, close_to_tray: true },
};

describe("Settings", () => {
  it("rejects a remote Hiddify address before persistence", async () => {
    render(<Settings settings={settings} />);
    const host = screen.getByLabelText("Host");
    await userEvent.clear(host);
    await userEvent.type(host, "0.0.0.0");
    await userEvent.click(screen.getByRole("button", { name: "Save settings" }));
    expect(await screen.findByText(/Invalid literal value/)).toBeVisible();
  });
});
