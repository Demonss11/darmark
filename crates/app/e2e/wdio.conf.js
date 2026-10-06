// E2E-конфигурация darmark: WebdriverIO + Cucumber поверх реального Tauri-бинарника.
// Драйвер — внешний tauri-driver (cargo install tauri-driver); на Windows сервис
// сам подбирает msedgedriver под установленный WebView2. Код приложения не трогается.
//
// Запуск: из crates/app → `npm run test:e2e`
// Переопределить бинарник: переменная окружения DARMARK_APP_BINARY.
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));

// Workspace-сборка кладёт бинарник в <repo>/target/release (см. корневой Cargo.toml).
const appBinary =
  process.env.DARMARK_APP_BINARY ??
  path.resolve(here, "../../../target/release/darmark.exe");

export const config = {
  runner: "local",
  specs: [path.join(here, "features", "**", "*.feature")],
  maxInstances: 1,

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
