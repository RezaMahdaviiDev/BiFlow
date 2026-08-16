import { create } from "zustand";
import { desktop } from "../api/desktop";
import { missingConnectRequirements } from "../lib/connectRequirements";
import { ACTION_TIMEOUT_MS, controlsLocked } from "../lib/lifecycle";
import type {
  AppConfig,
  BootstrapResult,
  CloudRulesStatus,
  DependencyStatus,
  DiagnosticsReport,
  DirectRulesDocument,
  InstallGuide,
  NetworkStatus,
  StackSnapshot,
  TrafficTotals,
  UpdateProgress,
  UpdateStatus,
} from "../api/models";

type Page = "dashboard" | "rules" | "diagnostics" | "settings" | "about";

const initialUpdateProgress = (): UpdateProgress => ({
  phase: "idle",
  percent: null,
  version: null,
  error: null,
  app_available: false,
  rules_available: false,
  thirdparty_available: false,
});

interface AppStore {
  loading: boolean;
  actionPending: boolean;
  installingId: string | null;
  page: Page;
  boot: BootstrapResult | null;
  snapshot: StackSnapshot | null;
  settings: AppConfig | null;
  rules: DirectRulesDocument | null;
  cloudRules: CloudRulesStatus | null;
  dependencies: DependencyStatus[];
  networkStatus: NetworkStatus | null;
  networkRefreshing: boolean;
  trafficTotals: TrafficTotals;
  trafficRefreshing: boolean;
  diagnostics: DiagnosticsReport | null;
  error: string | null;
  installGuide: InstallGuide | null;
  update: UpdateProgress;
  setPage: (page: Page) => void;
  initialize: () => Promise<() => void>;
  toggleConnection: () => Promise<void>;
  pauseConnection: () => Promise<void>;
  resumeConnection: () => Promise<void>;
  cancel: () => Promise<void>;
  saveSettings: (draft: AppConfig) => Promise<void>;
  addRule: (input: string) => Promise<void>;
  pinRoute: (input: string, outbound: "direct" | "vpn") => Promise<void>;
  removeRule: (input: string) => Promise<void>;
  refreshRules: () => Promise<void>;
  syncCloudRules: () => Promise<void>;
  refreshNetworkStatus: () => Promise<void>;
  refreshTrafficTotals: () => Promise<void>;
  installDependency: (id: string) => Promise<void>;
  installHelper: () => Promise<void>;
  runDiagnostics: () => Promise<void>;
  applyUpdateProgress: (progress: UpdateProgress) => void;
  checkForUpdate: () => Promise<void>;
  installUpdate: () => Promise<void>;
  retryUpdate: () => Promise<void>;
  openRepository: () => Promise<void>;
  clearError: () => void;
  clearInstallGuide: () => void;
}

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "An unexpected error occurred";
}

async function ensureRequiredServices(
  get: () => AppStore,
  set: (partial: Partial<AppStore>) => void,
): Promise<void> {
  const missing = missingConnectRequirements(
    get().snapshot,
    get().dependencies,
  );
  for (const id of missing) {
    if (id === "helper") {
      set({ installingId: "helper" });
      await desktop.installHelper();
      const snapshot = await desktop.getSnapshot();
      set({ snapshot });
      if (
        snapshot.helper.phase === "unavailable" ||
        snapshot.helper.phase === "error"
      ) {
        throw new Error(
          "privileged helper is still unavailable after installation",
        );
      }
      continue;
    }
    set({ installingId: id });
    const result = await desktop.installDependency(id);
    const dependencies = await desktop.listDependencies();
    set({
      dependencies,
      installGuide: result.installed ? null : result.guide,
    });
    if (!result.installed) {
      throw new Error(`${id} installation did not complete`);
    }
  }
  set({ installingId: null });
}

