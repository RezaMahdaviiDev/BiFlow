import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useAppStore } from "../store/app";
import { AppStatusBar } from "./AppStatusBar";
import { countryFlag } from "./country";

describe("AppStatusBar", () => {
  it("shows internet, public IP, location, and country flag", () => {
    useAppStore.setState({
      networkStatus: {
        state: "online",
        public_ip: "203.0.113.8",
        country_code: "IR",
        city: "Tehran",
        checked_at: "2026-08-13T00:00:00.000Z",
        detail: "Internet is reachable",
      },
    });

    render(<AppStatusBar />);

    expect(screen.getByRole("status")).toHaveTextContent("Internet connected");
    expect(screen.getByRole("status")).toHaveTextContent("203.0.113.8");
    expect(screen.getByRole("status")).toHaveTextContent("Tehran");
    expect(screen.getByRole("status")).toHaveTextContent("🇮🇷");
    expect(screen.getByRole("status").className).toMatch(/sticky/);
  });

  it("creates flags only for valid ISO country codes", () => {
    expect(countryFlag("US")).toBe("🇺🇸");
    expect(countryFlag("unknown")).toBe("");
    expect(countryFlag(null)).toBe("");
  });
});
