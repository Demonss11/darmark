// Шаги строки мониторинга ресурсов (TZ-RESOURCE-MON, ADR-0031): слот
// `#logs-resources` в шапке панели «Журнал». Проверяем только наблюдаемый DOM
// (текст/классы/атрибуты), без `getComputedStyle` и без магических пауз: данные
// приходят событием `resource-metrics` (тик 2 с), поэтому все проверки — через
// `waitUntil` с DOM-условием.
import { Then } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

Then(
  "в слоте ресурсов видна строка главного процесса со значением в мегабайтах",
  async () => {
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          // Контракт DOM — §6.2: строка главного процесса `.mon-row.mon-main`
          // со значением `.mon-val`. Фолбэк на текст самой строки (если число
          // отрисовано отдельным узлом без класса), но НЕ на весь слот — иначе
          // проверка прошла бы на одной строке плагина.
          const row = document
            .getElementById("logs-resources")
            ?.querySelector(".mon-row.mon-main");
          if (!row) return false;
          const val = row.querySelector(".mon-val")?.textContent ?? row.textContent ?? "";
          return /\d[\d\s\u00a0]*МБ/.test(val);
        }),
      {
        timeout: 8000,
        timeoutMsg: "В #logs-resources нет ОЗУ главного процесса в формате «… МБ»",
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
          // Привязка строки к плагину — `data-plugin-id` (§6.2/§6.3).
          const row = document.querySelector(
            `#logs-resources .mon-row.mon-child[data-plugin-id="${id}"]`
          );
          const val = row?.querySelector(".mon-val")?.textContent ?? "";
          // «<commit> / <лимит> МБ» (§6.3): два числа, разделитель «/», единица «МБ».
          return /^\s*\d[\d\s\u00a0]*\/\s*\d[\d\s\u00a0]*МБ\s*$/.test(val);
        }, pluginId),
      {
        timeout: 8000,
        timeoutMsg:
          `В #logs-resources нет строки плагина ${pluginId} в формате «<commit> / <лимит> МБ»`,
      }
    );
  }
);

Then("в слоте ресурсов возраст снимка показан в секундах", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        // Возраст — `.mon-age` (§6.2/§6.4).
        const age = document.querySelector("#logs-resources .mon-age");
        return !!age && /\d+\s*сек\s*назад/.test(age.textContent ?? "");
      }),
    {
      timeout: 8000,
      timeoutMsg: "В #logs-resources нет возраста снимка «N сек назад»",
    }
  );
});
