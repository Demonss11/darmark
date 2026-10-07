// E2E-конфигурация darmark: WebdriverIO + Cucumber поверх реального Tauri-бинарника.
// Драйвер — внешний tauri-driver (cargo install tauri-driver); на Windows сервис
// сам подбирает msedgedriver под установленный WebView2. Код приложения не трогается.
//
// Запуск: из crates/app → `npm run test:e2e`
// Переопределить бинарник: переменная окружения DARMARK_APP_BINARY.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));

// Корень workspace: <repo> (wdio.conf лежит в crates/app/e2e).
const repoRoot = path.resolve(here, "../../..");

// Workspace-сборка кладёт бинарник в <repo>/target/release (см. корневой Cargo.toml).
const appBinary =
  process.env.DARMARK_APP_BINARY ??
  path.resolve(repoRoot, "target/release/darmark.exe");

// Подготовка спеков плагинов (H2, Фаза 4): временный каталог плагинов с
// фикстурой и путь к child-бинарнику. Обычные спеки этого не требуют, поэтому
// ошибки подготовки не валят запуск — они лишь оставляют env невыставленным
// (плагинный спек тогда упадёт с явной причиной, остальные пройдут).
//
// Временные каталоги копятся здесь и удаляются в `onComplete`.
const tempDirs = [];

function tempDir(prefix) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  tempDirs.push(dir);
  return dir;
}

function preparePlugins() {
  try {
    // Изолируем конфиг: иначе `init_settings_dir` создаст/прочитает реальный
    // %APPDATA%/darmark/config.json, и выключенный там `e2e-view` уронит спек.
    process.env.DARMARK_CONFIG_PATH = path.join(tempDir("darmark-e2e-config-"), "config.json");

    const fixtures = path.join(here, "fixtures", "plugins");
    if (fs.existsSync(fixtures)) {
      const dir = tempDir("darmark-e2e-plugins-");
      for (const entry of fs.readdirSync(fixtures, { withFileTypes: true })) {
        if (entry.isDirectory()) {
          fs.cpSync(path.join(fixtures, entry.name), path.join(dir, entry.name), {
            recursive: true,
          });
        }
      }
      process.env.DARMARK_PLUGINS_DIR = dir;
    }

    const hostExe =
      process.env.DARMARK_PLUGIN_HOST_EXE ??
      path.resolve(repoRoot, "target/release/darmark-plugin-host.exe");
    if (!fs.existsSync(hostExe)) {
      // child-бинарник собирается из workspace (mlua vendored — первая сборка долгая).
      execFileSync("cargo", ["build", "-p", "plugin-host", "--release"], {
        cwd: repoRoot,
        stdio: "inherit",
      });
    }
    if (fs.existsSync(hostExe)) process.env.DARMARK_PLUGIN_HOST_EXE = hostExe;
  } catch (e) {
    console.warn("[e2e] подготовка плагинов пропущена:", String(e));
  }
}

function cleanupPlugins() {
  for (const dir of tempDirs) {
    try {
      fs.rmSync(dir, { recursive: true, force: true });
    } catch (e) {
      console.warn("[e2e] не удалить временный каталог:", String(e));
    }
  }
  tempDirs.length = 0;
}

export const config = {
  runner: "local",
  specs: [path.join(here, "features", "**", "*.feature")],
  maxInstances: 1,

  // Готовит env для плагинных спеков до старта приложения (см. preparePlugins).
  onPrepare: () => preparePlugins(),
  // Убирает временные каталоги (плагины, конфиг) после прогона.
  onComplete: () => cleanupPlugins(),

  services: [
    [
      "tauri",
      {
        appBinaryPath: appBinary,
        driverProvider: "external", // явный tauri-driver вместо embedded-сервера
        autoInstallTauriDriver: false, // ставится вручную: cargo install tauri-driver
        autoDownloadEdgeDriver: true, // Windows: подбор msedgedriver под WebView2
        tauriDriverPort: 4444,
        logLevel: "warn",
        startTimeout: 60_000,
      },
    ],
  ],

  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application: appBinary },
    },
  ],

  logLevel: "info",
  bail: 0,
  waitforTimeout: 10_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 2,

  framework: "cucumber",
  reporters: ["spec"],

  cucumberOpts: {
    import: [path.join(here, "steps", "**", "*.js")],
    timeout: 60_000,
    strict: true,
    // Нативные диалоги/системный браузер через WebDriver не автоматизируются —
    // такие сценарии помечаем @manual и по умолчанию пропускаем.
    tags: "not @manual",
  },
};
