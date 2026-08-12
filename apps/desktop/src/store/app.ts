import { create } from "zustand";
import { desktop } from "../api/desktop";
import type {
  AppConfig,
  BootstrapResult,
  DiagnosticsReport,
  DirectRulesDocument,
  StackSnapshot,
} from "../api/models";

type Page = "dashboard" | "rules" | "diagnostics" | "settings";

interface AppStore {
  loading: boolean;
  actionPending: boolean;
  page: Page;
  boot: BootstrapResult | null;
  snapshot: StackSnapshot | null;
  settings: AppConfig | null;
  rules: DirectRulesDocument | null;
  diagnostics: DiagnosticsReport | null;
  error: string | null;
  setPage: (page: Page) => void;
  initialize: () => Promise<() => void>;
  toggleConnection: () => Promise<void>;
  cancel: () => Promise<void>;
  saveSettings: (draft: AppConfig) => Promise<void>;
  addRule: (input: string) => Promise<void>;
  removeRule: (input: string) => Promise<void>;
  refreshRules: () => Promise<void>;
  runDiagnostics: () => Promise<void>;
  clearError: () => void;
}

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "An unexpected error occurred";
}

export const useAppStore = create<AppStore>((set, get) => ({
  loading: true,
  actionPending: false,
  page: "dashboard",
  boot: null,
  snapshot: null,
  settings: null,
  rules: null,
  diagnostics: null,
  error: null,
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
      });
      return desktop.subscribe((snapshot) => set({ snapshot, actionPending: false }));
    } catch (error) {
      set({ loading: false, error: message(error) });
      return () => undefined;
    }
  },
  toggleConnection: async () => {
    const snapshot = get().snapshot;
    if (!snapshot) return;
    set({ actionPending: true, error: null });
    try {
      if (snapshot.phase === "running" || snapshot.phase === "degraded") {
        await desktop.stop();
      } else {
        await desktop.start();
      }
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
  runDiagnostics: async () => {
    set({ actionPending: true, diagnostics: null, error: null });
    try {
      const diagnostics = await desktop.diagnostics();
      set({ diagnostics, actionPending: false });
    } catch (error) {
      set({ actionPending: false, error: message(error) });
    }
  },
  clearError: () => set({ error: null }),
}));
