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

When("я применяю формат «Жирный»", async () => {
  const ok = await browser.execute(() => {
    const btn = document.querySelector('#format-group button[data-action="bold"]');
    if (!btn) return false;
    btn.click();
    return true;
  });
  if (!ok) throw new Error("Не найдена кнопка форматирования «Жирный»");
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
