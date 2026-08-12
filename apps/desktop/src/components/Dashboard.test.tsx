import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { StackSnapshot } from "../api/models";
import { useAppStore } from "../store/app";
import { Dashboard } from "./Dashboard";

vi.mock("../api/desktop", () => ({
  desktop: {
    start: vi.fn().mockResolvedValue({ operation_id: "op", already_complete: false }),
    stop: vi.fn().mockResolvedValue({ operation_id: "op", already_complete: false }),
    cancel: vi.fn().mockResolvedValue(true),
  },
}));

const now = new Date().toISOString();
const stopped: StackSnapshot = {
  revision: 1,
  phase: "stopped",
  operation_id: null,
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

describe("Dashboard", () => {
  it("shows real component state and starts without blocking the UI", async () => {
    useAppStore.setState({ snapshot: stopped, actionPending: false });
    render(<Dashboard snapshot={stopped} />);
    expect(screen.getAllByText("stopped")).toHaveLength(5);
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    expect(useAppStore.getState().actionPending).toBe(true);
  });

  it("exposes cancellation during an operation", () => {
    render(
      <Dashboard snapshot={{ ...stopped, phase: "starting_core", operation_id: "operation-1" }} />,
    );
    expect(screen.getByRole("button", { name: "Cancel operation" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("starting core");
  });
});
