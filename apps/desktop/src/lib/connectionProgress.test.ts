import { describe, expect, it } from "vitest";
import type { StackSnapshot } from "../api/models";
import {
  connectionButtonProgress,
  resolveOperationStage,
} from "./connectionProgress";

const now = new Date().toISOString();

const base = (overrides: Partial<StackSnapshot> = {}): StackSnapshot => ({
  revision: 1,
  phase: "stopped",
  busy: null,
  operation_stage: null,
  operation_id: null,
  helper: { phase: "running", message: null, since: now },
  hiddify: { phase: "stopped", message: null, since: now },
  mihomo: { phase: "stopped", message: null, since: now },
  tun: { phase: "stopped", message: null, since: now },
  dns: { phase: "stopped", message: null, since: now },
  providers: { ready: 0, total: 0, rules_loaded: 0, last_refresh: null },
  exit_ip: null,
  backend: "external_hiddify",
  last_error: null,
  updated_at: now,
  ...overrides,
});

describe("connectionButtonProgress", () => {
  it("keeps idle labels until a matching operation starts", () => {
    const snapshot = base();
    expect(connectionButtonProgress(snapshot, "connect")).toEqual({
      labelKey: "connect",
      percent: 0,
      processing: false,
    });
    expect(connectionButtonProgress(snapshot, "disconnect").processing).toBe(
      false,
    );
  });

  it("follows Connect stages from backend milestones", () => {
    const start = base({
      busy: "connecting",
      operation_stage: "starting_hiddify",
      phase: "starting_hiddify",
      operation_id: "op-1",
    });
    expect(connectionButtonProgress(start, "connect")).toEqual({
      labelKey: "stages.startHiddify",
      percent: 25,
      processing: true,
    });
    expect(connectionButtonProgress(start, "disconnect").processing).toBe(
      false,
    );

    const mihomo = {
      ...start,
      phase: "starting_core" as const,
      operation_stage: "starting_core" as const,
    };
    expect(connectionButtonProgress(mihomo, "connect")).toEqual({
      labelKey: "stages.startMihomo",
      percent: 70,
      processing: true,
    });
  });

  it("uses install milestones before the stack start stages", () => {
    expect(
      resolveOperationStage(base({ busy: "connecting" }), "hiddify"),
    ).toEqual({
      percent: 16,
      labelKey: "stages.installHiddify",
    });
  });

  it("maps Disconnect and Pause to their stop stages", () => {
    const disconnecting = base({
      phase: "stopping",
      busy: "disconnecting",
      operation_stage: "stopping_proxy",
      hiddify: { phase: "running", message: null, since: now },
    });
    expect(connectionButtonProgress(disconnecting, "disconnect")).toEqual({
      labelKey: "stages.stopHiddify",
      percent: 65,
      processing: true,
    });

    const pausing = base({
      phase: "stopping",
      busy: "pausing",
      operation_stage: "stopping_core",
      mihomo: { phase: "running", message: null, since: now },
    });
    expect(connectionButtonProgress(pausing, "pause")).toEqual({
      labelKey: "stages.stopMihomo",
      percent: 35,
      processing: true,
    });
  });

  it("fills to 100% on the last published stage before idle", () => {
    const ready = base({
      phase: "checking_readiness",
      busy: "resuming",
      operation_stage: "checking_readiness",
    });
    expect(connectionButtonProgress(ready, "resume").percent).toBe(85);
  });

  it("shows an optimistic preparing fill before the first snapshot", () => {
    expect(
      connectionButtonProgress(base(), "connect", null, true),
    ).toMatchObject({
      labelKey: "stages.preparing",
      processing: true,
    });
  });
});
