// Шаги мультивкладок документов.
// Вкладки живут в #tab-strip (.tab), каждая имеет data-id, aria-selected,
// .name (имя), .dirty (индикатор изменений) и .close (кнопка закрытия).
import { Given, Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

/// Количество вкладок в #tab-strip.
async function tabCount() {
  return browser.execute(
    () => document.querySelectorAll("#tab-strip .tab").length
  );
}

/// Индекс активной вкладки (0-based) или -1.
async function activeTabIndex() {
  return browser.execute(() => {
    const tabs = Array.from(document.querySelectorAll("#tab-strip .tab"));
    return tabs.findIndex((t) => t.getAttribute("aria-selected") === "true");
  });
}

/// Дожидается точного числа вкладок: создание/закрытие — асинхронный IPC
/// (`newDocument`/`closeTab`), поэтому проверяем с ожиданием, а не мгновенно.
/// Поддерживаем только #tab-strip — единственный контейнер вкладок.
async function expectTabCount(selector, expected) {
  await browser.waitUntil(
    async () => (await tabCount()) === expected,
    {
      timeout: 5000,
      timeoutMsg: `В ${selector} не стало ${expected} вкладок`,
    }
  );
}

Then("в {string} {int} вкладка", async (selector, expected) => {
  await expectTabCount(selector, expected);
});

Then("в {string} {int} вкладки", async (selector, expected) => {
  await expectTabCount(selector, expected);
});

When("я создаю новый документ", async () => {
  const before = await tabCount();
  const ok = await browser.execute(() => {
    const btn = document.getElementById("btn-new");
    if (!btn) return false;
    btn.click();
    return true;
  });
  if (!ok) throw new Error("Кнопка #btn-new не найдена");
  // Даём store.newDocument() завершить IPC и обновить вкладки: новая ровно одна.
  await browser.waitUntil(
    async () => (await tabCount()) === before + 1,
    { timeout: 5000, timeoutMsg: "Новая вкладка не создалась" }
  );
});

When("я переключаюсь на {int} вкладку", async (index1) => {
  // index1 — 1-based для читаемости Gherkin (первая, вторая, ...)
  const idx = index1 - 1;
  const ok = await browser.execute((i) => {
    const tabs = document.querySelectorAll("#tab-strip .tab");
    const tab = tabs[i];
    if (!tab) return false;
    tab.click();
    return true;
  }, idx);
  if (!ok) throw new Error(`Вкладка №${index1} не найдена`);
  await browser.pause(50);
});

Then("активна {int} вкладка", async (index1) => {
  const idx = index1 - 1;
  await browser.waitUntil(
    async () => {
      const active = await activeTabIndex();
      return active === idx;
    },
    { timeout: 5000, timeoutMsg: `Активна вкладка ${index1}` }
  );
});

When("я закрываю {int} вкладку", async (index1) => {
  const idx = index1 - 1;
  const ok = await browser.execute((i) => {
    const tabs = document.querySelectorAll("#tab-strip .tab");
    const tab = tabs[i];
    if (!tab) return false;
    const close = tab.querySelector(".close");
    if (!close) return false;
    close.click();
    return true;
  }, idx);
  if (!ok) throw new Error(`Вкладка №${index1} не найдена`);
  await browser.pause(100); // IPC close + перерисовка
});

Then("у {int} вкладки aria-selected равен {string}", async (index1, expected) => {
  const idx = index1 - 1;
  const actual = await browser.execute((i) => {
    const tabs = document.querySelectorAll("#tab-strip .tab");
    return tabs[i]?.getAttribute("aria-selected") ?? null;
  }, idx);
  if (actual !== expected) {
    throw new Error(
      `У вкладки №${index1} aria-selected=${JSON.stringify(actual)}, ожидалось ${JSON.stringify(expected)}`
    );
  }
});

Then("у {int} вкладки есть dirty-точка", async (index1) => {
  const idx = index1 - 1;
  await browser.waitUntil(
    async () => {
      const has = await browser.execute((i) => {
        const tabs = document.querySelectorAll("#tab-strip .tab");
        const tab = tabs[i];
        if (!tab) return false;
        return !!tab.querySelector(".dirty");
      }, idx);
      return has;
    },
    { timeout: 5000, timeoutMsg: `У вкладки №${index1} нет dirty-точки` }
  );
});

Then("у {int} вкладки нет dirty-точки", async (index1) => {
  const idx = index1 - 1;
  await browser.waitUntil(
    async () => {
      const has = await browser.execute((i) => {
        const tabs = document.querySelectorAll("#tab-strip .tab");
        const tab = tabs[i];
        if (!tab) return false;
        return !!tab.querySelector(".dirty");
      }, idx);
      return !has;
    },
    { timeout: 5000, timeoutMsg: `У вкладки №${index1} осталась dirty-точка` }
  );
});

When("я сохраняю документ", async () => {
  const ok = await browser.execute(() => {
    const btn = document.getElementById("btn-save");
    if (!btn) return false;
    btn.click();
    return true;
  });
  if (!ok) throw new Error("Кнопка #btn-save не найдена");
  // Даём saveDocument() завершить IPC и снять dirty
  await browser.pause(500);
});
