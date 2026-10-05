// Хуки, общие для всех сценариев.
import { After, Before } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import { setInspectorActive } from "./helpers.js";

// Сбрасываем XSS-ловушку между сценариями. Редактор здесь не трогаем:
// часть сценариев проверяет стартовое состояние (непустой документ).
Before(async () => {
  await browser.execute(() => {
    delete window.__xss;
  });
});

// Инспектор меняет DOM предпросмотра (обёртки .md-block) и навешивает
// слушатели — гасим его после каждого сценария, чтобы не влиять на остальные.
After(async () => {
  await setInspectorActive(false);
});
