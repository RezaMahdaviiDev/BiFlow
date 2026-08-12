import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

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
  });
});
