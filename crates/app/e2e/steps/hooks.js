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
  // Дровер плагинов — тоже модальный слой (`inert` на #app), но он не попадает
  // под очистку `.dialog-backdrop` ниже. Закрываем его явно: иначе открытый
  // дровер «переезжает» в следующий сценарий, и клик по `#tb-plugins` в сценарии
  // открытия, наоборот, закрывает уже открытый дровер (регрессия изоляции).
  await browser.execute(() => {
    const drawer = document.getElementById("drawer");
    if (drawer?.classList.contains("open")) {
      document.getElementById("drawer-close")?.click();
    }
  });
  // R1: сначала закрываем модальные диалоги (палитра). Пока открыт диалог,
  // `#app` инертен, и последующие сбросы (клики по #btn-inspect и т.п.) могли бы
  // сломаться. Синтетический Escape по window закрывает верхний диалог; повторяем
  // на весь стек. Затем чистим тосты, чтобы они не «переезжали» между сценариями.
  await browser.execute(() => {
    for (let i = 0; i < 5; i++) {
      if (!document.querySelector(".dialog-backdrop")) break;
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })
      );
    }
    document.getElementById("toast-host")?.replaceChildren();
  });
  await setInspectorActive(false);
  await setPreviewVisible(true);
  await setSyncEnabled(true);
  // Возвращаем активной core-вкладку «Предпросмотр»: плагинный сценарий мог
  // переключиться на `#plugin-view`, скрыв `#preview` (нулевая геометрия).
  await browser.execute(() => {
    const core = document.querySelector("#view-switch .vtab.core");
    if (core && !core.classList.contains("active")) core.click();
  });
  await browser.execute(() => {
    const editor = document.getElementById("editor");
    const preview = document.getElementById("preview");
    if (editor) editor.scrollTop = 0;
    if (preview) preview.scrollTop = 0;
  });
});
