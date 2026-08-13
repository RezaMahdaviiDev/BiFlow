import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { About } from "./About";
import { useAppStore } from "../store/app";

vi.mock("../version", () => ({
  APP_VERSION: "1.2.0",
}));

describe("About", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    useAppStore.setState({
      update: {
        phase: "idle",
        percent: null,
        version: null,
        error: null,
      },
    });
  });

  it("shows author, repository, and root-sourced version", () => {
    render(<About />);
    expect(screen.getByText("Dariush Vesal")).toBeInTheDocument();
    expect(screen.getByText("1.2.0")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /devlifeX\/BiFlow/i }),
    ).toBeInTheDocument();
  });

  it("renders update progress and retry when the store reports failure", () => {
    useAppStore.setState({
      update: {
        phase: "failed",
        percent: null,
        version: "1.3.0",
        error: "Signature verification failed",
      },
    });
    render(<About />);
    expect(screen.getByRole("status")).toHaveTextContent(
      /could not check for updates/i,
    );
    expect(
      screen.getByText("Signature verification failed"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /retry update/i }),
    ).toBeInTheDocument();
  });

  it("explains the Debian download/open path", () => {
    useAppStore.setState({
      update: {
        phase: "manual",
        percent: null,
        version: "1.3.0",
        error: null,
      },
    });
    render(<About />);
    expect(screen.getByRole("status")).toHaveTextContent(/cannot self-update/i);
  });

  it("offers install when an update is available", async () => {
    const installUpdate = vi.fn(async () => undefined);
    useAppStore.setState({
      update: {
        phase: "available",
        percent: null,
        version: "1.3.0",
        error: null,
      },
      installUpdate,
    });
    render(<About />);
    await userEvent.click(
      screen.getByRole("button", { name: /install update 1\.3\.0/i }),
    );
    expect(installUpdate).toHaveBeenCalledOnce();
  });
});
