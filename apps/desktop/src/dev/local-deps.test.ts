import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  detectLocalHiddify,
  detectLocalMihomo,
  HIDDIFY_NAMES,
  MIHOMO_NAMES,
} from "../../local-deps";

describe("local dependency detection", () => {
  it("treats hiddify and mihomo in a search directory as installed", () => {
    const directory = mkdtempSync(join(tmpdir(), "biflow-deps-"));
    writeFileSync(join(directory, "hiddify"), "ok");
    writeFileSync(join(directory, "mihomo"), "ok");
    expect(HIDDIFY_NAMES).toContain("hiddify");
    expect(MIHOMO_NAMES).toContain("mihomo");
    expect(detectLocalHiddify([directory])).toBe(true);
    expect(detectLocalMihomo([directory])).toBe(true);
  });

  it("does not treat an empty directory as an install", () => {
    const directory = mkdtempSync(join(tmpdir(), "biflow-empty-"));
    expect(detectLocalHiddify([directory])).toBe(false);
    expect(detectLocalMihomo([directory])).toBe(false);
  });
});
