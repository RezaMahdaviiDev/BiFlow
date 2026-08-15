import { describe, expect, it } from "vitest";
import type { DependencyStatus, StackSnapshot } from "../api/models";
import { missingConnectRequirements } from "./connectRequirements";

const snapshot = (phase: StackSnapshot["helper"]["phase"]): StackSnapshot =>
  ({
    helper: { phase, message: null, since: "now" },
  }) as StackSnapshot;

const deps = (hiddify: boolean, mihomo: boolean): DependencyStatus[] => [
  {
    id: "hiddify",
    name: "Hiddify",
    installed: hiddify,
    version: null,
    path: null,
  },
  {
    id: "mihomo",
    name: "Mihomo",
    installed: mihomo,
    version: null,
    path: null,
  },
];

describe("missingConnectRequirements", () => {
  it("installs helper, Hiddify, then Mihomo in that order", () => {
    expect(
      missingConnectRequirements(snapshot("unavailable"), deps(false, false)),
    ).toEqual(["helper", "hiddify", "mihomo"]);
  });

  it("skips services that are already present", () => {
    expect(
      missingConnectRequirements(snapshot("running"), deps(true, true)),
    ).toEqual([]);
  });

  it("does not invent missing apps when the dependency list is empty", () => {
    expect(missingConnectRequirements(snapshot("running"), [])).toEqual([]);
  });
});
