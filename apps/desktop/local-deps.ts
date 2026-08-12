import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";

export const HIDDIFY_NAMES = [
  "hiddify",
  "hiddify-app",
  "Hiddify",
  "Hiddify.exe",
  "hiddify.exe",
];
export const MIHOMO_NAMES = [
  "mihomo",
  "clash-meta",
  "mihomo.exe",
  "clash-meta.exe",
];

export function defaultSearchDirs(
  pathValue = process.env.PATH ?? "",
): string[] {
  const home = homedir();
  const local = process.env.LOCALAPPDATA;
  return [
    ...pathValue.split(delimiter).filter(Boolean),
    join(home, ".local/bin"),
    join(home, ".local/share/biflow/bin"),
    join(home, ".local/share/biflow/apps"),
    join(home, "Applications"),
    "/usr/bin",
    "/usr/local/bin",
    "/opt/hiddify",
    "/opt/Hiddify",
    "/opt/biflow",
    ...(local
      ? [
          join(local, "biflow/bin"),
          join(local, "biflow/apps"),
          join(local, "Hiddify"),
        ]
      : []),
  ];
}

function fileExists(path: string): boolean {
  try {
    return existsSync(path);
  } catch {
    return false;
  }
}

export function namesInDirs(names: string[], dirs: string[]): boolean {
  return dirs.some((dir) => names.some((name) => fileExists(join(dir, name))));
}

export function hiddifyAppImageIn(dir: string): boolean {
  try {
    return readdirSync(dir).some((name) => {
      const lower = name.toLowerCase();
      return lower.startsWith("hiddify") && lower.endsWith(".appimage");
    });
  } catch {
    return false;
  }
}

export function detectLocalHiddify(dirs = defaultSearchDirs()): boolean {
  return (
    namesInDirs(HIDDIFY_NAMES, dirs) ||
    dirs.some((dir) => hiddifyAppImageIn(dir))
  );
}

export function detectLocalMihomo(dirs = defaultSearchDirs()): boolean {
  return namesInDirs(MIHOMO_NAMES, dirs);
}
