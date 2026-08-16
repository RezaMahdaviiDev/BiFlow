import { describe, expect, it } from "vitest";
import type { StackSnapshot } from "../api/models";
import { controlsLocked } from "./lifecycle";

const snapshot = (overrides: Partial<StackSnapshot> = {}): StackSnapshot =>
  ({
    revision: 1,
    phase: "stopped",
    busy: null,
    operation_id: null,
    ...overrides,
  }) as StackSnapshot;

describe("controlsLocked", () => {
  it("locks immediately while a store action is pending", () => {
    expect(controlsLocked(snapshot(), true)).toBe(true);
  });

  it("locks for every lifecycle busy state", () => {
    expect(controlsLocked(snapshot({ busy: "connecting" }), false)).toBe(true);
    expect(controlsLocked(snapshot({ busy: "disconnecting" }), false)).toBe(
      true,
    );
    expect(controlsLocked(snapshot({ busy: "pausing" }), false)).toBe(true);
    expect(controlsLocked(snapshot({ busy: "resuming" }), false)).toBe(true);
  });

  it("unlocks after success, failure, or a cleared timeout", () => {
    expect(controlsLocked(snapshot({ phase: "running" }), false)).toBe(false);
    expect(controlsLocked(snapshot({ phase: "error" }), false)).toBe(false);
    expect(controlsLocked(snapshot({ phase: "stopped" }), false)).toBe(false);
    expect(controlsLocked(snapshot({ phase: "paused" }), false)).toBe(false);
  });
});
