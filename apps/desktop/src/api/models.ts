export type LifecycleBusy =
  | "connecting"
  | "disconnecting"
  | "pausing"
  | "resuming"
  | "reconciling"
  | "applying_rules";

export type OperationStage =
  | "preparing"
  | "starting_hiddify"
  | "starting_openvpn"
  | "preparing_runtime"
  | "validating_config"
  | "starting_core"
  | "checking_readiness"
  | "stopping_core"
  | "stopping_proxy"
  | "cleaning_up"
  | "recovering";

export type StackPhase =
  | "uninitialized"
  | "stopped"
  | "starting_hiddify"
  | "starting_openvpn"
  | "preparing_runtime"
  | "validating_config"
  | "starting_core"
  | "checking_readiness"
  | "running"
  | "paused"
  | "degraded"
  | "stopping"
  | "recovering"
  | "error";

export type ComponentPhase =
  | "unknown"
  | "checking"
  | "stopped"
  | "starting"
  | "running"
  | "degraded"
  | "unavailable"
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
    | "install_dependency"
    | "run_diagnostics"
    | null;
  technical_details: string | null;
  correlation_id: string;
}

export interface StackSnapshot {
  revision: number;
  phase: StackPhase;
  busy?: LifecycleBusy | null;
  operation_stage?: OperationStage | null;
  operation_id: string | null;
  helper: ComponentStatus;
  hiddify: ComponentStatus;
  /** The optional OpenVPN side tunnel. Absent on snapshots from older builds. */
  openvpn: ComponentStatus;
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
export type DirectDnsPreset =
  | "fake_ip"
  | "shecan"
  | "electro"
  | "radar"
  | "mokhaberat"
  | "custom";

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
    direct_dns_preset: DirectDnsPreset;
    direct_dns_servers: string[];
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
  openvpn: OpenVpnConfig;
}

/**
 * The OpenVPN side tunnel that starts after Hiddify.
 *
 * It never owns the default route: only the tunnel's own network and the
 * CIDRs in `tunnel_routes` reach it, so a tunnel that drops cannot take the
 * machine's internet with it.
 */
export interface OpenVpnConfig {
  enabled: boolean;
  /** Fail Connect when the tunnel will not start. Off by default. */
  required: boolean;
  /** Keep the server's scoped routes. A default route is filtered regardless. */
  pull_routes: boolean;
  device: string;
  start_timeout_seconds: number;
  routing_mark: number;
  routing_table: number;
  profile?: string | null;
  auth_file?: string | null;
  executable?: string | null;
  /** Extra CIDRs that reach the tunnel. Never a default route. */
  tunnel_routes: string[];
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
  /** Hosts forced onto the VPN ahead of the bundled Iran list. */
  vpn_rules: DirectRule[];
  /** Hosts routed through the OpenVPN side tunnel. */
  openvpn_rules: DirectRule[];
}

export interface RouteTestResult {
  target: string;
  outbound: "direct" | "vpn" | "openvpn";
  reason: string;
  matched_rule: string | null;
  reachable: boolean | null;
  tested_at: string;
}

export type ReachabilityStatus = "ok" | "slow" | "unreachable";

export interface ReachabilityResult {
  id: string;
  domain: string;
  path: "vpn" | "direct";
  /** True when the probe actually went through the Hiddify proxy. */
  via_proxy: boolean;
  status: ReachabilityStatus;
  latency_ms: number | null;
  detail: string | null;
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
  cloud_rules: CloudRulesStatus;
  dependencies: DependencyStatus[];
  network_status: NetworkStatus;
}

export type InternetState = "checking" | "online" | "offline";

export interface TrafficTotals {
  sent: number;
  received: number;
}

export interface ActiveConnection {
  host: string;
  destination_ip: string;
  outbound: "direct" | "vpn" | "openvpn";
  rule: string;
}

export interface NetworkStatus {
  state: InternetState;
  public_ip: string | null;
  country_code: string | null;
  city: string | null;
  checked_at: string;
  detail: string | null;
}

export interface ExportResult {
  path: string;
  files: string[];
}

export interface DebugLogStatus {
  path: string;
  size_bytes: number;
}

export interface FreshStartReport {
  data_dir: string;
  backup_dir: string;
  cleared: string[];
  preserved: string[];
  stopped: boolean;
  started: boolean;
}

export interface UpdateStatus {
  available: boolean;
  version: string | null;
  notes: string | null;
  app_available: boolean;
  rules_available: boolean;
  thirdparty_available: boolean;
}

export type UpdatePhase =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "downloading"
  | "installing"
  | "restarting"
  | "installed"
  | "failed";

export interface UpdateProgress {
  phase: UpdatePhase;
  percent: number | null;
  version: string | null;
  error: string | null;
  operation_id?: string | null;
  app_available?: boolean;
  rules_available?: boolean;
  thirdparty_available?: boolean;
}

export interface CloudRuleSetStatus {
  id: string;
  kind: "domain" | "ip_cidr";
  entry_count: number;
  source: string;
  sha256: string | null;
}

export interface CloudRulesStatus {
  domain_count: number;
  ip_count: number;
  last_synced_at: string | null;
  source: string;
  snapshot_revision: string | null;
  sets: CloudRuleSetStatus[];
}

export interface DependencyStatus {
  id: "hiddify" | "mihomo";
  name: string;
  installed: boolean;
  version: string | null;
  path: string | null;
}

export interface InstallGuide {
  id: string;
  title: string;
  download_url: string;
  steps: string[];
}

export interface InstallResult {
  id: string;
  installed: boolean;
  path: string | null;
  guide: InstallGuide;
}