export const useAppStore = create<AppStore>((set, get) => ({
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
  networkRefreshing: false,
  trafficTotals: { sent: 0, received: 0 },
  trafficRefreshing: false,
  diagnostics: null,
  error: null,
  installGuide: null,
  update: initialUpdateProgress(),
  setPage: (page) => set({ page }),
  initialize: async () => {
    try {
      const boot = await desktop.bootstrap();
      set({
        loading: false,
        boot,
        snapshot: boot.snapshot,
        settings: boot.settings,
        rules: boot.direct_rules,
        cloudRules: boot.cloud_rules,
        dependencies: boot.dependencies,
        networkStatus: boot.network_status,
        update: initialUpdateProgress(),
      });
      void get().refreshNetworkStatus();
      void get().refreshTrafficTotals();
      const unsubscribeSnapshot = await desktop.subscribe((snapshot) => {
        set({
          snapshot,
          actionPending: snapshot.busy != null,
        });
        void get().refreshTrafficTotals();
      });
      const unsubscribeUpdate = await desktop.subscribeUpdateProgress(
        (progress) => {
          get().applyUpdateProgress(progress);
        },
      );
      return () => {
        unsubscribeSnapshot();
        unsubscribeUpdate();
      };
    } catch (error) {
      set({ loading: false, error: message(error) });
      return () => undefined;
    }
  },
  toggleConnection: async () => {
    const snapshot = get().snapshot;
    if (!snapshot || controlsLocked(snapshot, get().actionPending)) return;
    set({ actionPending: true, error: null, installGuide: null });
    const timeout = window.setTimeout(() => {
      const current = get().snapshot;
      if (get().actionPending) {
        set({
          actionPending: false,
          installingId: null,
          error: "The connection operation timed out.",
          snapshot: current ? { ...current, busy: null } : current,
        });
      }
    }, ACTION_TIMEOUT_MS);
    try {
      if (
        snapshot.phase === "running" ||
        snapshot.phase === "degraded" ||
        snapshot.phase === "paused"
      ) {
        await desktop.stop();
      } else {
        await ensureRequiredServices(get, set);
        await desktop.start();
      }
    } catch (error) {
      set({
        actionPending: false,
        installingId: null,
        error: message(error),
      });
    } finally {
      window.clearTimeout(timeout);
    }
  },
  pauseConnection: async () => {
    if (controlsLocked(get().snapshot, get().actionPending)) return;
    set({ actionPending: true, error: null });
    try {
      await desktop.pause();
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  resumeConnection: async () => {
    if (controlsLocked(get().snapshot, get().actionPending)) return;
    set({ actionPending: true, error: null });
    try {
      await desktop.resume();
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  cancel: async () => {
    const operationId = get().snapshot?.operation_id;
    if (operationId) await desktop.cancel(operationId);
  },
  saveSettings: async (draft) => {
    const current = get().settings;
    if (!current) return;
    set({ actionPending: true, error: null });
    try {
      const settings = await desktop.saveSettings(draft, current.revision);
      set({ settings, actionPending: false });
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  addRule: async (input) => {
    const rules = get().rules;
    if (!rules) return;
    set({ actionPending: true, error: null });
    try {
      const next = await desktop.addRule(input, rules.revision);
      set({ rules: next, actionPending: false });
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  pinRoute: async (input, outbound) => {
    const rules = get().rules;
    if (!rules) return;
    set({ actionPending: true, error: null });
    try {
      const next = await desktop.pinRoute(input, outbound, rules.revision);
      set({ rules: next, actionPending: false });
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  removeRule: async (input) => {
    const rules = get().rules;
    if (!rules) return;
    set({ actionPending: true, error: null });
    try {
      const next = await desktop.removeRule(input, rules.revision);
      set({ rules: next, actionPending: false });
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  refreshRules: async () => {
    set({ actionPending: true, error: null });
    try {
      const rules = await desktop.refreshRules();
      set({ rules, actionPending: false });
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  syncCloudRules: async () => {
    if (get().actionPending) {
      return;
    }
    set({ actionPending: true, error: null });
    try {
      const cloudRules = await desktop.syncCloudRules();
      set({ cloudRules, actionPending: false });
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  refreshTrafficTotals: async () => {
    if (get().trafficRefreshing) {
      return;
    }
    set({ trafficRefreshing: true });
    try {
      const trafficTotals = await desktop.getTrafficTotals();
      set({ trafficTotals, trafficRefreshing: false });
    } catch {
      set({ trafficRefreshing: false });
    }
  },
  refreshNetworkStatus: async () => {
    if (get().networkRefreshing) {
      return;
    }
    set({ networkRefreshing: true });
    try {
      const networkStatus = await desktop.getNetworkStatus();
      set({ networkStatus, networkRefreshing: false });
    } catch (error) {
      set({
        networkRefreshing: false,
        networkStatus: {
          state: "offline",
          public_ip: null,
          country_code: null,
          city: null,
          checked_at: new Date().toISOString(),
          detail: message(error),
        },
      });
    }
  },
  installDependency: async (id) => {
    set({ installingId: id, error: null, installGuide: null });
    try {
      const result = await desktop.installDependency(id);
      const dependencies = await desktop.listDependencies();
      set({
        dependencies,
        installingId: null,
        installGuide: result.installed ? null : result.guide,
      });
    } catch (error) {
      const guide = await desktop.getInstallGuide(id).catch(() => null);
      set({ installingId: null, error: message(error), installGuide: guide });
    }
  },
  installHelper: async () => {
    set({ installingId: "helper", error: null });
    try {
      await desktop.installHelper();
      const snapshot = await desktop.getSnapshot();
      set({ installingId: null, snapshot });
    } catch (error) {
      set({ installingId: null, error: message(error) });
    }
  },
  runDiagnostics: async () => {
    set({ actionPending: true, diagnostics: null, error: null });
    try {
      const diagnostics = await desktop.diagnostics();
      set({ diagnostics, actionPending: false });
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  applyUpdateProgress: (progress) => {
    set({ update: progress });
  },
  checkForUpdate: async () => {
    const phase = get().update.phase;
    if (
      phase === "checking" ||
      phase === "downloading" ||
      phase === "installing" ||
      phase === "restarting"
    ) {
      return;
    }
    set({
      update: {
        phase: "checking",
        percent: null,
        version: null,
        error: null,
      },
    });
    try {
      const status = await desktop.checkUpdate();
      set({
        update: updateStatusToProgress(status),
      });
    } catch (error) {
      set({
        update: {
          phase: "failed",
          percent: null,
          version: null,
          error: message(error),
        },
      });
    }
  },
  installUpdate: async () => {
    const phase = get().update.phase;
    if (
      phase === "checking" ||
      phase === "downloading" ||
      phase === "installing" ||
      phase === "restarting"
    ) {
      return;
    }
    const current = get().update;
    set({
      update: {
        phase: "downloading",
        percent: 0,
        version: current.version,
        error: null,
      },
    });
    try {
      await desktop.installUpdate();
    } catch (error) {
      set({
        update: {
          phase: "failed",
          percent: null,
          version: current.version,
          error: message(error),
        },
      });
    }
  },
  retryUpdate: async () => {
    const { update, checkForUpdate, installUpdate } = get();
    if (update.version) {
      set({
        update: {
          phase: "available",
          percent: null,
          version: update.version,
          error: null,
        },
      });
      await installUpdate();
      return;
    }
    await checkForUpdate();
    if (get().update.phase === "available") {
      await installUpdate();
    }
  },
  openRepository: async () => {
    await desktop.openUrl("https://github.com/devlifeX/BiFlow");
  },
  clearError: () => set({ error: null }),
  clearInstallGuide: () => set({ installGuide: null, error: null }),
}));

function updateStatusToProgress(status: UpdateStatus): UpdateProgress {
  if (!status.available) {
    return {
      phase: "current",
      percent: null,
      version: null,
      error: null,
      app_available: false,
      rules_available: false,
      thirdparty_available: false,
    };
  }
  return {
    phase: "available",
    percent: null,
    version: status.version,
    error: null,
    app_available: status.app_available,
    rules_available: status.rules_available,
    thirdparty_available: status.thirdparty_available,
  };
}
