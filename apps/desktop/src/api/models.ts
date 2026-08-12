export type StackPhase =
  | "uninitialized"
  | "stopped"
  | "starting_hiddify"
  | "preparing_runtime"
  | "validating_config"
  | "starting_core"
  | "checking_readiness"
  | "running"
  | "degraded"
  | "stopping"
  | "recovering"
  | "error";

export type ComponentPhase =
  | "unknown"
  | "stopped"
  | "starting"
  | "running"
  | "degraded"
  | "error";

export interface ComponentStatus {
  phase: ComponentPhase;
  message: string | null;
  since: string;
}

export interface ProviderSummary {
  ready: number;
  total: number;
  rules_loaded: number;
  last_refresh: string | null;
}

export interface AppError {
  code: string;
  message_key: string;
  retryable: boolean;
  remediation:
    | "retry"
    | "open_settings"
    | "install_helper"
    | "choose_hiddify_executable"
    | "run_diagnostics"
    | null;
  technical_details: string | null;
  correlation_id: string;
}

export interface StackSnapshot {
  revision: number;
  phase: StackPhase;
  operation_id: string | null;
  hiddify: ComponentStatus;
  mihomo: ComponentStatus;
  tun: ComponentStatus;
  dns: ComponentStatus;
  providers: ProviderSummary;
  exit_ip: string | null;
  backend: "external_hiddify";
  last_error: AppError | null;
  updated_at: string;
}

export interface OperationAccepted {
  operation_id: string;
  already_complete: boolean;
}

export type ExecutableSetting = "auto" | { path: string };
export type LogLevel = "error" | "warn" | "info" | "debug";

export interface AppConfig {
  schema_version: number;
  revision: number;
  hiddify: {
    host: string;
    port: number;
    executable: ExecutableSetting;
    start_timeout_seconds: number;
    stop_with_stack: boolean;
  };
  mihomo: {
    controller_host: string;
    controller_port: number;
    controller_secret: string;
    mixed_port: number;
    dns_port: number;
    tun_name: string;
    log_level: LogLevel;
  };
  rules: {
    refresh_interval_minutes: number;
    upstream_refresh_hours: number;
  };
  behavior: {
    launch_at_login: boolean;
    connect_at_launch: boolean;
    close_to_tray: boolean;
  };
}

export interface ValidationIssue {
  field: string;
  code: string;
  message: string;
}

export interface DirectTarget {
  kind: "domain" | "ip";
  value: string;
}

export interface DirectRule {
  target: DirectTarget;
  resolved_ips: string[];
  created_at: string;
  refreshed_at: string | null;
}

export interface DirectRulesDocument {
  revision: number;
  rules: DirectRule[];
}

export interface RouteTestResult {
  target: string;
  outbound: "direct" | "vpn";
  reason: string;
  matched_rule: string | null;
  reachable: boolean | null;
  tested_at: string;
}

export interface DiagnosticStep {
  id: string;
  label: string;
  status: "pending" | "running" | "passed" | "failed" | "warning";
  detail: string | null;
  started_at: string | null;
  finished_at: string | null;
}

export interface DiagnosticsReport {
  operation_id: string;
  steps: DiagnosticStep[];
  finished: boolean;
}

export interface LogEntry {
  timestamp: string;
  level: string;
  event: string;
  fields: Record<string, string>;
}

export interface BootstrapResult {
  app_version: string;
  platform: string;
  mock_mode: boolean;
  snapshot: StackSnapshot;
  settings: AppConfig;
  direct_rules: DirectRulesDocument;
}

export interface ExportResult {
  path: string;
  files: string[];
}

export interface UpdateStatus {
  available: boolean;
  version: string | null;
  notes: string | null;
}
