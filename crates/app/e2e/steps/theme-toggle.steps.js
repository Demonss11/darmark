// Шаги тумблера темы.
import { Given, Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import { setInspectorActive } from "./helpers.js";

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

// Клавиатурные шаги: синтетический keydown на активном элементе (как в palette.steps.js).
// Для кнопок Enter/Space генерируют click только через полный цикл событий;
// dispatchEvent(keydown) не вызывает default action. Поэтому для button вызываем
// click() вручную — это имитирует результат нажатия клавиши.
When("я нажимаю Enter", async () => {
  await browser.execute(() => {
    const target = document.activeElement ?? document.body;
    target.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "Enter",
        code: "Enter",
        bubbles: true,
        cancelable: true,
      })
    );
    if (target.tagName === "BUTTON") {
      target.dispatchEvent(
        new KeyboardEvent("keyup", {
          key: "Enter",
          code: "Enter",
          bubbles: true,
          cancelable: true,
        })
      );
      target.click();
    }
  });
  await browser.pause(50);
});

When("я нажимаю Space", async () => {
  await browser.execute(() => {
    const target = document.activeElement ?? document.body;
    target.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: " ",
        code: "Space",
        bubbles: true,
        cancelable: true,
      })
    );
    if (target.tagName === "BUTTON") {
      target.dispatchEvent(
        new KeyboardEvent("keyup", {
          key: " ",
          code: "Space",
          bubbles: true,
          cancelable: true,
        })
      );
      target.click();
    }
  });
  await browser.pause(50);
});

// Включение режима инспектора кнопкой #btn-inspect (setInspectorActive из helpers).
When("я включаю инспектор", async () => {
  await setInspectorActive(true);
});
