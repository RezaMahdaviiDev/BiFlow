import { expect, test, type Page } from "@playwright/test";

async function openFresh(page: Page, mode: "basic" | "advanced" = "advanced") {
  await page.goto("/");
  await page.waitForFunction(
    "typeof window.__BIFLOW_RESET_MOCK === 'function'",
  );
  await page.evaluate(
    ([nextMode]) => {
      window.__BIFLOW_RESET_MOCK?.();
      localStorage.setItem("biflow-ui-mode-v1", nextMode);
    },
    [mode],
  );
  await page.reload();
  await expect(page.getByRole("radio", { name: "Advanced" })).toBeVisible();
}

function connectButton(page: Page) {
  return page.getByRole("button", { name: "Connect", exact: true });
}

async function expectNoDocumentOverflow(page: Page) {
  const overflow = await page.evaluate(() => ({
    horizontal:
      document.documentElement.scrollWidth >
      document.documentElement.clientWidth,
    vertical:
      document.documentElement.scrollHeight >
      document.documentElement.clientHeight,
  }));
  expect(overflow.horizontal).toBe(false);
  expect(overflow.vertical).toBe(false);
}

async function walkAdvancedPages(page: Page, labels: string[]) {
  for (const name of labels) {
    await page.getByRole("button", { name }).click();
    await expectNoDocumentOverflow(page);
  }
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
    await expect(statusBar).toContainText("Sent: 1.00 MiB");
    await expect(statusBar).toContainText("Received: 2.00 MiB");
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

    await connectButton(page).click();
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

    await page.getByRole("button", { name: "Pause" }).click();
    await expect(
      page.getByRole("heading", { name: "Split routing is paused" }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Resume" })).toBeVisible();
    await page.getByRole("button", { name: "Resume" }).click();
    await expect(
      page.getByRole("heading", { name: "Protected split routing is active" }),
    ).toBeVisible();

    await page.getByRole("button", { name: "Disconnect" }).click();
    await expect(
      page.getByRole("heading", { name: "Ready when you are" }),
    ).toBeVisible();
    await expect(page.locator(".traffic-flow-route")).toHaveCount(0);
  });

  test("disables lifecycle controls after the first Connect click", async ({
    page,
  }) => {
    await openFresh(page);
    const installButtons = page.getByRole("button", {
      name: "Install",
      exact: true,
    });
    await installButtons.nth(0).click();
    await page.getByRole("button", { name: "Install", exact: true }).click();
    const connect = page.locator("[data-connection-action='connect']");
    await expect(connect).toHaveAttribute("data-connect-glow", "available");
    await page.evaluate(() => {
      const seen: string[] = [];
      const record = () => {
        const label = document.querySelector(
          "[data-connection-action='connect'] .connection-action-label",
        );
        const text = label?.textContent?.trim();
        if (text && seen.at(-1) !== text) {
          seen.push(text);
        }
      };
      record();
      const observer = new MutationObserver(record);
      observer.observe(document.body, {
        subtree: true,
        childList: true,
        characterData: true,
      });
      window.__BIFLOW_STAGE_SEEN = seen;
      window.__BIFLOW_STAGE_STOP = () => observer.disconnect();
    });
    await connect.click();
    await expect(connect).toBeDisabled();
    await expect(connect).toHaveAttribute("data-processing", "true");
    await expect(connect).toHaveAttribute("data-connect-glow", "off");
    await connect.click({ force: true });
    await expect(
      page.getByRole("heading", { name: "Protected split routing is active" }),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Pause" })).toBeEnabled();
    await expect(
      page.getByRole("button", { name: "Disconnect" }),
    ).toBeEnabled();
    const stages = await page.evaluate(() => {
      window.__BIFLOW_STAGE_STOP?.();
      return window.__BIFLOW_STAGE_SEEN ?? [];
    });
    expect(stages).toEqual(
      expect.arrayContaining(["Start Hiddify", "Start Mihomo"]),
    );
  });

  test("connect installs missing apps before starting the stack", async ({
    page,
  }) => {
    await openFresh(page);
    await expect(
      page.getByRole("button", { name: "Install", exact: true }),
    ).toHaveCount(2);
    await connectButton(page).click();
    await expect(
      page.getByRole("heading", { name: "Protected split routing is active" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Install", exact: true }),
    ).toHaveCount(0);
  });

  test("installs a missing helper from the advanced dashboard", async ({
    page,
  }) => {
    await openFresh(page);
    await page.evaluate(() => {
      sessionStorage.setItem("biflow-mock-force-missing-helper", "1");
      window.__BIFLOW_RESET_MOCK?.();
    });
    await page.reload();
    await expect(page.getByText("BiFlow")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Setup needs attention" }),
    ).toBeVisible();
    await expect(
      page.getByText("Helper service is not installed or running"),
    ).toBeVisible();
    const installButtons = page.getByRole("button", {
      name: "Install",
      exact: true,
    });
    await expect(installButtons).toHaveCount(3);
    await installButtons.first().click();
    await expect(page.getByText("Mock helper is ready")).toBeVisible();
    await expect(installButtons).toHaveCount(2);
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

    await page.getByLabel("Domain or IP").fill("aparat.com");
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

    await expect(
      page.getByRole("heading", { name: "Permanent debug.log", exact: true }),
    ).toBeVisible();
    await expect(page.getByTestId("debug-log-size")).toBeVisible();
    await page.getByRole("button", { name: "Show file" }).click();
    await expect(
      page.getByText("Opened the folder containing debug.log."),
    ).toBeVisible();
    page.once("dialog", (dialog) => dialog.accept());
    await page.getByRole("button", { name: "Delete log" }).click();
    await expect(
      page.getByText(/previous log content was deleted/i),
    ).toBeVisible();
    await expect(page.getByTestId("debug-log-size")).toHaveText("512 B");
    await page.getByRole("button", { name: "Export" }).click();
    await expect(page.getByText(/Included:.*debug\.log/)).toBeVisible();
  });

  test("rings the window green while connected and amber while paused", async ({
    page,
  }) => {
    await openFresh(page);
    const shell = page.locator("[data-connection-glow]");
    await expect(shell).toHaveAttribute("data-connection-glow", "none");

    const installButtons = page.getByRole("button", {
      name: "Install",
      exact: true,
    });
    await expect(installButtons).toHaveCount(2);
    await installButtons.nth(0).click();
    await expect(installButtons).toHaveCount(1);
    await installButtons.first().click();
    await expect(installButtons).toHaveCount(0);

    await connectButton(page).click();
    await expect(shell).toHaveAttribute("data-connection-glow", "active");
    await expect(shell).toHaveClass(/connection-glow-active/);

    await page.getByRole("button", { name: "Pause" }).click();
    await expect(shell).toHaveAttribute("data-connection-glow", "paused");
    await expect(shell).toHaveClass(/connection-glow-paused/);

    await page.getByRole("button", { name: "Resume" }).click();
    await expect(shell).toHaveAttribute("data-connection-glow", "active");

    await page.getByRole("button", { name: "Disconnect" }).click();
    await expect(shell).toHaveAttribute("data-connection-glow", "none");
    await expect(shell).not.toHaveClass(/connection-glow\b/);
  });

  test("pins an Iran host onto the VPN and back to direct", async ({
    page,
  }) => {
    await openFresh(page);
    await page.getByRole("button", { name: "Diagnostics" }).click();
    await page.getByLabel("Test IP or domain").fill("https://www.rade.ir/");
    await page.getByRole("button", { name: "Test flow" }).click();
    // The bundled Iran list keeps every .ir host direct until it is pinned.
    await expect(page.getByText("www.rade.ir → DIRECT")).toBeVisible();

    await page
      .getByRole("button", { name: /Add www\.rade\.ir to VPN/ })
      .click();
    await expect(page.getByText("www.rade.ir → VPN")).toBeVisible();

    // The pin shows on the rules page with a VPN badge.
    await page.getByRole("button", { name: "Direct rules" }).click();
    const pinned = page.getByRole("listitem").filter({
      has: page.getByText("rade.ir", { exact: true }),
    });
    await expect(pinned).toContainText("VPN");

    // Moving it back leaves the bundled list to decide again.
    await pinned.getByRole("button", { name: /to direct/ }).click();
    await page.getByRole("button", { name: "Diagnostics" }).click();
    await page.getByLabel("Test IP or domain").fill("www.rade.ir");
    await page.getByRole("button", { name: "Test flow" }).click();
    await expect(page.getByText("www.rade.ir → DIRECT")).toBeVisible();
  });

  test("restarts Hiddify on clean state from diagnostics", async ({ page }) => {
    await openFresh(page);
    await page.getByRole("button", { name: "Diagnostics" }).click();
    await expect(
      page.getByRole("heading", { name: "Fresh Hiddify start", exact: true }),
    ).toBeVisible();

    page.once("dialog", (dialog) => dialog.dismiss());
    await page.getByRole("button", { name: "Fresh Hiddify start" }).click();
    await expect(
      page.getByText(/Hiddify restarted on clean runtime state/),
    ).toHaveCount(0);

    page.once("dialog", (dialog) => dialog.accept());
    await page.getByRole("button", { name: "Fresh Hiddify start" }).click();
    await expect(
      page.getByText(/Hiddify restarted on clean runtime state/),
    ).toBeVisible();
    await expect(page.getByText(/Kept: db\.sqlite/)).toBeVisible();
    await expect(page.getByText(/Backup: .*hiddify-/)).toBeVisible();
  });

  test("keeps the fixed viewport free of document overflow in English and Persian", async ({
    page,
  }) => {
    await openFresh(page);
    await walkAdvancedPages(page, [
      "Dashboard",
      "Direct rules",
      "Diagnostics",
      "Settings",
      "About",
    ]);

    await page.evaluate(() => {
      localStorage.setItem("biflow-language", "fa");
    });
    await page.reload();
    await expect(page.getByText("BiFlow")).toBeVisible();
    await walkAdvancedPages(page, [
      "داشبورد",
      "قوانین مستقیم",
      "عیب‌یابی",
      "تنظیمات",
      "درباره",
    ]);
  });

  test("starts in Basic mode on first launch and can return to Advanced", async ({
    page,
  }) => {
    await openFresh(page, "basic");
    await expect(connectButton(page)).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Direct rules" }),
    ).toHaveCount(0);
    await expectNoDocumentOverflow(page);

    await connectButton(page).click();
    await expect(
      page.getByRole("heading", { name: "Protected split routing is active" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Disconnect" }).click();

    await page.getByRole("radio", { name: "Advanced" }).click();
    await expect(
      page.getByRole("heading", { name: "Ready when you are" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Direct rules" }),
    ).toBeVisible();
  });

  test("hides advanced chrome in Basic mode and can return to Advanced", async ({
    page,
  }) => {
    await openFresh(page);
    await page.getByRole("radio", { name: "Basic" }).click();
    await expect(connectButton(page)).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Direct rules" }),
    ).toHaveCount(0);
    await expectNoDocumentOverflow(page);

    await connectButton(page).click();
    await expect(
      page.getByRole("heading", { name: "Protected split routing is active" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Pause" }).click();
    await expect(
      page.getByRole("heading", { name: "Split routing is paused" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Resume" }).click();
    await expect(
      page.getByRole("heading", { name: "Protected split routing is active" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Disconnect" }).click();
    await expect(connectButton(page)).toBeVisible();

    await page.getByRole("radio", { name: "Advanced" }).click();
    await expect(
      page.getByRole("heading", { name: "Ready when you are" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Direct rules" }),
    ).toBeVisible();
  });

  test("offers select all, copy, cut, and paste on text inputs", async ({
    page,
  }) => {
    await openFresh(page);
    await page.getByRole("button", { name: "Diagnostics" }).click();
    const field = page.getByLabel("Test IP or domain");
    await field.fill("example.ir");
    await field.evaluate((node) => {
      if (node instanceof HTMLInputElement) {
        node.focus();
        node.setSelectionRange(0, 0);
      }
    });
    await field.click({ button: "right" });
    const menu = page.getByTestId("input-context-menu");
    await expect(menu).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Select All" }),
    ).toBeEnabled();
    await expect(menu.getByRole("menuitem", { name: "Copy" })).toBeDisabled();
    await expect(menu.getByRole("menuitem", { name: "Cut" })).toBeDisabled();
    await expect(menu.getByRole("menuitem", { name: "Paste" })).toBeEnabled();
    await menu.getByRole("menuitem", { name: "Select All" }).click();
    await expect(field).toHaveJSProperty("selectionStart", 0);
    await expect(field).toHaveJSProperty("selectionEnd", "example.ir".length);
    await field.click({ button: "right" });
    await expect(page.getByRole("menuitem", { name: "Copy" })).toBeEnabled();
    await expect(page.getByRole("menuitem", { name: "Cut" })).toBeEnabled();
  });

  test("blocks the document context menu", async ({ page }) => {
    await openFresh(page);
    const prevented = await page.evaluate(() => {
      const event = new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
      });
      document.dispatchEvent(event);
      return event.defaultPrevented;
    });
    expect(prevented).toBe(true);
  });

  test("shows About author, version, and update check", async ({ page }) => {
    await openFresh(page);
    await page.getByRole("button", { name: "About" }).click();
    await expect(page.getByText("Dariush Vesal")).toBeVisible();
    await expect(page.getByText(/Version \d+\.\d+\.\d+/)).toBeVisible();
    await page.getByRole("button", { name: "Check for updates" }).click();
    await expect(page.getByText(/latest published version/i)).toBeVisible();
    await expectNoDocumentOverflow(page);

    await page.evaluate(() =>
      sessionStorage.setItem("biflow-mock-update-available", "1"),
    );
    await page.getByRole("button", { name: "Check for updates" }).click();
    await expect(page.getByText(/Version 9\.9\.9 is available/i)).toBeVisible();
    await page.getByRole("button", { name: /Install update 9\.9\.9/i }).click();
    await expect(page.getByRole("progressbar")).toBeVisible();
    await expect(
      page.getByText("an update is already in progress"),
    ).toHaveCount(0);
  });

  test("pins a subdomain to the registrable root while connected", async ({
    page,
  }) => {
    await openFresh(page);
    await page.getByRole("button", { name: "Direct rules" }).click();
    await page.getByLabel("Domain or IP").fill("api.shop.example.com");
    await page.getByRole("button", { name: "Add rule" }).click();
    await expect(page.getByText("example.com").first()).toBeVisible();
    await expect(page.getByText("api.shop.example.com")).toHaveCount(0);

    await page.getByRole("button", { name: "Diagnostics" }).click();
    await page.getByLabel("Test IP or domain").fill("www.technolife.com");
    await page.getByRole("button", { name: "Test flow" }).click();
    await expect(page.getByText("www.technolife.com → DIRECT")).toBeVisible();
  });
});
