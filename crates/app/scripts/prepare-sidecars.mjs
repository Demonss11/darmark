// prepare-sidecars.mjs — собирает release/debug-бинарники плагинной подсистемы и
// раскладывает их как Tauri sidecar-ы (`bundle.externalBin`).
//
// Зачем: `tauri build` собирает только крейт приложения `darmark`. Для работы
// плагинов рядом с `darmark.exe` нужен `darmark-plugin-host.exe` (child-хост), а
// для разработки плагинов — CLI `darmark-plugin.exe`. Tauri externalBin ожидает
// файлы `binaries/<name>-<target-triple>.exe`.
//
// ВАЖНО: build.rs приложения (`tauri-build`) КОПИРУЕТ externalBin рядом с бинарником
// и падает, если файла нет. Поэтому сайдкары обязаны существовать ДО компиляции
// крейта `darmark` — скрипт вызывается из beforeDevCommand/beforeBuildCommand и в CI.
//
// Запуск:
//   cd crates/app && node scripts/prepare-sidecars.mjs               # release (build)
//   cd crates/app && node scripts/prepare-sidecars.mjs --profile debug  # debug (dev)

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const EXT = process.platform === "win32" ? ".exe" : "";

/** Разбирает `--profile <debug|release>` (по умолчанию `release`). */
function parseProfile(argv) {
  const i = argv.indexOf("--profile");
  const value = i >= 0 ? argv[i + 1] : "release";
  if (value !== "debug" && value !== "release") {
    throw new Error(`--profile: ожидается debug или release, получено ${value}`);
  }
  return value;
}

/** Целевой triple хост-сборки (rustc -vV → строка `host:`). */
function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const line = out.split(/\r?\n/).find((l) => l.startsWith("host:"));
  if (!line) throw new Error("не удалось определить target triple (rustc -vV)");
  return line.slice("host:".length).trim();
}

const scriptDir = dirname(fileURLToPath(import.meta.url)); // crates/app/scripts
const appDir = resolve(scriptDir, ".."); // crates/app
const repoRoot = resolve(appDir, "..", ".."); // корень workspace
const binariesDir = join(appDir, "src-tauri", "binaries");

const profile = parseProfile(process.argv.slice(2));
const profileDir = profile === "debug" ? "debug" : "release";
const targetDir = process.env.CARGO_TARGET_DIR
  ? resolve(repoRoot, process.env.CARGO_TARGET_DIR)
  : join(repoRoot, "target");

// Пакет → имя бинарника (см. crates/*/Cargo.toml).
const sidecars = [
  { pkg: "plugin-host", bin: "darmark-plugin-host" },
  { pkg: "plugin-cli", bin: "darmark-plugin" },
];

const triple = hostTriple();
console.log(`[sidecars] target triple: ${triple}, profile: ${profile}`);

const cargoArgs = ["build", "-p", "plugin-host", "-p", "plugin-cli"];
if (profile === "release") cargoArgs.push("--release");
execFileSync("cargo", cargoArgs, { cwd: repoRoot, stdio: "inherit" });

mkdirSync(binariesDir, { recursive: true });
for (const { bin } of sidecars) {
  const src = join(targetDir, profileDir, `${bin}${EXT}`);
  if (!existsSync(src)) {
    throw new Error(`не найден собранный бинарник: ${src}`);
  }
  const dst = join(binariesDir, `${bin}-${triple}${EXT}`);
  copyFileSync(src, dst);
  console.log(`[sidecars] ${src} → ${dst}`);
}

console.log("[sidecars] готово");
