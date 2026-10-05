// Хуки, общие для всех сценариев.
import { After, Before } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import { setInspectorActive, setPreviewVisible, setSyncEnabled } from "./helpers.js";

// Сбрасываем XSS-ловушку и накапливаем необработанные ошибки между сценариями.
// Редактор здесь не трогаем: часть сценариев проверяет стартовое состояние.
Before(async () => {
  await browser.execute(() => {
    delete window.__xss;
    if (!window.__errorCapture) {
      window.__errors = [];
      window.addEventListener("error", (e) => {
        window.__errors.push(String(e.message || e.error || e));
      });
      window.__errorCapture = true;
    }
    window.__errors.length = 0;
  });
});

// Инспектор меняет DOM предпросмотра (обёртки .md-block) и навешивает
// слушатели — гасим его после каждого сценария, чтобы не влиять на остальные.
// Заодно возвращаем дефолтные состояния тумблеров и обнуляем прокрутку панелей.
After(async () => {
  await setInspectorActive(false);
  await setPreviewVisible(true);
  await setSyncEnabled(true);
  await browser.execute(() => {
    const editor = document.getElementById("editor");
    const preview = document.getElementById("preview");
    if (editor) editor.scrollTop = 0;
    if (preview) preview.scrollTop = 0;
  });
});
