import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop } from "../api/desktop";
import { useAppStore } from "../store/app";
import { Diagnostics } from "./Diagnostics";

vi.mock("../api/desktop", () => ({
  desktop: {
    queryLogs: vi.fn().mockResolvedValue([]),
    debugLogStatus: vi.fn().mockResolvedValue({
      path: "/home/user/.local/share/biflow/debug.log",
      size_bytes: 48_512,
    }),
    revealDebugLog: vi.fn().mockResolvedValue({
      path: "/home/user/.local/share/biflow/debug.log",
      size_bytes: 48_512,
    }),
    deleteDebugLog: vi.fn().mockResolvedValue({
      path: "/home/user/.local/share/biflow/debug.log",
      size_bytes: 512,
    }),
    testRoute: vi.fn().mockResolvedValue({
      target: "openai.com",
      outbound: "vpn",
      reason: "default_proxy",
      matched_rule: "MATCH",
      reachable: true,
      tested_at: new Date().toISOString(),
    }),
    exportBundle: vi.fn(),
    listActiveConnections: vi.fn().mockResolvedValue([]),
    freshHiddifyStart: vi.fn().mockResolvedValue({
      data_dir: "/home/user/.local/share/hiddify",
      backup_dir: "/home/user/.local/share/biflow/backups/hiddify-20260815",
      cleared: ["configs", "data", "app.log"],
      preserved: ["db.sqlite", "shared_preferences.json"],
      stopped: true,
      started: true,
    }),
  },
}));

