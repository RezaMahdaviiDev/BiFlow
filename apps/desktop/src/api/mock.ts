import { APP_VERSION } from "../version";
import type {
  AppConfig,
  BootstrapResult,
  CloudRulesStatus,
  DependencyStatus,
  DiagnosticStep,
  DiagnosticsReport,
  DebugLogStatus,
  DirectRule,
  DirectRulesDocument,
  ExportResult,
  InstallGuide,
  InstallResult,
  LogEntry,
  NetworkStatus,
  OperationAccepted,
  RouteTestResult,
  StackPhase,
  StackSnapshot,
  UpdateProgress,
  UpdateStatus,
  ValidationIssue,
} from "./models";

const now = () => new Date().toISOString();
const component = (
  phase: StackSnapshot["mihomo"]["phase"],
  message: string,
) => ({
  phase,
  message,
  since: now(),
});

function initialSnapshot(): StackSnapshot {
  const helperMissing =
    typeof sessionStorage !== "undefined" &&
    sessionStorage.getItem("biflow-mock-force-missing-helper") === "1";
  return {
    revision: 1,
    phase: "stopped",
    operation_id: null,
    helper: helperMissing
      ? {
          phase: "unavailable",
          message: "Helper service is not installed or running",
          since: now(),
        }
      : {
          phase: "running",
          message: "Mock helper is ready",
          since: now(),
        },
    hiddify: component("stopped", "Hiddify proxy is not listening"),
    mihomo: component("stopped", "Mihomo controller is not listening"),
    tun: component("stopped", "TUN interface is absent"),
    dns: component("stopped", "DNS listener is inactive"),
    providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
    exit_ip: null,
    backend: "external_hiddify",
    last_error: null,
    updated_at: now(),
  };
}

function initialSettings(): AppConfig {
  return {
    schema_version: 1,
    revision: 0,
    hiddify: {
      host: "127.0.0.1",
      port: 12334,
      executable: "auto",
      start_timeout_seconds: 45,
      stop_with_stack: true,
    },
    mihomo: {
      controller_host: "127.0.0.1",
      controller_port: 19090,
      controller_secret: "[managed by desktop core]",
      mixed_port: 17890,
      dns_port: 1053,
      tun_name: "clash-iran",
      log_level: "info",
    },
    rules: { refresh_interval_minutes: 15, upstream_refresh_hours: 24 },
    behavior: {
      launch_at_login: false,
      connect_at_launch: false,
      close_to_tray: true,
    },
  };
}

function initialDirectRules(): DirectRulesDocument {
  return {
    revision: 1,
    rules: [
      {
        target: { kind: "domain", value: "example.ir" },
        resolved_ips: ["203.0.113.8"],
        created_at: now(),
        refreshed_at: now(),
      },
    ],
  };
}

function initialCloudRules(): CloudRulesStatus {
  return {
    domain_count: 62_828,
    ip_count: 2_906,
    last_synced_at: null,
    source: "bundled",
    snapshot_revision: null,
    sets: [
      {
        id: "iran-domains",
        kind: "domain",
        entry_count: 62_828,
        source: "bundled",
        sha256: null,
      },
      {
        id: "iran-networks",
        kind: "ip_cidr",
        entry_count: 2_888,
        source: "bundled",
        sha256: null,
      },
      {
        id: "private",
        kind: "ip_cidr",
        entry_count: 18,
        source: "bundled",
        sha256: null,
      },
    ],
  };
}

function initialDependencies(): DependencyStatus[] {
  if (
    typeof sessionStorage !== "undefined" &&
    sessionStorage.getItem("biflow-mock-force-missing-deps") === "1"
  ) {
    return missingDependencies();
  }
  return mergeInstalled(detectedDependencies(), loadSavedDependencies());
}

function persistMockDependencies() {
  try {
    localStorage.setItem(
      "biflow-mock-installed-deps",
      JSON.stringify(dependencies),
    );
  } catch {
    // Ignore quota / private-mode failures.
  }
}

