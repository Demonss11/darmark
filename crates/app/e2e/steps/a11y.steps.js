// Шаги доступности базовых контролов (UI-ревью, приоритет 1).
// Проверяем статические атрибуты (role/доступное имя) и видимость фокус-ринга
// через вычисленные стили — без обращения к скриншотам.
import { Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

Then("у элемента {string} роль {string}", async (selector, role) => {
  const actual = await browser.execute(
    (s) => document.querySelector(s)?.getAttribute("role") ?? null,
    selector
  );
  if (actual !== role) {
    throw new Error(
      `У ${selector} role=${JSON.stringify(actual)}, ожидалось ${JSON.stringify(role)}`
    );
  }
});

Then(
  "доступное имя элемента {string} равно {string}",
  async (selector, expected) => {
    const actual = await browser.execute((s) => {
      const el = document.querySelector(s);
      if (!el) return null;
      return (
        el.getAttribute("aria-label") ||
        el.getAttribute("title") ||
        el.textContent?.trim() ||
        null
      );
    }, selector);
    if (actual !== expected) {
      throw new Error(
        `Доступное имя ${selector} = ${JSON.stringify(actual)}, ожидалось ${JSON.stringify(expected)}`
      );
    }
  }
);

When("я фокусирую элемент {string}", async (selector) => {
  const ok = await browser.execute((s) => {
    const el = document.querySelector(s);
    if (!el) return false;
    el.focus();
    return true;
  }, selector);
  if (!ok) throw new Error(`Элемент ${selector} не найден`);
  await browser.pause(30);
});

Then("у элемента {string} виден фокус-ринг", async (selector) => {
  const info = await browser.execute((s) => {
    const el = document.querySelector(s);
    if (!el) return null;
    const cs = getComputedStyle(el);
    return {
      width: parseFloat(cs.outlineWidth) || 0,
      style: cs.outlineStyle,
    };
  }, selector);
  if (!info) throw new Error(`Элемент ${selector} не найден`);
  if (info.style === "none" || info.width <= 0) {
    throw new Error(
      `У ${selector} нет видимого фокус-ринга (outline: ${info.style} ${info.width}px)`
    );
  }
});
