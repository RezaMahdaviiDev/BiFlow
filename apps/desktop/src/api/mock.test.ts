import { beforeEach, describe, expect, it } from "vitest";
import { APP_VERSION } from "../version";
import { mockApi, resetMockState } from "./mock";

describe("mock transport", () => {
  beforeEach(() => {
    resetMockState();
  });

  it("bootstraps the current application version", async () => {
    const boot = await mockApi.bootstrap();
    expect(boot.app_version).toBe(APP_VERSION);
    expect(boot.mock_mode).toBe(true);
    expect(boot.cloud_rules.domain_count).toBeGreaterThan(0);
    expect(boot.dependencies).toHaveLength(2);
  });

  it("installs missing third-party apps into the user data path", async () => {
    const result = await mockApi.installDependency("hiddify");
    expect(result.installed).toBe(true);
    const [hiddify] = await mockApi.listDependencies();
    expect(hiddify?.installed).toBe(true);
    expect(hiddify?.path).toContain("biflow");
  });

  it("routes .ir hosts direct and other hosts through the vpn", async () => {
    await expect(mockApi.testRoute("digikala.ir")).resolves.toMatchObject({
      outbound: "direct",
    });
    await expect(mockApi.testRoute("openai.com")).resolves.toMatchObject({
      outbound: "vpn",
      matched_rule: "MATCH",
    });
  });

  it("resyncs cloud rule counts from the fail-safe source", async () => {
    const synced = await mockApi.syncCloudRules();
    expect(synced.source).toBe("jsdelivr");
    expect(synced.last_synced_at).toBeTruthy();
    expect(synced.domain_count).toBeGreaterThan(62_829);
  });
});
