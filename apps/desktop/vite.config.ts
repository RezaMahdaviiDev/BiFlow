import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const appVersion = readFileSync(resolve(root, "version"), "utf8").trim();

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: ["es2021", "chrome100", "safari13"],
    minify: process.env.TAURI_DEBUG ? false : "esbuild",
    sourcemap: Boolean(process.env.TAURI_DEBUG),
  },
  test: {
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
    isolate: true,
    css: true,
    exclude: ["**/node_modules/**", "**/e2e/**", "**/dist/**"],
    coverage: {
      provider: "v8",
      reporter: ["text", "html"],
    },
  },
});
