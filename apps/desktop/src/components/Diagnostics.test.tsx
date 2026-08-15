import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
    const addRule = vi.fn().mockResolvedValue(undefined);
    const previous = useAppStore.getState().addRule;
    useAppStore.setState({ addRule });
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

      expect(addRule).toHaveBeenCalledWith("openai.com");
      const { desktop } = await import("../api/desktop");
      // The result is re-tested so the card reflects the new routing.
      expect(desktop.testRoute).toHaveBeenCalledTimes(2);
    } finally {
      useAppStore.setState({ addRule: previous });
    }
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
});
