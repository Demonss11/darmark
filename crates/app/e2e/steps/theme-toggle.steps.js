// Шаги тумблера темы (ADR-0037).
import { Given, Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

When("я перезагружаю страницу", async () => {
  await browser.execute(() => location.reload());
  await browser.waitUntil(
    async () =>
      browser.execute(() => document.readyState === "complete"),
    { timeout: 10000, timeoutMsg: "Страница не перезагрузилась" }
  );
  // Даём приложению время на инициализацию (applyTheme вызывается при загрузке)
  await browser.pause(500);
});

When("я очищаю localStorage", async () => {
  await browser.execute(() => localStorage.clear());
});

Given("тема сброшена на тёмную", async () => {
  const current = await browser.execute(() => localStorage.getItem("darmark.theme"));
  if (current !== "dark") {
    // Клик по кнопке вызывает applyTheme (обновляет aria-label/aria-pressed/иконки)
    await browser.execute(() => document.getElementById("sb-theme").click());
    await browser.waitUntil(
      async () =>
        browser.execute(() => document.documentElement.getAttribute("data-theme") === "dark"),
      { timeout: 3000, timeoutMsg: "Тема не сброшена на тёмную" }
    );
  }
});

Then("элемент {string} виден", async (selector) => {
  const visible = await browser.execute((s) => {
    const el = document.querySelector(s);
    if (!el) return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  }, selector);
  if (!visible) throw new Error(`Элемент ${selector} не виден`);
});

Then("в localStorage ключ {string} равен {string}", async (key, expected) => {
  const actual = await browser.execute(
    (k) => localStorage.getItem(k),
    key
  );
  if (actual !== expected) {
    throw new Error(
      `localStorage["${key}"] = ${JSON.stringify(actual)}, ожидалось ${JSON.stringify(expected)}`
    );
  }
});

Then("статусбар виден", async () => {
  const visible = await browser.execute(() => {
    const el = document.getElementById("statusbar");
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  });
  if (!visible) throw new Error("Статусбар не виден");
});

Then("у элемента html атрибут {string} равен {string}", async (attr, expected) => {
  const actual = await browser.execute(
    (a) => document.documentElement.getAttribute(a),
    attr
  );
  if (actual !== expected) {
    throw new Error(
      `У html ${attr}=${JSON.stringify(actual)}, ожидалось ${JSON.stringify(expected)}`
    );
  }
});

Then("у элемента html свойство {string} равно {string}", async (prop, expected) => {
  const actual = await browser.execute(
    (p) => getComputedStyle(document.documentElement)[p],
    prop
  );
  if (actual !== expected) {
    throw new Error(
      `У html ${prop}=${JSON.stringify(actual)}, ожидалось ${JSON.stringify(expected)}`
    );
  }
});
