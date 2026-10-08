// Шаги Markdown-форматирования выделения (formatActions через тулбар).
//
// «Жирный» вызываем кликом по кнопке `#format-group button[data-action="bold"]`
// программно (`.click()`): это детерминированно и не зависит от доставки
// клавиатурных событий/состояния модификатора в WebDriver. Кнопка и Ctrl+B
// ведут к одному обработчику `formatActions.bold()`, поэтому поведение обёртки
// проверяется одинаково — а именно `wrap()` и есть источник BUG-001.
import { Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

/// Текущее значение редактора (для проверок формата и сообщений об ошибках).
function editorValue() {
  return browser.execute(() => document.getElementById("editor").value);
}

/// Клик по кнопке формата с имитацией реального нажатия мышью: фокус уходит с
/// редактора на кнопку. `btn.click()` без смены фокуса не воспроизводит поведение
/// пользователя (именно фокус-путь может давать побочный скролл).
async function applyFormat(action) {
  const ok = await browser.execute((a) => {
    const editor = document.getElementById("editor");
    editor.blur(); // реальный клик по тулбару снимает фокус с редактора
    const btn = document.querySelector(`#format-group button[data-action="${a}"]`);
    if (!btn) return false;
    btn.focus();
    btn.click();
    return true;
  }, action);
  if (!ok) throw new Error(`Не найдена кнопка форматирования "${action}"`);
}

When("я применяю формат «Жирный»", () => applyFormat("bold"));
When("я применяю формат «Ссылка»", () => applyFormat("link"));

// Путь хоткея: синтетический Ctrl+B в window — тот же обработчик shell.ts, что и
// у реальной клавиатуры (в отличие от клика по кнопке редактор при этом в фокусе).
When("я применяю «Жирный» через Ctrl+B", async () => {
  await browser.execute(() => {
    document.getElementById("editor").focus();
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "b", ctrlKey: true, bubbles: true, cancelable: true })
    );
  });
});

// Скролл панелей: программный сброс в начало и проверка, что формат его не сдвинул
// (проверяются оба контейнера: #editor и #preview — BUG-005 сдвигал предпросмотр).
When("редактор прокручен в начало", async () => {
  await browser.execute(() => {
    const ed = document.getElementById("editor");
    ed.focus();
    ed.scrollTop = 0;
  });
});

Then("скролл панелей остаётся в начале", async () => {
  // Ждём дебаунс рендера (`update_document` через 120 мс) и синхронизацию скролла.
  await browser.pause(400);
  const s = await browser.execute(() => ({
    editor: Math.round(document.getElementById("editor").scrollTop),
    preview: Math.round(document.getElementById("preview").scrollTop),
  }));
  if (s.editor > 1 || s.preview > 1) {
    throw new Error(`Скролл уехал вниз: ${JSON.stringify(s)}`);
  }
});

Then("значение редактора равно {string}", async (expected) => {
  const value = await editorValue();
  if (value !== expected) {
    throw new Error(
      `Значение редактора ${JSON.stringify(value)}, ожидалось ${JSON.stringify(expected)}`
    );
  }
});

Then("значение редактора начинается с {string}", async (prefix) => {
  const value = await editorValue();
  if (!value.startsWith(prefix)) {
    throw new Error(
      `Значение редактора ${JSON.stringify(value)} не начинается с ${JSON.stringify(prefix)}`
    );
  }
});

Then("значение редактора не содержит {string}", async (fragment) => {
  const value = await editorValue();
  if (value.includes(fragment)) {
    throw new Error(
      `Значение редактора ${JSON.stringify(value)} содержит ${JSON.stringify(fragment)}`
    );
  }
});
