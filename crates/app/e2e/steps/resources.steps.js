// Шаги вкладки «Мониторинг»: тело `#mon-body`
// содержит слот `#logs-resources`, а строки процессов рендерятся в `#mon-rows`.
// Проверяем только наблюдаемый DOM (текст/классы/атрибуты), без `getComputedStyle`
// и без магических пауз: данные приходят событием `resource-metrics` (тик 2 с),
// поэтому все проверки — через `waitUntil` с DOM-условием.
import { Then } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

Then(
  "в слоте ресурсов видна строка главного процесса со значением в мегабайтах",
  async () => {
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          // Контракт DOM — §6.2: главная строка `.mon-row.mon-main` в `#mon-rows`,
          // значение `.mon-val`. Скоуп — `#mon-body` (вкладка, а не старая шапка).
          const row = document.querySelector(
            "#mon-body #logs-resources .mon-row.mon-main.mon-grid"
          );
          if (!row) return false;
          const val = row.querySelector(".mon-val")?.textContent ?? row.textContent ?? "";
          return /\d[\d\s\u00a0]*МБ/.test(val);
        }),
      {
        timeout: 8000,
        timeoutMsg: "В #mon-rows нет ОЗУ главного процесса в формате «… МБ»",
      }
    );
  }
);

Then(
  "в слоте ресурсов есть строка плагина {string} с долей от лимита в мегабайтах",
  async (pluginId) => {
    await browser.waitUntil(
      async () =>
        browser.execute((id) => {
          // Привязка строки к плагину — `data-plugin-id` (§6.2/§6.3), скоуп — вкладка.
          const row = document.querySelector(
            `#mon-body #logs-resources .mon-row.mon-child.mon-grid[data-plugin-id="${id}"]`
          );
          const val = row?.querySelector(".mon-val")?.textContent ?? "";
          // «<commit> / <лимит> МБ» (§6.3): два числа, разделитель «/», единица «МБ».
          return /^\s*\d[\d\s\u00a0]*\/\s*\d[\d\s\u00a0]*МБ\s*$/.test(val);
        }, pluginId),
      {
        timeout: 8000,
        timeoutMsg:
          `В #mon-rows нет строки плагина ${pluginId} в формате «<commit> / <лимит> МБ»`,
      }
    );
  }
);

Then("в слоте ресурсов возраст снимка показан в секундах", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        // Возраст — `.mon-age` (§6.2/§6.4), скоуп — вкладка «Мониторинг».
        const age = document.querySelector("#mon-body #logs-resources .mon-age");
        return !!age && /\d+\s*сек\s*назад/.test(age.textContent ?? "");
      }),
    {
      timeout: 8000,
      timeoutMsg: "В #mon-rows нет возраста снимка «N сек назад»",
    }
  );
});
