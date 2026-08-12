import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mockApi } from "./mock";
import type {
  AppConfig,
  BootstrapResult,
  CloudRulesStatus,
  DependencyStatus,
  DiagnosticsReport,
  DirectRulesDocument,
  ExportResult,
  InstallGuide,
  InstallResult,
  LogEntry,
  OperationAccepted,
  RouteTestResult,
  StackSnapshot,
  UpdateStatus,
  ValidationIssue,
} from "./models";

const native =
  typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;

export const desktop = {
  bootstrap(): Promise<BootstrapResult> {
    return native ? invoke("bootstrap_app") : mockApi.bootstrap();
  },
  getSnapshot(): Promise<StackSnapshot> {
    return native ? invoke("get_stack_snapshot") : mockApi.getSnapshot();
  },
  start(): Promise<OperationAccepted> {
    return native ? invoke("start_stack") : mockApi.start();
  },
  stop(): Promise<OperationAccepted> {
    return native ? invoke("stop_stack") : mockApi.stop();
  },
  cancel(operationId: string): Promise<boolean> {
    return native
      ? invoke("cancel_operation", { operationId })
      : mockApi.cancel(operationId);
  },
  getSettings(): Promise<AppConfig> {
    return native ? invoke("get_settings") : mockApi.getSettings();
  },
  validateSettings(draft: AppConfig): Promise<ValidationIssue[]> {
    return native
      ? invoke("validate_settings", { draft })
      : mockApi.validateSettings(draft);
  },
  saveSettings(draft: AppConfig, expectedRevision: number): Promise<AppConfig> {
    return native
      ? invoke("save_settings", { draft, expectedRevision })
      : mockApi.saveSettings(draft, expectedRevision);
  },
  listRules(): Promise<DirectRulesDocument> {
    return native ? invoke("list_direct_rules") : mockApi.listRules();
  },
  addRule(
    input: string,
    expectedRevision: number,
  ): Promise<DirectRulesDocument> {
    return native
      ? invoke("add_direct_rule", { input, expectedRevision })
      : mockApi.addRule(input, expectedRevision);
  },
  removeRule(
    input: string,
    expectedRevision: number,
  ): Promise<DirectRulesDocument> {
    return native
      ? invoke("remove_direct_rule", { input, expectedRevision })
      : mockApi.removeRule(input, expectedRevision);
  },
  refreshRules(): Promise<DirectRulesDocument> {
    return native ? invoke("refresh_direct_rules") : mockApi.refreshRules();
  },
  getCloudRules(): Promise<CloudRulesStatus> {
    return native ? invoke("get_cloud_rules_status") : mockApi.getCloudRules();
  },
  syncCloudRules(): Promise<CloudRulesStatus> {
    return native ? invoke("sync_cloud_rules") : mockApi.syncCloudRules();
  },
  listDependencies(): Promise<DependencyStatus[]> {
    return native ? invoke("list_dependencies") : mockApi.listDependencies();
  },
  installDependency(id: string): Promise<InstallResult> {
    return native
      ? invoke("install_dependency", { id })
      : mockApi.installDependency(id);
  },
  getInstallGuide(id: string): Promise<InstallGuide> {
    return native
      ? invoke("get_install_guide", { id })
      : mockApi.getInstallGuide(id);
  },
  openUrl(url: string): Promise<void> {
    return native ? invoke("open_external_url", { url }) : mockApi.openUrl(url);
  },
  testRoute(target: string): Promise<RouteTestResult> {
    return native
      ? invoke("test_route", { target })
      : mockApi.testRoute(target);
  },
  diagnostics(): Promise<DiagnosticsReport> {
    return native ? invoke("run_full_diagnostics") : mockApi.diagnostics();
  },
  queryLogs(): Promise<LogEntry[]> {
    return native
      ? invoke("query_logs", { maximum: 500 })
      : mockApi.queryLogs();
  },
  exportBundle(): Promise<ExportResult> {
    return native ? invoke("export_support_bundle") : mockApi.exportBundle();
  },
  checkUpdate(): Promise<UpdateStatus> {
    return native ? invoke("check_for_update") : mockApi.checkUpdate();
  },
  async subscribe(
    listener: (snapshot: StackSnapshot) => void,
  ): Promise<() => void> {
    if (!native) return mockApi.subscribe(listener);
    const unlisten = await listen<StackSnapshot>("stack-snapshot", (event) =>
      listener(event.payload),
    );
    return unlisten;
  },
};

export type { AppConfig, StackSnapshot } from "./models";
