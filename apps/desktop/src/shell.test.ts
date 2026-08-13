import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { installContextMenuGuard } from "./installContextMenuGuard";

const css = readFileSync(path.join(process.cwd(), "src/index.css"), "utf8");
const guard = readFileSync(
  path.join(process.cwd(), "src/installContextMenuGuard.ts"),
  "utf8",
);
const main = readFileSync(path.join(process.cwd(), "src/main.tsx"), "utf8");

describe("fixed desktop shell", () => {
  it("locks html, body, and #root to the viewport without outer scroll", () => {
    expect(css).toMatch(/html,\s*body,\s*#root[\s\S]*overflow:\s*hidden/);
    expect(css).toMatch(/height:\s*100%/);
  });

  it("disables selection on chrome while keeping inputs and diagnostics selectable", () => {
    expect(css).toMatch(/user-select:\s*none/);
    expect(css).toMatch(/input,[\s\S]*user-select:\s*text/);
    expect(css).toMatch(/\.diagnostics-selectable/);
  });

  it("blocks the native context menu before React renders", () => {
    expect(guard).toMatch(/addEventListener\(\s*["']contextmenu["']/);
    expect(guard).toMatch(/preventDefault\(\)/);
    expect(main).toMatch(/installContextMenuGuard\(\)/);
    expect(main.indexOf("installContextMenuGuard();")).toBeLessThan(
      main.indexOf('createRoot(document.getElementById("root")'),
    );
  });

  it("prevents contextmenu events at runtime", () => {
    installContextMenuGuard();
    const event = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
    });
    document.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });
});
