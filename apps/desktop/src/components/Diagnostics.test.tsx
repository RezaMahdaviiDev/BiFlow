import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
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
  },
}));

describe("Diagnostics", () => {
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