describe("Diagnostics", () => {
  // The module mock is shared by every test, so call history has to be dropped
  // or a "was not called" assertion sees the previous test's click.
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(desktop.listActiveConnections).mockResolvedValue([]);
    useAppStore.setState({ snapshot: null, actionPending: false });
  });

  it("tests whether a host is direct or vpn", async () => {
    render(<Diagnostics report={null} />);
    await userEvent.type(
      screen.getByLabelText("Test IP or domain"),
      "openai.com",
    );
    await userEvent.click(screen.getByRole("button", { name: "Test flow" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "openai.com → VPN",
    );
  });

  it("shows the permanent debug log location and size", async () => {
    render(<Diagnostics report={null} />);
    expect(await screen.findByTestId("debug-log-size")).toHaveTextContent(
      "47 KiB",
    );
    expect(
      screen.getByText("/home/user/.local/share/biflow/debug.log"),
    ).toBeVisible();
  });

  it("accepts a full URL and tests only its host", async () => {
    render(<Diagnostics report={null} />);
    await userEvent.type(
      screen.getByLabelText("Test IP or domain"),
      "https://www.rade.ir/some/path?a=1",
    );
    await userEvent.click(screen.getByRole("button", { name: "Test flow" }));

    const { desktop } = await import("../api/desktop");
    expect(desktop.testRoute).toHaveBeenCalledWith("www.rade.ir");
  });

  it("offers to move a VPN host to direct and re-tests it", async () => {
    const pinRoute = vi.fn().mockResolvedValue(undefined);
    const previous = useAppStore.getState().pinRoute;
    useAppStore.setState({ pinRoute });
    try {
      render(<Diagnostics report={null} />);
      await userEvent.type(
        screen.getByLabelText("Test IP or domain"),
        "openai.com",
      );
      await userEvent.click(screen.getByRole("button", { name: "Test flow" }));

      // openai.com resolves to VPN, so the offer is to pin it direct.
      const move = await screen.findByRole("button", {
        name: /Add openai\.com to direct/,
      });
      await userEvent.click(move);

      expect(pinRoute).toHaveBeenCalledWith("openai.com", "direct");
      const { desktop } = await import("../api/desktop");
      // The result is re-tested so the card reflects the new routing.
      expect(desktop.testRoute).toHaveBeenCalledTimes(2);
    } finally {
      useAppStore.setState({ pinRoute: previous });
    }
  });

  it("offers Add to VPN for a host the bundled Iran list keeps direct", async () => {
    const pinRoute = vi.fn().mockResolvedValue(undefined);
    const previous = useAppStore.getState().pinRoute;
    useAppStore.setState({ pinRoute });
    const { desktop } = await import("../api/desktop");
    vi.mocked(desktop.testRoute).mockResolvedValueOnce({
      target: "iran.ir",
      outbound: "direct",
      reason: "iran_domain",
      matched_rule: "ir",
      reachable: true,
      tested_at: new Date().toISOString(),
    });
    try {
      render(<Diagnostics report={null} />);
      await userEvent.type(
        screen.getByLabelText("Test IP or domain"),
        "iran.ir",
      );
      await userEvent.click(screen.getByRole("button", { name: "Test flow" }));

      const move = await screen.findByRole("button", {
        name: /Add iran\.ir to VPN/,
      });
      await userEvent.click(move);
      expect(pinRoute).toHaveBeenCalledWith("iran.ir", "vpn");
    } finally {
      useAppStore.setState({ pinRoute: previous });
    }
  });

  it("never offers to move a private or local address onto the VPN", async () => {
    const { desktop } = await import("../api/desktop");
    vi.mocked(desktop.testRoute).mockResolvedValueOnce({
      target: "192.168.1.1",
      outbound: "direct",
      reason: "private_or_local",
      matched_rule: "192.168.1.1",
      reachable: true,
      tested_at: new Date().toISOString(),
    });
    render(<Diagnostics report={null} />);
    await userEvent.type(
      screen.getByLabelText("Test IP or domain"),
      "192.168.1.1",
    );
    await userEvent.click(screen.getByRole("button", { name: "Test flow" }));
    await screen.findByText(/192\.168\.1\.1 → DIRECT/);

    expect(screen.queryByRole("button", { name: /to VPN/ })).toBeNull();
    expect(screen.getByText(/always stay direct/)).toBeVisible();
  });

  it("offers Add to VPN when a custom rule is what made it direct", async () => {
    const { desktop } = await import("../api/desktop");
    vi.mocked(desktop.testRoute).mockResolvedValueOnce({
      target: "example.ir",
      outbound: "direct",
      reason: "custom_rule",
      matched_rule: "example.ir",
      reachable: true,
      tested_at: new Date().toISOString(),
    });
    render(<Diagnostics report={null} />);
    await userEvent.type(
      screen.getByLabelText("Test IP or domain"),
      "example.ir",
    );
    await userEvent.click(screen.getByRole("button", { name: "Test flow" }));

    expect(
      await screen.findByRole("button", { name: /Add example\.ir to VPN/ }),
    ).toBeVisible();
  });

  it("restarts Hiddify on clean state and reports the backup", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<Diagnostics report={null} />);
    await userEvent.click(
      screen.getByRole("button", { name: /Fresh Hiddify start/ }),
    );

    expect(confirm).toHaveBeenCalledOnce();
    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent(
      "Hiddify restarted on clean runtime state",
    );
    expect(status).toHaveTextContent("configs, data, app.log");
    expect(status).toHaveTextContent("db.sqlite, shared_preferences.json");
    expect(status).toHaveTextContent(
      "/home/user/.local/share/biflow/backups/hiddify-20260815",
    );
  });

  it("does not touch Hiddify when the confirmation is declined", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<Diagnostics report={null} />);
    await userEvent.click(
      screen.getByRole("button", { name: /Fresh Hiddify start/ }),
    );

    expect(confirm).toHaveBeenCalledOnce();
    const { desktop } = await import("../api/desktop");
    expect(desktop.freshHiddifyStart).not.toHaveBeenCalled();
  });

  it("requires confirmation before deleting the log", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<Diagnostics report={null} />);
    await screen.findByTestId("debug-log-size");
    await userEvent.click(screen.getByRole("button", { name: "Delete log" }));
    expect(confirm).toHaveBeenCalledOnce();
    const { desktop } = await import("../api/desktop");
    expect(desktop.deleteDebugLog).not.toHaveBeenCalled();
  });

  it("shows live DIRECT and VPN connections while the stack is running", async () => {
    vi.mocked(desktop.listActiveConnections).mockResolvedValue([
      {
        host: "digikala.ir",
        destination_ip: "5.22.12.1",
        outbound: "direct",
        rule: "iran-domains",
      },
      {
        host: "openai.com",
        destination_ip: "104.18.1.1",
        outbound: "vpn",
        rule: "MATCH",
      },
    ]);
    useAppStore.setState({
      snapshot: {
        revision: 1,
        phase: "running",
        operation_id: null,
        helper: { phase: "running", message: null, since: "now" },
        hiddify: { phase: "running", message: null, since: "now" },
        mihomo: { phase: "running", message: null, since: "now" },
        tun: { phase: "running", message: null, since: "now" },
        dns: { phase: "running", message: null, since: "now" },
        providers: {
          ready: 6,
          total: 6,
          rules_loaded: 12,
          last_refresh: null,
        },
        exit_ip: "203.0.113.42",
        backend: "external_hiddify",
        last_error: null,
        updated_at: "now",
      },
    });
    render(<Diagnostics report={null} />);
    expect(
      await screen.findByRole("heading", { name: "Live connections" }),
    ).toBeVisible();
    expect(await screen.findByText("digikala.ir")).toBeVisible();
    expect(screen.getByText("openai.com")).toBeVisible();
    // Route badges; the actions column also renders DIRECT/VPN as the
    // switch-route button label, so scope to the badge spans.
    const direct = screen
      .getAllByRole("cell", { name: "DIRECT" })
      .filter((cell) => cell.querySelector("span"));
    const vpn = screen
      .getAllByRole("cell", { name: "VPN" })
      .filter((cell) => cell.querySelector("span"));
    expect(direct).toHaveLength(1);
    expect(vpn).toHaveLength(1);
    // Each row offers a button that moves the host to the opposite route.
    expect(screen.getByTitle("Add digikala.ir to VPN")).toBeVisible();
    expect(screen.getByTitle("Add openai.com to direct")).toBeVisible();
  });
});
