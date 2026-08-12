import { describe, expect, it } from "vitest";
import { APP_VERSION } from "./version";

describe("application version", () => {
  it("is a semver value injected from the version file", () => {
    expect(APP_VERSION).toMatch(/^\d+\.\d+\.\d+$/);
  });
});
