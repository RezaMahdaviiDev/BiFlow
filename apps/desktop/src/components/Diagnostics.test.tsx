import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Diagnostics } from "./Diagnostics";

vi.mock("../api/desktop", () => ({
  desktop: {
    queryLogs: vi.fn().mockResolvedValue([]),
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
});
