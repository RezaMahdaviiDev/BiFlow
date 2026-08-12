import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { App } from "./App";
import { resetMockState } from "./api/mock";
import { useAppStore } from "./store/app";
import { APP_VERSION } from "./version";

beforeEach(() => {
  resetMockState();
  useAppStore.setState({
    loading: true,
    actionPending: false,
    installingId: null,
    page: "dashboard",
    boot: null,
    snapshot: null,
    settings: null,
    rules: null,
    cloudRules: null,
    dependencies: [],
    diagnostics: null,
    error: null,
    installGuide: null,
  });
});

describe("App", () => {
  it("boots BiFlow and walks the primary screens", async () => {
    render(<App />);
    expect(
      await screen.findByRole("heading", { name: "Ready when you are" }),
    ).toBeVisible();
    expect(screen.getByText("BiFlow")).toBeVisible();
    expect(screen.getAllByRole("button", { name: /^Install$/ })).toHaveLength(
      2,
    );

    await userEvent.click(screen.getByRole("button", { name: "Direct rules" }));
    expect(screen.getByRole("heading", { name: "Direct rules" })).toBeVisible();
    expect(
      screen.getByRole("button", { name: /update from cloud/i }),
    ).toBeEnabled();

    await userEvent.click(screen.getByRole("button", { name: "Diagnostics" }));
    expect(screen.getByRole("heading", { name: "Diagnostics" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Test flow" })).toBeDisabled();
    await userEvent.type(
      screen.getByLabelText("Test IP or domain"),
      "example.ir",
    );
    expect(screen.getByRole("button", { name: "Test flow" })).toBeEnabled();

    await userEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("heading", { name: "Settings" })).toBeVisible();
  });

  it("exposes the version file through bootstrap", async () => {
    render(<App />);
    expect(
      await screen.findByRole("heading", { name: "Ready when you are" }),
    ).toBeVisible();
    expect(screen.getByText("BiFlow")).toBeVisible();
    expect(APP_VERSION).toMatch(/^\d+\.\d+\.\d+$/);
  });
});