function loadSavedDependencies(): DependencyStatus[] | null {
  try {
    const raw = localStorage.getItem("biflow-mock-installed-deps");
    if (!raw) return null;
    const parsed = JSON.parse(raw) as DependencyStatus[];
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function mergeInstalled(
  base: DependencyStatus[],
  saved: DependencyStatus[] | null,
): DependencyStatus[] {
  if (!saved) return base;
  return base.map((item) => {
    const extra = saved.find((entry) => entry.id === item.id);
    return extra?.installed
      ? { ...item, installed: true, path: extra.path ?? item.path }
      : item;
  });
}

function missingDependencies(): DependencyStatus[] {
  return [
    {
      id: "hiddify",
      name: "Hiddify",
      installed: false,
      version: null,
      path: null,
    },
    {
      id: "mihomo",
      name: "Mihomo",
      installed: false,
      version: null,
      path: null,
    },
  ];
}

function detectedDependencies(): DependencyStatus[] {
  const hiddify = Boolean(__MOCK_HIDDIFY_INSTALLED__);
  const mihomo = Boolean(__MOCK_MIHOMO_INSTALLED__);
  return [
    {
      id: "hiddify",
      name: "Hiddify",
      installed: hiddify,
      version: null,
      path: hiddify ? "detected" : null,
    },
    {
      id: "mihomo",
      name: "Mihomo",
      installed: mihomo,
      version: null,
      path: mihomo ? "detected" : null,
    },
  ];
}

let snapshot = initialSnapshot();
let settings = initialSettings();
let directRules = initialDirectRules();
let cloudRules = initialCloudRules();
let dependencies = initialDependencies();

function mockNetworkStatus(): NetworkStatus {
  return {
    state: "online",
    public_ip: "198.51.100.24",
    country_code: "IR",
    city: "Tehran",
    checked_at: now(),
    detail: "Internet is reachable",
  };
}

function guideFor(id: string): InstallGuide {
  const linux =
    typeof navigator === "undefined" || !/win/i.test(navigator.platform);
  if (id === "mihomo") {
    return {
      id,
      title: linux ? "Install Mihomo on Linux" : "Install Mihomo on Windows",
      download_url: "https://github.com/MetaCubeX/mihomo/releases/latest",
      steps: linux
        ? [
            "Download mihomo-linux-amd64 gzip from the MetaCubeX GitHub release.",
            "Decompress it into ~/.local/share/biflow/bin/mihomo",
            "chmod +x ~/.local/share/biflow/bin/mihomo",
            "Restart BiFlow and press Connect.",
          ]
        : [
            "Download the Windows zip from the MetaCubeX GitHub release.",
            "Extract mihomo.exe into %LOCALAPPDATA%\\biflow\\bin\\mihomo.exe",
            "Restart BiFlow and press Connect.",
          ],
    };
  }
  return {
    id,
    title: linux ? "Install Hiddify on Linux" : "Install Hiddify on Windows",
    download_url: "https://github.com/hiddify/hiddify-app/releases/latest",
    steps: linux
      ? [
          "Download Hiddify-Linux-x64-AppImage.AppImage from the official GitHub release.",
          "chmod +x Hiddify-Linux-x64-AppImage.AppImage",
          "Move it to ~/.local/share/biflow/apps/Hiddify.AppImage",
          "Restart BiFlow and press Connect.",
        ]
      : [
          "Download Hiddify-Windows-Setup-x64.exe from the official GitHub release.",
          "Run the installer and accept the permission prompt.",
          "Restart BiFlow and press Connect.",
        ],
  };
}

const logs: LogEntry[] = [
  {
    timestamp: now(),
    level: "info",
    event: "mock_transport_ready",
    fields: { mode: "development" },
  },
];
let debugLogSize = 48_512;

function mockDebugLogStatus(): DebugLogStatus {
  return {
    path: "/home/user/.local/share/biflow/debug.log",
    size_bytes: debugLogSize,
  };
}

const listeners = new Set<(next: StackSnapshot) => void>();
const updateListeners = new Set<(progress: UpdateProgress) => void>();

function mockUpdateAvailable(): boolean {
  return (
    typeof sessionStorage !== "undefined" &&
    sessionStorage.getItem("biflow-mock-update-available") === "1"
  );
}

function mockUpdateShouldFail(): boolean {
  return (
    typeof sessionStorage !== "undefined" &&
    sessionStorage.getItem("biflow-mock-update-fail") === "1"
  );
}

function emitUpdateProgress(progress: UpdateProgress) {
  for (const listener of updateListeners) {
    listener(structuredClone(progress));
  }
}

async function simulateInstallProgress(version: string) {
  for (const percent of [0, 35, 70, 100]) {
    emitUpdateProgress({
      phase: "downloading",
      percent,
      version,
      error: null,
    });
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  emitUpdateProgress({
    phase: "installing",
    percent: 100,
    version,
    error: null,
  });
  await new Promise((resolve) => setTimeout(resolve, 20));
  emitUpdateProgress({
    phase: "restarting",
    percent: 100,
    version,
    error: null,
  });
}

function emit(phase: StackPhase, operationId: string | null) {
  snapshot = {
    ...snapshot,
    revision: snapshot.revision + 1,
    phase,
    operation_id: operationId,
    updated_at: now(),
  };
  for (const listener of listeners) listener(structuredClone(snapshot));
}

function operation(): OperationAccepted {
  return { operation_id: crypto.randomUUID(), already_complete: false };
}

async function runStart(accepted: OperationAccepted) {
  const phases: StackPhase[] = [
    "starting_hiddify",
    "preparing_runtime",
    "validating_config",
    "starting_core",
    "checking_readiness",
  ];
  for (const phase of phases) {
    emit(phase, accepted.operation_id);
    await new Promise((resolve) => setTimeout(resolve, 180));
  }
  snapshot = {
    ...snapshot,
    hiddify: component("running", "Hiddify proxy is listening"),
    mihomo: component("running", "Mihomo controller is ready"),
    tun: component("running", "TUN interface is active"),
    dns: component("running", "DNS listener is active"),
    providers: {
      ready: 6,
      total: 6,
      rules_loaded: 184203,
      last_refresh: now(),
    },
    exit_ip: "203.0.113.42",
  };
  emit("running", null);
  logs.push({
    timestamp: now(),
    level: "info",
    event: "stack_running",
    fields: {},
  });
}

export const mockApi = {
  async bootstrap(): Promise<BootstrapResult> {
    return {
      app_version: APP_VERSION,
      platform: navigator.platform,
      mock_mode: true,
      snapshot: structuredClone(snapshot),
      settings: structuredClone(settings),
      direct_rules: structuredClone(directRules),
      cloud_rules: structuredClone(cloudRules),
      dependencies: structuredClone(dependencies),
      network_status: mockNetworkStatus(),
    };
  },
  async getSnapshot() {
    return structuredClone(snapshot);
  },
  async getNetworkStatus() {
    return mockNetworkStatus();
  },
  async start(): Promise<OperationAccepted> {
    if (snapshot.phase === "running") {
      return { operation_id: crypto.randomUUID(), already_complete: true };
    }
    const accepted = operation();
    void runStart(accepted);
    return accepted;
  },
  async stop(): Promise<OperationAccepted> {
    if (snapshot.phase === "stopped") {
      return { operation_id: crypto.randomUUID(), already_complete: true };
    }
    const accepted = operation();
    emit("stopping", accepted.operation_id);
    window.setTimeout(() => {
      snapshot = {
        ...snapshot,
        hiddify: component("stopped", "Hiddify proxy is not listening"),
        mihomo: component("stopped", "Mihomo controller is not listening"),
        tun: component("stopped", "TUN interface is absent"),
        dns: component("stopped", "DNS listener is inactive"),
        providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
        exit_ip: null,
      };
      emit("stopped", null);
    }, 350);
    return accepted;
  },
  async pause(): Promise<OperationAccepted> {
    if (snapshot.phase === "paused") {
      return { operation_id: crypto.randomUUID(), already_complete: true };
    }
    if (snapshot.phase !== "running" && snapshot.phase !== "degraded") {
      return { operation_id: crypto.randomUUID(), already_complete: true };
    }
    const accepted = operation();
    emit("stopping", accepted.operation_id);
    window.setTimeout(() => {
      snapshot = {
        ...snapshot,
        hiddify: component("running", "Hiddify proxy is listening"),
        mihomo: component("stopped", "Mihomo controller is not listening"),
        tun: component("stopped", "TUN interface is absent"),
        dns: component("stopped", "DNS listener is inactive"),
        providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
        exit_ip: null,
      };
      emit("paused", null);
    }, 350);
    return accepted;
  },
  async resume(): Promise<OperationAccepted> {
    if (snapshot.phase === "running") {
      return { operation_id: crypto.randomUUID(), already_complete: true };
    }
    if (snapshot.phase !== "paused") {
      return { operation_id: crypto.randomUUID(), already_complete: true };
    }
    const accepted = operation();
    void runStart(accepted);
    return accepted;
  },
  async cancel(operationId: string) {
    if (snapshot.operation_id === operationId) {
      emit("stopped", null);
      return true;
    }
    return false;
  },
  async getSettings() {
    return structuredClone(settings);
  },
  async validateSettings(draft: AppConfig): Promise<ValidationIssue[]> {
    const ports = [
      draft.hiddify.port,
      draft.mihomo.controller_port,
      draft.mihomo.mixed_port,
      draft.mihomo.dns_port,
    ];
    const issues: ValidationIssue[] = [];
    if (new Set(ports).size !== ports.length) {
      issues.push({
        field: "ports",
        code: "PORT_CONFLICT",
        message: "Ports must be unique",
      });
    }
    if (draft.hiddify.host !== "127.0.0.1") {
      issues.push({
        field: "hiddify.host",
        code: "LOOPBACK_REQUIRED",
        message: "Hiddify must listen on loopback",
      });
    }
    return issues;
  },
  async saveSettings(draft: AppConfig, expectedRevision: number) {
    if (expectedRevision !== settings.revision)
      throw new Error("Settings changed in another window");
    settings = { ...structuredClone(draft), revision: settings.revision + 1 };
    return structuredClone(settings);
  },
  async listRules() {
    return structuredClone(directRules);
  },
  async addRule(input: string, expectedRevision: number) {
    if (expectedRevision !== directRules.revision)
      throw new Error("Rules changed in another window");
    const value = input.trim().toLowerCase();
    const kind = /^\d{1,3}(\.\d{1,3}){3}$/.test(value) ? "ip" : "domain";
    const rule: DirectRule = {
      target: { kind, value },
      resolved_ips: kind === "ip" ? [value] : ["203.0.113.9"],
      created_at: now(),
      refreshed_at: now(),
    };
    if (!directRules.rules.some((item) => item.target.value === value)) {
      directRules = {
        revision: directRules.revision + 1,
        rules: [...directRules.rules, rule],
      };
    }
    return structuredClone(directRules);
  },
  async removeRule(input: string, expectedRevision: number) {
    if (expectedRevision !== directRules.revision)
      throw new Error("Rules changed in another window");
    directRules = {
      revision: directRules.revision + 1,
      rules: directRules.rules.filter((item) => item.target.value !== input),
    };
    return structuredClone(directRules);
  },
  async refreshRules() {
    directRules = {
      revision: directRules.revision + 1,
      rules: directRules.rules.map((item) => ({
        ...item,
        refreshed_at: now(),
      })),
    };
    return structuredClone(directRules);
  },
  async getCloudRules() {
    return structuredClone(cloudRules);
  },
  async syncCloudRules() {
    cloudRules = {
      ...cloudRules,
      last_synced_at: now(),
      source: "devlifeX/BiFlow",
      snapshot_revision: "767ef8bf5673",
      domain_count: 63_104,
      ip_count: 2_912,
    };
    return structuredClone(cloudRules);
  },
  async listDependencies() {
    return structuredClone(dependencies);
  },
  async installDependency(id: string): Promise<InstallResult> {
    await new Promise((resolve) => setTimeout(resolve, 250));
    dependencies = dependencies.map((item) =>
      item.id === id
        ? { ...item, installed: true, path: `/tmp/biflow/${id}` }
        : item,
    );
    persistMockDependencies();
    return {
      id,
      installed: true,
      path: `/tmp/biflow/${id}`,
      guide: guideFor(id),
    };
  },
  async installHelper(): Promise<{ installed: boolean }> {
    await new Promise((resolve) => setTimeout(resolve, 150));
    snapshot = {
      ...snapshot,
      revision: snapshot.revision + 1,
      helper: {
        phase: "running",
        message: "Mock helper is ready",
        since: now(),
      },
      updated_at: now(),
    };
    for (const listener of listeners) listener(structuredClone(snapshot));
    return { installed: true };
  },
  async getInstallGuide(id: string) {
    return guideFor(id);
  },
  async openUrl(_url: string) {
    return undefined;
  },
  async testRoute(target: string): Promise<RouteTestResult> {
    const direct =
      target.endsWith(".ir") ||
      directRules.rules.some((item) => item.target.value === target);
    return {
      target,
      outbound: direct ? "direct" : "vpn",
      reason: direct ? "custom_or_iran_rule" : "default_proxy",
      matched_rule: direct ? target : "MATCH",
      reachable: true,
      tested_at: now(),
    };
  },
  async diagnostics(): Promise<DiagnosticsReport> {
    const steps: DiagnosticStep[] = [
      "Helper authorization",
      "Hiddify listener",
      "Mihomo controller",
      "Providers",
      "Owned TUN state",
      "Foreign egress",
    ].map((label, index) => ({
      id: String(index),
      label,
      status:
        index === 2 && snapshot.phase === "stopped" ? "warning" : "passed",
      detail:
        index === 2 && snapshot.phase === "stopped"
          ? "Stack is disconnected"
          : null,
      started_at: now(),
      finished_at: now(),
    }));
    return { operation_id: crypto.randomUUID(), steps, finished: true };
  },
  async queryLogs() {
    return structuredClone(logs);
  },
  async debugLogStatus(): Promise<DebugLogStatus> {
    return mockDebugLogStatus();
  },
  async revealDebugLog(): Promise<DebugLogStatus> {
    return mockDebugLogStatus();
  },
  async deleteDebugLog(): Promise<DebugLogStatus> {
    debugLogSize = 512;
    return mockDebugLogStatus();
  },
  async exportBundle(): Promise<ExportResult> {
    return {
      path: "/tmp/biflow-support-mock.json",
      files: [
        "versions.json",
        "config-redacted.json",
        "snapshot.json",
        "debug.log",
      ],
    };
  },
  async checkUpdate(): Promise<UpdateStatus> {
    if (mockUpdateShouldFail()) {
      throw new Error("Malformed update manifest");
    }
    if (mockUpdateAvailable()) {
      return {
        available: true,
        version: "9.9.9",
        notes: "Mock signed release",
      };
    }
    return { available: false, version: null, notes: null };
  },
  async installUpdate(): Promise<OperationAccepted> {
    if (mockUpdateShouldFail()) {
      emitUpdateProgress({
        phase: "failed",
        percent: null,
        version: "9.9.9",
        error: "Signature verification failed",
      });
      throw new Error("Signature verification failed");
    }
    if (!mockUpdateAvailable()) {
      throw new Error("no update is available");
    }
    const accepted = operation();
    await simulateInstallProgress("9.9.9");
    return accepted;
  },
  subscribeUpdateProgress(listener: (progress: UpdateProgress) => void) {
    updateListeners.add(listener);
    return () => updateListeners.delete(listener);
  },
  subscribe(listener: (next: StackSnapshot) => void) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};

export function resetMockState() {
  try {
    sessionStorage.setItem("biflow-mock-force-missing-deps", "1");
    localStorage.removeItem("biflow-mock-installed-deps");
  } catch {
    // jsdom and Playwright always provide web storage.
  }
  snapshot = initialSnapshot();
  settings = initialSettings();
  directRules = initialDirectRules();
  cloudRules = initialCloudRules();
  dependencies = missingDependencies();
  logs.length = 0;
  debugLogSize = 48_512;
  logs.push({
    timestamp: now(),
    level: "info",
    event: "mock_transport_ready",
    fields: { mode: "development" },
  });
  listeners.clear();
  updateListeners.clear();
}

if (typeof window !== "undefined") {
  window.__BIFLOW_RESET_MOCK = resetMockState;
}
