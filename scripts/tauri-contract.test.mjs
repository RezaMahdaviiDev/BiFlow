import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function rustSources(directory) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (
      entry.name === "target" ||
      entry.name === "node_modules" ||
      entry.name === ".git"
    ) {
      continue;
    }
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...rustSources(path));
    } else if (entry.name.endsWith(".rs")) {
      files.push(path);
    }
  }
  return files;
}

describe("Tauri frontend contract", () => {
  it("registers every Rust command invoked by the production UI", () => {
    const frontend = readFileSync(
      join(root, "apps/desktop/src/api/desktop.ts"),
      "utf8",
    );
    const rust = readFileSync(join(root, "src-tauri/src/lib.rs"), "utf8");
    const invoked = [...frontend.matchAll(/\binvoke\("([a-z0-9_]+)"/g)].map(
      (match) => match[1],
    );
    const handlerBlock = rust.match(/tauri::generate_handler!\[([\s\S]*?)\]\)/);

    assert.ok(handlerBlock, "Rust invoke handler registration is missing");
    const registered = handlerBlock[1]
      .split(",")
      .map((command) => command.trim())
      .filter(Boolean);
    const missing = [...new Set(invoked)].filter(
      (command) => !registered.includes(command),
    );

    assert.deepEqual(missing, []);
    assert.match(frontend, /window\.__TAURI_INTERNALS__ !== undefined/);
    assert.match(frontend, /listen<StackSnapshot>\("stack-snapshot"/);
    assert.match(rust, /emit\("stack-snapshot"/);
    assert.match(rust, /emit\("update-progress"/);
  });

  it("fixes the main window at 1120x760 without resize", () => {
    const config = JSON.parse(
      readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
    );
    const window = config.app.windows[0];
    assert.equal(window.width, 1120);
    assert.equal(window.height, 760);
    assert.equal(window.minWidth, 1120);
    assert.equal(window.minHeight, 760);
    assert.equal(window.maxWidth, 1120);
    assert.equal(window.maxHeight, 760);
    assert.equal(window.resizable, false);
  });

  it("assigns the default application icon to the tray builder", () => {
    const rust = readFileSync(join(root, "src-tauri/src/lib.rs"), "utf8");
    assert.match(rust, /default_window_icon\(\)/);
    assert.match(
      rust,
      /TrayIconBuilder::new\(\)[\s\S]*?\.icon\(icon\)[\s\S]*?\.menu\(&menu\)/,
    );
  });

  it("disables WebKitGTK DMA-BUF rendering before the Linux webview starts", () => {
    const rust = readFileSync(join(root, "src-tauri/src/lib.rs"), "utf8");
    assert.match(rust, /WEBKIT_DISABLE_DMABUF_RENDERER/);
    assert.match(rust, /WEBKIT_DISABLE_COMPOSITING_MODE/);
    assert.match(rust, /LIBGL_ALWAYS_SOFTWARE/);
    assert.match(rust, /apply_linux_webview_workarounds\(\)/);
    assert.match(rust, /BIFLOW_WEBKIT_WORKAROUNDS/);
    const applyFn = rust.match(
      /fn apply_linux_webview_workarounds\(\) \{[\s\S]*?\n\}/,
    );
    assert.ok(applyFn, "apply_linux_webview_workarounds is missing");
    assert.match(applyFn[0], /command\.exec\(\)/);
    assert.doesNotMatch(applyFn[0], /\.status\(\)/);
    assert.match(rust, /current_exe\(\)/);
    assert.match(rust, /single_instance_dbus_id/);
    assert.match(rust, /dbus_id\(single_instance_dbus_id/);
    const run = rust.match(/pub fn run\(\) \{([\s\S]*?)let builder =/);
    assert.ok(run, "run() is missing");
    const apply = run[1].indexOf("apply_linux_webview_workarounds");
    const diagnostics = run[1].indexOf("initialize_diagnostics");
    assert.ok(
      apply >= 0 && diagnostics > apply,
      "WebKit env must be set before diagnostics and GTK start",
    );
  });

  it("does not attach a console or leftover terminal when the GUI starts", () => {
    const desktopMain = readFileSync(
      join(root, "src-tauri/src/main.rs"),
      "utf8",
    );
    const helperMain = readFileSync(
      join(root, "crates/iran-split-helper/src/main.rs"),
      "utf8",
    );
    const config = JSON.parse(
      readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
    );
    const desktop = readFileSync(
      join(root, "packaging/linux/app.desktop"),
      "utf8",
    );
    assert.match(
      desktopMain,
      /#!\[cfg_attr\(windows, windows_subsystem = "windows"\)\]/,
    );
    assert.match(
      helperMain,
      /#!\[cfg_attr\(windows, windows_subsystem = "windows"\)\]/,
    );
    assert.equal(
      config.bundle.linux.deb.desktopTemplate,
      "../packaging/linux/app.desktop",
    );
    assert.match(desktop, /^Terminal=false$/m);
  });

  it("keeps Unix-only OpenOptions inside the Unix sync_directory function", () => {
    const config = readFileSync(
      join(root, "crates/iran-split-config/src/lib.rs"),
      "utf8",
    );
    assert.doesNotMatch(config, /use std::fs::OpenOptions/);
    assert.doesNotMatch(config, /fs::\{self, OpenOptions\}/);
    assert.match(config, /fs::OpenOptions::new\(\)/);
  });

  it("uses tail expressions in cfg blocks so host Clippy needless_return stays clean", () => {
    const cfgReturn = /#\[cfg\([^\]]+\)\]\s*\{\s*return /;
    const offenders = rustSources(root).filter((path) =>
      cfgReturn.test(readFileSync(path, "utf8")),
    );

    assert.deepEqual(
      offenders,
      [],
      "cfg blocks compiled by host Clippy must be tail expressions, not return statements",
    );
  });
});
