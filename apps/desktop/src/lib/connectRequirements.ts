import type { DependencyStatus, StackSnapshot } from "../api/models";

export type ConnectRequirement = "helper" | "hiddify" | "mihomo";

export function helperNeedsInstall(
  snapshot: StackSnapshot | null | undefined,
): boolean {
  const phase = snapshot?.helper.phase;
  return phase === "unavailable" || phase === "error";
}

export function missingConnectRequirements(
  snapshot: StackSnapshot | null | undefined,
  dependencies: DependencyStatus[],
): ConnectRequirement[] {
  const missing: ConnectRequirement[] = [];
  if (helperNeedsInstall(snapshot)) {
    missing.push("helper");
  }
  const hiddify = dependencies.find((item) => item.id === "hiddify");
  const mihomo = dependencies.find((item) => item.id === "mihomo");
  if (hiddify && !hiddify.installed) {
    missing.push("hiddify");
  }
  if (mihomo && !mihomo.installed) {
    missing.push("mihomo");
  }
  return missing;
}
