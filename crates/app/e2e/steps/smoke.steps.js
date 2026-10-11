// Шаги дымовых проверок оболочки.
import { Given, Then } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import { waitPreviewContains } from "./helpers.js";

// ВАЖНО: здесь нельзя использовать WebDriver-команды элементов (`$`/`$$`,
// waitForExist/getValue и т.п.). Их перехватывает `ensureActiveWindowFocus()`
// из @wdio/tauri-service и на каждую команду делает запрос состояния окна через
// `browser.tauri.execute`. В darmark нет guest-JS `@wdio/tauri-plugin`, поэтому
// проба готовности крутится ~100×50 мс и падает — это ~5 с простоя НА КАЖДУЮ
// команду. Ожидание/чтение DOM делаем через `browser.execute` (проектный паттерн).
Given("приложение darmark запущено", async () => {
  // Таймаут с запасом: на поздних спеках длинного прогона вебвью стартует
  // медленнее (см. startTimeout в wdio.conf.js).
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => !!document.getElementById("editor") && !!document.getElementById("preview")
      ),
    { timeout: 45_000, timeoutMsg: "Редактор/предпросмотр не появились" }
  );
});

Then("поле редактора содержит непустой документ", async () => {
  // Стартовый документ создаётся через IPC (bootstrap → newDocument), поэтому
  // дожидаемся непустого значения, а не только существования #editor.
  await browser.waitUntil(
    async () => ((await browser.execute(() => document.getElementById("editor").value)) ?? "").length > 0,
    { timeout: 8_000, timeoutMsg: "Поле редактора пусто" }
  );
});

Then("предпросмотр содержит заголовок {string}", async (title) => {
  await waitPreviewContains(title);
});
