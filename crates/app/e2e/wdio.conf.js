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

// «Осиротевшие» процессы прогона: при сбое/прерывании на Windows остаются
// darmark.exe / msedgedriver.exe / tauri-driver.exe. Они держат exe (блокируют
// следующую release-сборку) и мешают чисто поднять новый сеанс. Чистим на
// границах прогона (onPrepare/onComplete), не трогая активный сеанс.
// @wdio/tauri-service складывает скачанный msedgedriver в новый случайный
// подкаталог `%TEMP%/msedgedriver/<hash>` на каждый прогон и не убирает его —
// каталог копит сотни копий. Чистим после прогона.
function cleanupEdgeDriverCache() {
  const dir = path.join(os.tmpdir(), "msedgedriver");
  try {
    fs.rmSync(dir, { recursive: true, force: true });
  } catch (e) {
    console.warn("[e2e] не очистить кэш msedgedriver:", String(e));
  }
}

function killStrayProcesses() {
  if (process.platform !== "win32") return;
  for (const image of ["darmark.exe", "msedgedriver.exe", "tauri-driver.exe"]) {
    try {
      execFileSync("taskkill", ["/IM", image, "/F", "/T"], { stdio: "ignore" });
    } catch {
      // процесса нет — это норма
    }
  }
}

export const config = {
  runner: "local",
  specs: [path.join(here, "features", "**", "*.feature")],
  maxInstances: 1,

  // Готовит env для плагинных спеков до старта приложения (см. preparePlugins).
  // Перед стартом убираем «осиротевшие» процессы прошлых прогонов.
  //
  // msedgedriver: @wdio/tauri-service каждый прогон пытается скачать драйвер и
  // проверяет его версию регэкспом `/MSEdgeDriver …/`, который НЕ совпадает с
  // реальным выводом `Microsoft Edge WebDriver …`. Поэтому «found в PATH» не
  // срабатывает и сервис всегда идёт качать (при сбое сети — теряет время).
  // Лечится наличием msedgedriver.exe в PATH: сам `tauri-driver` берёт драйвер
  // оттуда, и прогон проходит даже если загрузка сервиса не удалась. Держим
  // совместимый по мажору драйвер в PATH (например, ~/.cargo/bin).
  onPrepare: () => {
    killStrayProcesses();
    // Кэш драйвера чистим здесь (в onComplete файлы ещё держит драйвер → EPERM).
    cleanupEdgeDriverCache();
    preparePlugins();
  },
  // Убирает временные каталоги (плагины, конфиг) и процессы после прогона
  // (иначе darmark.exe держит release-бинарник и мешает следующей сборке).
  onComplete: () => {
    cleanupPlugins();
    killStrayProcesses();
  },
  // Небольшая пауза перед каждым сеансом: даёт ОС освободить ресурсы после
  // предыдущего спека (снижает флейк старта приложения в длинных прогонах).
  beforeSession: async () => {
    await new Promise((resolve) => setTimeout(resolve, 1000));
  },

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
        // Запас на медленный старт приложения/драйвера в длинных прогонах
        // (по умолчанию 60с; флейк старта фиксировался на поздних спеках).
        startTimeout: 120_000,
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
    // С запасом: старт сеанса на поздних спеках мог занимать около минуты.
    timeout: 120_000,
    strict: true,
    // Нативные диалоги/системный браузер через WebDriver не автоматизируются —
    // такие сценарии помечаем @manual и по умолчанию пропускаем.
    // @wip — TDD-сценарии, написанные ДО реализации:
    // держим в репозитории, но не гоняем в обычном прогоне, пока фича не готова.
    // (BUG-003 реализован — его сценарии больше не помечены @wip.)
    tags: "not @manual and not @wip",
  },
};
