// Хуки, общие для всех сценариев.
import { After, Before } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import { setInspectorActive, setPreviewVisible, setSyncEnabled } from "./helpers.js";

// Изоляция состояния вкладок (tabs.feature, тег @tabs): сценарии предполагают
// ровно одну вкладку на старте. Приложение живёт между сценариями, вкладки
// накапливаются, а у dirty-вкладки создание нового документа/закрытие вызвало бы
// НАТИВНЫЙ диалог подтверждения (WebDriver его не автоматизирует).
//
// Навигацию (refresh/reload) не используем: она рушит WebDriver-сессию. Вместо
// этого приводим состояние к одной ЧИСТОЙ вкладке средствами самого приложения:
//   1) снимаем dirty со всех вкладок синтетическим Ctrl+Z по #editor — undo
//      возвращает текст к savedText и гасит индикатор (docStore.history);
//   2) закрываем лишние вкладки, пока не останется одна.
// Хук объявлен ДО общего Before, чтобы xss-ловушка навесилась на готовое состояние.
Before({ tags: "@tabs" }, async () => {
  const tabCount = () =>
    browser.execute(() => document.querySelectorAll("#tab-strip .tab").length);

  // 1. Снять dirty со всех вкладок. Вкладку активируем кликом (undo работает по
  //    активному документу), затем «отменяем» правки синтетическим Ctrl+Z, пока
  //    не исчезнет .dirty или пока отмена не перестанет менять состояние. Ограничиваем
  //    число итераций: если истории не хватает, дальше повторять бессмысленно.
  const total = await tabCount();
  for (let i = 0; i < total; i++) {
    const activated = await browser.execute((idx) => {
      const tab = document.querySelectorAll("#tab-strip .tab")[idx];
      if (!tab) return false;
      tab.click();
      return true;
    }, i);
    if (!activated) break;
    await browser.pause(30);

    for (let n = 0; n < 5; n++) {
      const hasDirty = await browser.execute((idx) => {
        const tab = document.querySelectorAll("#tab-strip .tab")[idx];
        return !!tab?.querySelector(".dirty");
      }, i);
      if (!hasDirty) break;
      await browser.execute(() => {
        document.getElementById("editor")?.dispatchEvent(
          new KeyboardEvent("keydown", {
            code: "KeyZ",
            key: "z",
            ctrlKey: true,
            bubbles: true,
            cancelable: true,
          })
        );
      });
      await browser.pause(80); // IPC undo + перерисовка
    }
  }

  // 2. Закрыть лишние вкладки: пока их больше одной — закрываем последнюю и ждём
  //    уменьшения счётчика (закрытие — асинхронный IPC close_document).
  for (let guard = 0; guard < 20; guard++) {
    const before = await tabCount();
    if (before <= 1) break;
    const ok = await browser.execute(() => {
      const tabs = document.querySelectorAll("#tab-strip .tab");
      const close = tabs[tabs.length - 1]?.querySelector(".close");
      if (!close) return false;
      close.click();
      return true;
    });
    if (!ok) break;
    await browser.waitUntil(async () => (await tabCount()) < before, {
      timeout: 5000,
      timeoutMsg: "Лишняя вкладка не закрылась",
    });
  }

  // 3. Вернуть активной core-вкладку «Предпросмотр» (как в общем After): предыдущий
  //    плагинный сценарий мог переключиться на `#plugin-view`, скрыв `#preview`.
  await browser.execute(() => {
    const core = document.querySelector("#view-switch .vtab.core");
    if (core && !core.classList.contains("active")) core.click();
  });
});

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
  // Панель «Журнал» тоже живёт в DOM между сценариями (logs.feature): развёрнутая
  // панель «переезжает» дальше, и сценарии, начинающиеся с клика по #sb-logs,
  // вместо разворота сворачивали бы её. Возвращаем свёрнутый вид; у скрытой панели
  // пользовательского состояния ещё нет — не трогаем.
  await browser.execute(() => {
    const panel = document.getElementById("logs-panel");
    if (!panel || panel.hidden || panel.classList.contains("collapsed")) return;
    document.getElementById("logs-collapse")?.click();
  });
});
