import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppConfig } from "./models";

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

describe("native desktop transport", () => {
  beforeAll(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
  });

  afterAll(() => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  });

  it("routes UI calls and events through the Tauri bridge", async () => {
    vi.resetModules();
    const boot = { mock_mode: false };
    const unlisten = vi.fn();
    tauri.invoke.mockResolvedValue(boot);
    tauri.listen.mockResolvedValue(unlisten);

    const { desktop } = await import("./desktop");
    const draft = { revision: 4 } as AppConfig;

    await expect(desktop.bootstrap()).resolves.toBe(boot);
    await desktop.getNetworkStatus();
    await desktop.saveSettings(draft, 4);
    await desktop.cancel("operation-1");
    await desktop.installDependency("mihomo");
    const listener = vi.fn();
    await expect(desktop.subscribe(listener)).resolves.toBe(unlisten);

    expect(tauri.invoke).toHaveBeenNthCalledWith(1, "bootstrap_app");
    expect(tauri.invoke).toHaveBeenNthCalledWith(2, "get_network_status");
    expect(tauri.invoke).toHaveBeenNthCalledWith(3, "save_settings", {
      draft,
      expectedRevision: 4,
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(4, "cancel_operation", {
      operationId: "operation-1",
    });
    expect(tauri.invoke).toHaveBeenNthCalledWith(5, "install_dependency", {
      id: "mihomo",
    });
    expect(tauri.listen).toHaveBeenCalledWith(
      "stack-snapshot",
      expect.any(Function),
    );
  });
});
