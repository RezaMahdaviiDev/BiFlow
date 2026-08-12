import type {
  AppConfig,
  BootstrapResult,
  DiagnosticStep,
  DiagnosticsReport,
  DirectRule,
  DirectRulesDocument,
  ExportResult,
  LogEntry,
  OperationAccepted,
  RouteTestResult,
  StackPhase,
  StackSnapshot,
  UpdateStatus,
  ValidationIssue,
} from "./models";

const now = () => new Date().toISOString();
const component = (phase: StackSnapshot["mihomo"]["phase"]) => ({
  phase,
  message: null,
  since: now(),
});

let snapshot: StackSnapshot = {
  revision: 1,
  phase: "stopped",
  operation_id: null,
  hiddify: component("stopped"),
  mihomo: component("stopped"),
  tun: component("stopped"),
  dns: component("stopped"),
  providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
  exit_ip: null,
  backend: "external_hiddify",
  last_error: null,
  updated_at: now(),
};

let settings: AppConfig = {
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

let directRules: DirectRulesDocument = {
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

const logs: LogEntry[] = [
  {
    timestamp: now(),
    level: "info",
    event: "mock_transport_ready",
    fields: { mode: "development" },
  },
];

const listeners = new Set<(next: StackSnapshot) => void>();

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
    hiddify: component("running"),
    mihomo: component("running"),
    tun: component("running"),
    dns: component("running"),
    providers: { ready: 6, total: 6, rules_loaded: 184203, last_refresh: now() },
    exit_ip: "203.0.113.42",
  };
  emit("running", null);
  logs.push({ timestamp: now(), level: "info", event: "stack_running", fields: {} });
}

export const mockApi = {
  async bootstrap(): Promise<BootstrapResult> {
    return {
      app_version: "0.1.0-dev",
      platform: navigator.platform,
      mock_mode: true,
      snapshot: structuredClone(snapshot),
      settings: structuredClone(settings),
      direct_rules: structuredClone(directRules),
    };
  },
  async getSnapshot() {
    return structuredClone(snapshot);
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
        hiddify: component("stopped"),
        mihomo: component("stopped"),
        tun: component("stopped"),
        dns: component("stopped"),
        providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
        exit_ip: null,
      };
      emit("stopped", null);
    }, 350);
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
      issues.push({ field: "ports", code: "PORT_CONFLICT", message: "Ports must be unique" });
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
    if (expectedRevision !== settings.revision) throw new Error("Settings changed in another window");
    settings = { ...structuredClone(draft), revision: settings.revision + 1 };
    return structuredClone(settings);
  },
  async listRules() {
    return structuredClone(directRules);
  },
  async addRule(input: string, expectedRevision: number) {
    if (expectedRevision !== directRules.revision) throw new Error("Rules changed in another window");
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
    if (expectedRevision !== directRules.revision) throw new Error("Rules changed in another window");
    directRules = {
      revision: directRules.revision + 1,
      rules: directRules.rules.filter((item) => item.target.value !== input),
    };
    return structuredClone(directRules);
  },
  async refreshRules() {
    directRules = {
      revision: directRules.revision + 1,
      rules: directRules.rules.map((item) => ({ ...item, refreshed_at: now() })),
    };
    return structuredClone(directRules);
  },
  async testRoute(target: string): Promise<RouteTestResult> {
    const direct =
      target.endsWith(".ir") || directRules.rules.some((item) => item.target.value === target);
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
      status: index === 2 && snapshot.phase === "stopped" ? "warning" : "passed",
      detail: index === 2 && snapshot.phase === "stopped" ? "Stack is disconnected" : null,
      started_at: now(),
      finished_at: now(),
    }));
    return { operation_id: crypto.randomUUID(), steps, finished: true };
  },
  async queryLogs() {
    return structuredClone(logs);
  },
  async exportBundle(): Promise<ExportResult> {
    return {
      path: "/tmp/iran-split-support-mock.json",
      files: ["versions.json", "config-redacted.toml", "snapshot.json", "logs.json"],
    };
  },
  async checkUpdate(): Promise<UpdateStatus> {
    return { available: false, version: null, notes: null };
  },
  subscribe(listener: (next: StackSnapshot) => void) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};
