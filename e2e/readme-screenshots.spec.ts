import { expect, test, type Page } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { join } from "node:path";

const screenshotDir = join(process.cwd(), "docs/screenshots");

async function openAdvanced(page: Page) {
  await page.goto("/");
  await page.waitForFunction(
    "typeof window.__BIFLOW_RESET_MOCK === 'function'",
  );
  await page.evaluate(() => {
    window.__BIFLOW_RESET_MOCK?.();
    localStorage.setItem("biflow-ui-mode-v1", "advanced");
  });
  await page.reload();
  await expect(page.getByRole("radio", { name: "Advanced" })).toBeVisible();
}

async function connectStack(page: Page) {
  const installButtons = page.getByRole("button", {
    name: "Install",
    exact: true,
  });
  const count = await installButtons.count();
  for (let index = 0; index < count; index += 1) {
    await page
      .getByRole("button", { name: "Install", exact: true })
      .first()
      .click();
  }
  await page.getByRole("button", { name: "Connect", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Protected split routing is active" }),
  ).toBeVisible();
}

test.describe("readme screenshots", () => {
  test.skip(
    !process.env.BIFLOW_CAPTURE_README,
    "set BIFLOW_CAPTURE_README=1 to refresh docs/screenshots",
  );

  test("captures desktop and mobile product shots", async ({ page }) => {
    mkdirSync(screenshotDir, { recursive: true });
    await page.setViewportSize({ width: 1120, height: 760 });
    await openAdvanced(page);
    await connectStack(page);
    await page.screenshot({
      path: join(screenshotDir, "desktop.png"),
      animations: "disabled",
    });

    // Diagnostics with the grouped, sortable live-connections table.
    await page.getByRole("button", { name: "Diagnostics" }).click();
    const connections = page.getByTestId("live-connections");
    await expect(connections).toBeVisible();
    await expect(connections.getByText("digikala.ir")).toBeVisible();
    await connections.scrollIntoViewIfNeeded();
    await page.screenshot({
      path: join(screenshotDir, "diagnostics.png"),
      animations: "disabled",
    });

    await page.getByRole("button", { name: "Dashboard" }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.getByTestId("bottom-nav")).toBeVisible();
    await page.screenshot({
      path: join(screenshotDir, "mobile.png"),
      animations: "disabled",
    });
  });
});
