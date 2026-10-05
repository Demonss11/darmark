// Хуки, общие для всех сценариев.
import { Before } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

// Сбрасываем XSS-ловушку между сценариями. Редактор здесь не трогаем:
// часть сценариев проверяет стартовое состояние (непустой документ).
Before(async () => {
  await browser.execute(() => {
    delete window.__xss;
  });
});
