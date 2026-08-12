import { beforeEach, describe, expect, it, vi } from "vitest";
import { desktop } from "../api/desktop";
import type { BootstrapResult } from "../api/models";
import { useAppStore } from "./app";

vi.mock("../api/desktop", () => ({
  desktop: {
    bootstrap: vi.fn(),
    subscribe: vi.fn(async () => () => undefined),
    start: vi.fn(),
    stop: vi.fn(),
    installDependency: vi.fn(),
    listDependencies: vi.fn(),
    getInstallGuide: vi.fn(),
    syncCloudRules: vi.fn(),
    getNetworkStatus: vi.fn(),
  },
}));

const boot = {
  app_version: "1.0.0",
  platform: "linux",
  mock_mode: true,
  snapshot: {
    revision: 1,
    phase: "stopped",
    operation_id: null,
    helper: { phase: "running", message: "Helper is ready", since: "now" },
    hiddify: { phase: "stopped", message: null, since: "now" },
    mihomo: { phase: "stopped", message: null, since: "now" },
    tun: { phase: "stopped", message: null, since: "now" },
    dns: { phase: "stopped", message: null, since: "now" },
    providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
    exit_ip: null,
    backend: "external_hiddify",
    last_error: null,
    updated_at: "now",
  },
  settings: { revision: 0 },
  direct_rules: { revision: 1, rules: [] },
  cloud_rules: {
    domain_count: 10,
    ip_count: 4,
    last_synced_at: null,
    source: "bundled",
    sets: [],
  },
  dependencies: [
    {
      id: "hiddify",
      name: "Hiddify",
      installed: false,
      version: null,
      path: null,
    },
  ],
  network_status: {
    state: "checking",
    public_ip: null,
    country_code: null,
    city: null,
    checked_at: "now",
    detail: null,
  },
} as unknown as BootstrapResult;

describe("app store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
      networkStatus: null,
      diagnostics: null,
      error: null,
      installGuide: null,
    });
  });

  it("loads bootstrap data including cloud rules and dependencies", async () => {
    vi.mocked(desktop.bootstrap).mockResolvedValue(boot);
    vi.mocked(desktop.getNetworkStatus).mockResolvedValue({
      state: "online",
      public_ip: "203.0.113.8",
      country_code: "IR",
      city: "Tehran",
      checked_at: "now",
      detail: null,
    });
    await useAppStore.getState().initialize();
    const state = useAppStore.getState();
    expect(state.loading).toBe(false);
    expect(state.cloudRules?.domain_count).toBe(10);
    expect(state.dependencies[0]?.id).toBe("hiddify");
    expect(desktop.getNetworkStatus).toHaveBeenCalledOnce();
  });

  it("starts the stack from a stopped snapshot", async () => {
    vi.mocked(desktop.start).mockResolvedValue({
      operation_id: "op",
      already_complete: false,
    });
    useAppStore.setState({ snapshot: boot.snapshot, actionPending: false });
    await useAppStore.getState().toggleConnection();
    expect(desktop.start).toHaveBeenCalledOnce();
    expect(useAppStore.getState().actionPending).toBe(true);
  });

  it("keeps a manual install guide when automatic install fails", async () => {
    vi.mocked(desktop.installDependency).mockRejectedValue(
      new Error("download blocked"),
    );
    vi.mocked(desktop.getInstallGuide).mockResolvedValue({
      id: "hiddify",
      title: "Install Hiddify on Linux",
      download_url: "https://github.com/hiddify/hiddify-app/releases/latest",
      steps: ["Download the AppImage"],
    });
    await useAppStore.getState().installDependency("hiddify");
    const state = useAppStore.getState();
    expect(state.installingId).toBeNull();
    expect(state.error).toMatch(/download blocked/);
    expect(state.installGuide?.steps[0]).toMatch(/AppImage/);
  });

  it("updates cloud rule counts after a successful sync", async () => {
    vi.mocked(desktop.syncCloudRules).mockResolvedValue({
      domain_count: 20,
      ip_count: 8,
      last_synced_at: "2026-08-13T00:00:00.000Z",
      source: "jsdelivr",
      sets: [],
    });
    await useAppStore.getState().syncCloudRules();
    expect(useAppStore.getState().cloudRules?.source).toBe("jsdelivr");
    expect(useAppStore.getState().actionPending).toBe(false);
  });
});
