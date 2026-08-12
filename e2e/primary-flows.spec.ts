import { expect, test, type Page } from "@playwright/test";

async function openFresh(page: Page) {
  await page.goto("/");
  await page.waitForFunction(
    "typeof window.__BIFLOW_RESET_MOCK === 'function'",
  );
  await page.evaluate("window.__BIFLOW_RESET_MOCK()");
  await page.reload();
  await expect(page.getByText("BiFlow")).toBeVisible();
}

test.describe("primary BiFlow flows", () => {
  test("installs missing apps, connects, and splits traffic", async ({
    page,
  }) => {
    await openFresh(page);
    await expect(
      page.getByRole("heading", { name: "Ready when you are" }),
    ).toBeVisible();
    const statusBar = page.locator("footer[role='status']");
    await expect(statusBar).toContainText("Internet connected");
    await expect(statusBar).toContainText("198.51.100.24");
    await expect(statusBar).toContainText("🇮🇷");
    await expect(page.getByText("unknown", { exact: true })).toHaveCount(0);

    const installButtons = page.getByRole("button", {
      name: "Install",
      exact: true,
    });
    await expect(installButtons).toHaveCount(2);
    await installButtons.nth(0).click();
    await expect(
      page.getByRole("button", { name: "Install", exact: true }),
    ).toHaveCount(1);
    await page.getByRole("button", { name: "Install", exact: true }).click();
    await expect(
      page.getByRole("button", { name: "Install", exact: true }),
    ).toHaveCount(0);

    await page.getByRole("button", { name: "Connect" }).click();
    await expect(
      page.getByRole("heading", { name: "Protected split routing is active" }),
    ).toBeVisible();
    await expect(page.getByText("203.0.113.42")).toBeVisible();
    await expect(
      page.getByRole("img", {
        name: /traffic leaving this device and splitting/i,
      }),
    ).toBeVisible();
    await expect(page.locator(".traffic-flow-route")).toHaveCount(2);

    await page.getByRole("button", { name: "Disconnect" }).click();
    await expect(
      page.getByRole("heading", { name: "Ready when you are" }),
    ).toBeVisible();
    await expect(page.locator(".traffic-flow-route")).toHaveCount(0);
  });

  test("shows cloud rule counts and adds a custom direct rule", async ({
    page,
  }) => {
    await openFresh(page);
    await page.getByRole("button", { name: "Direct rules" }).click();
    await expect(
      page.getByRole("heading", { name: "Direct rules" }),
    ).toBeVisible();
    await expect(
      page.getByText("62,828").or(page.getByText("62828")),
    ).toBeVisible();
    await expect(
      page.getByText("2,906").or(page.getByText("2906")),
    ).toBeVisible();

    await page.getByRole("button", { name: "Update from cloud" }).click();
    await expect(
      page.getByText("63,104").or(page.getByText("63104")),
    ).toBeVisible();

    await page.getByLabel("Exact domain or IP").fill("aparat.com");
    await page.getByRole("button", { name: "Add rule" }).click();
    await expect(page.getByText("aparat.com")).toBeVisible();
  });

  test("diagnoses whether a host is direct or vpn", async ({ page }) => {
    await openFresh(page);
    await page.getByRole("button", { name: "Diagnostics" }).click();
    await page.getByLabel("Test IP or domain").fill("openai.com");
    await page.getByRole("button", { name: "Test flow" }).click();
    await expect(page.getByText("openai.com → VPN")).toBeVisible();

    await page.getByLabel("Test IP or domain").fill("example.ir");
    await page.getByRole("button", { name: "Test flow" }).click();
    await expect(page.getByText("example.ir → DIRECT")).toBeVisible();
  });
});
