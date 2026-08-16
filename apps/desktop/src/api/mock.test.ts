import { beforeEach, describe, expect, it, vi } from "vitest";
import { APP_VERSION } from "../version";
import { mockApi, resetMockState } from "./mock";

describe("mock transport", () => {
  beforeEach(() => {
    sessionStorage.removeItem("biflow-mock-force-missing-helper");
    resetMockState();
  });

  it("bootstraps the current application version", async () => {
    const boot = await mockApi.bootstrap();
    expect(boot.app_version).toBe(APP_VERSION);
    expect(boot.mock_mode).toBe(true);
    expect(boot.cloud_rules.domain_count).toBeGreaterThan(0);
    expect(boot.dependencies).toHaveLength(2);
    expect(boot.dependencies.every((item) => item.installed === false)).toBe(
      true,
    );
  });

  it("installs missing third-party apps into the user data path", async () => {
    const result = await mockApi.installDependency("hiddify");
    expect(result.installed).toBe(true);
    const [hiddify] = await mockApi.listDependencies();
    expect(hiddify?.installed).toBe(true);
    expect(hiddify?.path).toContain("biflow");
    expect(localStorage.getItem("biflow-mock-installed-deps")).toMatch(
      /"installed":true/,
    );
  });

  it("installs a missing mock helper", async () => {
    sessionStorage.setItem("biflow-mock-force-missing-helper", "1");
    resetMockState();
    const boot = await mockApi.bootstrap();
    expect(boot.snapshot.helper.phase).toBe("unavailable");
    await mockApi.installHelper();
    const snapshot = await mockApi.getSnapshot();
    expect(snapshot.helper.phase).toBe("running");
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

  it("resyncs cloud rule counts from the BiFlow snapshot", async () => {
    const synced = await mockApi.syncCloudRules();
    expect(synced.source).toBe("devlifeX/BiFlow");
    expect(synced.snapshot_revision).toBeTruthy();
    expect(synced.last_synced_at).toBeTruthy();
    expect(synced.domain_count).toBeGreaterThan(62_829);
  });

  it("reports an available mock update when session storage requests it", async () => {
    sessionStorage.setItem("biflow-mock-update-available", "1");
    await expect(mockApi.checkUpdate()).resolves.toMatchObject({
      available: true,
      version: "9.9.9",
    });
  });

  it("emits download progress during mock install", async () => {
    sessionStorage.setItem("biflow-mock-update-available", "1");
    const phases: string[] = [];
    const unsubscribe = mockApi.subscribeUpdateProgress((progress) => {
      phases.push(progress.phase);
    });
    await mockApi.installUpdate();
    unsubscribe();
    expect(phases).toContain("downloading");
    expect(phases.at(-1)).toBe("restarting");
  });

  it("rejects a second connection operation while one is running", async () => {
    const first = mockApi.start();
    await expect(mockApi.stop()).rejects.toThrow(/already in progress/);
    await expect(mockApi.pause()).rejects.toThrow(/already in progress/);
    await first;
    await vi.waitFor(async () => {
      const snapshot = await mockApi.getSnapshot();
      expect(snapshot.phase).toBe("running");
      expect(snapshot.busy).toBeNull();
    });
  });

  it("fails mock install when signature verification is forced to fail", async () => {
    sessionStorage.setItem("biflow-mock-update-available", "1");
    sessionStorage.setItem("biflow-mock-update-fail", "1");
    await expect(mockApi.installUpdate()).rejects.toThrow(
      /signature verification failed/i,
    );
  });
});
