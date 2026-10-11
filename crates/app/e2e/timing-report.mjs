// Генерация docs/E2E-PERFORMANCE.md из JSONL-замеров (см. steps/timing.js).
//
// Использование:
//   cd crates/app
//   $env:DARMARK_E2E_TIMING="$env:TEMP\darmark-e2e-timing.jsonl"
//   Remove-Item $env:DARMARK_E2E_TIMING -ErrorAction SilentlyContinue
//   npm run test:e2e
//   node e2e/timing-report.mjs "$env:TEMP\darmark-e2e-timing.jsonl" ../../docs/E2E-PERFORMANCE.md
//
// Файл — снимок последних значений (без дельт), перезаписывается целиком.
import fs from "node:fs";

const [, , jsonlPath, outPath] = process.argv;
if (!jsonlPath || !outPath) {
  console.error("usage: node timing-report.mjs <timing.jsonl> <out.md>");
  process.exit(2);
}

const lines = fs
  .readFileSync(jsonlPath, "utf8")
  .split(/\r?\n/)
  .filter(Boolean)
  .map((l) => JSON.parse(l));

const esc = (s) => String(s ?? "").replace(/\|/g, "\\|").replace(/\r?\n/g, " ");
const secs = (ms) => (ms == null ? "—" : (ms / 1000).toFixed(1) + " с");

const total = lines.length;
const failed = lines.filter((r) => r.status !== "PASSED");
const sum = lines.reduce((s, r) => s + (r.durationMs ?? 0), 0);
const sorted = [...lines].sort((a, b) => (b.durationMs ?? 0) - (a.durationMs ?? 0));

const byFeature = new Map();
for (const r of lines) {
  if (!byFeature.has(r.feature)) byFeature.set(r.feature, { n: 0, sum: 0, max: 0, fail: 0 });
  const f = byFeature.get(r.feature);
  f.n++;
  f.sum += r.durationMs ?? 0;
  f.max = Math.max(f.max, r.durationMs ?? 0);
  if (r.status !== "PASSED") f.fail++;
}

const out = [];
const p = (s = "") => out.push(s);

p("# E2E: замеры времени и статуса сценариев");
p("");
p("> **Назначение:** следить за скоростью и стабильностью GUI E2E-набора");
p("> (`cd crates/app && npm run test:e2e`; Cucumber + WebdriverIO поверх release-бинарника).");
p("> **Метод:** время — wall-clock каждого сценария (хуки `Before`/`After`), а не");
p("> `result.duration` cucumber (он показывает лишь последний шаг). WDIO запускает");
p("> каждую `.feature` отдельной сессией, поэтому в первый сценарий сессии входит старт приложения.");
p("> **Обновление:** файл — снимок последних значений (без дельт); перегенерировать");
p("> `node e2e/timing-report.mjs <timing.jsonl> ../../docs/E2E-PERFORMANCE.md` (см. шапку скрипта).");
p("");
p("Платформа: Windows x86_64; приложение — release-сборка; WebView2.");
p("");
p("## Сводка");
p("");
p(`- Сценариев: **${total}**; выполнено **${total - failed.length}**, ошибок **${failed.length}**.`);
p(`- Суммарное время сценариев: **${secs(sum)}**.`);
p(
  `- Медиана: **${secs(sorted[Math.floor(sorted.length / 2)]?.durationMs)}**, максимум: **${secs(
    sorted[0]?.durationMs
  )}**.`
);
p("");

if (failed.length) {
  p("## Не прошли");
  p("");
  p("| Тест | Функционал | Время | Статус | Причина |");
  p("| --- | --- | ---: | --- | --- |");
  for (const r of failed) {
    p(`| ${esc(r.name)} | ${esc(r.feature)} | ${secs(r.durationMs)} | ошибка | ${esc(r.error)} |`);
  }
  p("");
}

p("## Сводка по функционалам");
p("");
p("| Функционал | Сценариев | Сумма | Среднее | Макс | Ошибок |");
p("| --- | ---: | ---: | ---: | ---: | ---: |");
[...byFeature.entries()]
  .sort((a, b) => b[1].sum - a[1].sum)
  .forEach(([name, f]) => {
    p(`| ${esc(name)} | ${f.n} | ${secs(f.sum)} | ${secs(f.sum / f.n)} | ${secs(f.max)} | ${f.fail} |`);
  });
p("");

p("## Сценарии по убыванию времени");
p("");
p("| # | Тест | Описание (функционал) | Время | Статус |");
p("| ---: | --- | --- | ---: | --- |");
sorted.forEach((r, i) => {
  p(
    `| ${i + 1} | ${esc(r.name)} | ${esc(r.feature)} | ${secs(r.durationMs)} | ${
      r.status === "PASSED" ? "выполнено" : "ошибка"
    } |`
  );
});
p("");

p("## Контекст: устранённые источники накладных расходов");
p("");
p("### WebDriver-команды элементов (`$`/`$$`, `waitForExist`, `getValue`, `elementClick`, `getTitle`)");
p("");
p("Их перехватывает `ensureActiveWindowFocus()` из `@wdio/tauri-service` и на каждую команду");
p("запрашивает состояние окна через `browser.tauri.execute`. В приложении darmark нет guest-JS");
p("`@wdio/tauri-plugin`, поэтому проба готовности плагина крутится полный бюджет (~100×50 мс ≈ 5 с)");
p("и падает («Failed to get window states»). Общая точка входа была — шаг `приложение darmark запущено`");
p('(`smoke.steps.js`) с `$("#editor").waitForExist()` / `$("#preview").waitForExist()`; отсюда до 374 с');
p("на сессию у дровера, вкладок, smoke и a11y.");
p("");
p("**Исправлено:** в `smoke.steps.js` ожидание/чтение DOM переведено на `browser.execute`; в `a11y.steps.js`");
p("реальный клик по тумблеру — на W3C pointer-actions (`browser.action`). Элементных WebDriver-команд в");
p("шагах больше нет (0 `findElement` / 0 «Failed to get window states» за прогон).");
p("");
p("### Дубликат определения шага");
p("");
p("Шаг `предпросмотр виден` был определён и в `logs.steps.js`, и в `plugins.steps.js` → «Multiple step");
p("definitions match». `logs.feature` помечена `@wip`, но файлы шагов загружаются всегда, поэтому");
p("регистрация дублировалась.");
p("");
p("**Исправлено:** вариант в `logs.steps.js` переименован в `предпросмотр виден в рабочей области`");
p("(проверка по геометрии; у глобального шага — по атрибуту `hidden`), Gherkin `logs.feature` обновлён.");
p("");
p("### Остаточные расходы (норма)");
p("");
p("- Новое приложение на каждый spec-файл: время старта входит в первый сценарий сессии, холодный старт —");
p("  в самый первый сценарий прогона.");
p("- Фиксированные паузы шагов (`browser.pause`) — сотни миллисекунд на сценарий.");
p("");

fs.writeFileSync(outPath, out.join("\n") + "\n", "utf8");
console.log(`Wrote ${outPath}: ${total} scenarios, ${failed.length} failed, total ${secs(sum)}`);
