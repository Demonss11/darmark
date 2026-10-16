// Шаги WAI-ARIA паттерна вкладок.
//
// Проверяем наблюдаемый контракт паттерна tabs: роли и связи (role=tablist/
// tab/tabpanel, aria-controls/aria-labelledby/aria-selected), roving tabindex и
// клавиатуру на #tab-strip (←/→/Home/End — перемещение фокуса с активацией,
// Delete и Ctrl+W — закрытие активной вкладки).
//
// Всё читаем одним снимком DOM через `browser.execute`: по элементным
// WebDriver-командам тянется проба окна tauri-service (~5 с на команду,
// см. комментарий в smoke.steps.js). Клавиатура — синтетический keydown на
// document.activeElement (конвенция palette/theme-toggle): обработчик висит на
// контейнере #tab-strip, а читает он именно document.activeElement.
import { Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

const TAB_SEL = "#tab-strip .tab";

/// Снимок ARIA-состояния вкладок в порядке отрисовки.
/// `tabIndex` — свойство элемента (roving tabindex выставляется свойством,
/// а не атрибутом, поэтому читаем именно его).
function tabAriaSnapshot() {
  return browser.execute((sel) =>
    Array.from(document.querySelectorAll(sel)).map((el) => ({
      id: el.id,
      role: el.getAttribute("role"),
      selected: el.getAttribute("aria-selected"),
      controls: el.getAttribute("aria-controls"),
      tabIndex: el.tabIndex,
    })), TAB_SEL);
}

/// Подпись панели #panes (aria-labelledby активной вкладки).
function panelLabelledBy() {
  return browser.execute(
    () => document.getElementById("panes")?.getAttribute("aria-labelledby") ?? null
  );
}

Then("все вкладки имеют роль {string} и aria-controls {string}", async (role, controls) => {
  const tabs = await tabAriaSnapshot();
  if (tabs.length === 0) throw new Error("В #tab-strip нет вкладок");
  const bad = tabs.filter((t) => t.role !== role || t.controls !== controls);
  if (bad.length > 0) {
    throw new Error(
      `Ожидалось role=${role} и aria-controls=${controls} у всех вкладок; ` +
        `нарушили: ${JSON.stringify(bad)}`
    );
  }
  // aria-controls должен вести на существующую панель, иначе связь висячая.
  const panelExists = await browser.execute(
    (c) => !!document.getElementById(c),
    controls
  );
  if (!panelExists) throw new Error(`aria-controls ссылается на несуществующий #${controls}`);
});

Then("выбрана ровно одна вкладка", async () => {
  await browser.waitUntil(
    async () => {
      const tabs = await tabAriaSnapshot();
      return tabs.length > 0 && tabs.filter((t) => t.selected === "true").length === 1;
    },
    {
      timeout: 5000,
      timeoutMsg: "Ровно одна вкладка не имеет aria-selected=true",
    }
  );
});

Then("aria-labelledby панели указывает на активную вкладку", async () => {
  await browser.waitUntil(
    async () => {
      const [tabs, labelledby] = await Promise.all([tabAriaSnapshot(), panelLabelledBy()]);
      const active = tabs.find((t) => t.selected === "true");
      if (!active || !labelledby) return false;
      if (labelledby !== active.id) return false;
      // Подпись должна совпадать с id активной вкладки, и элемент с таким id
      // обязан существовать: иначе скринридер объявит пустую панель.
      return browser.execute((id) => !!document.getElementById(id), labelledby);
    },
    {
      timeout: 5000,
      timeoutMsg: "#panes aria-labelledby не совпадает с id активной вкладки",
    }
  );
});

Then("в таб-порядке ровно одна вкладка", async () => {
  await browser.waitUntil(
    async () => {
      const tabs = await tabAriaSnapshot();
      return tabs.length > 0 && tabs.filter((t) => t.tabIndex === 0).length === 1;
    },
    {
      timeout: 5000,
      timeoutMsg: "Ровно одна вкладка с tabindex=0 не выполняется",
    }
  );
});

Then("у {int} вкладки tabindex равен {string}", async (index1, expected) => {
  const idx = index1 - 1;
  const actual = await browser.execute(
    (i) => document.querySelectorAll("#tab-strip .tab")[i]?.tabIndex ?? null,
    idx
  );
  const want = Number(expected);
  if (actual !== want) {
    throw new Error(
      `У вкладки №${index1} tabindex=${JSON.stringify(actual)}, ожидалось ${JSON.stringify(expected)}`
    );
  }
});

When("я фокусирую вкладку {int}", async (index1) => {
  const idx = index1 - 1;
  const ok = await browser.execute((i) => {
    const tab = document.querySelectorAll("#tab-strip .tab")[i];
    if (!tab) return false;
    tab.focus();
    return document.activeElement === tab;
  }, idx);
  if (!ok) throw new Error(`Вкладка №${index1} не найдена или не взяла фокус`);
  await browser.pause(30);
});

Then("фокус находится на {int} вкладке", async (index1) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (i) => document.activeElement === document.querySelectorAll("#tab-strip .tab")[i],
        index1 - 1
      ),
    { timeout: 5000, timeoutMsg: `Фокус не на вкладке №${index1}` }
  );
});

// Нажатие клавиши на сфокусированном элементе. Спецификация — либо имя клавиши
// ("ArrowRight"/"Home"/"End"/"Delete"), либо сочетание "Ctrl+W": префикс перед
// именем без пробела разбирается на модификатор и код. Одиночная заглавная
// буква — физическая клавиша: key приходит строчной ("w"), code — "KeyW"
// (в обработчике закрытия сравнивается именно code, независимо от раскладки).
When("я нажимаю клавишу {string}", async (spec) => {
  const combo = /^([A-Za-z]+)\+(.+)$/.exec(spec);
  const raw = combo ? combo[2] : spec;
  const modifiers = combo ? combo[1].split("+").map((m) => m.toLowerCase()) : [];
  const isLetter = /^[A-Za-z]$/.test(raw);
  const key = isLetter ? raw.toLowerCase() : raw;
  const code = isLetter ? `Key${raw.toUpperCase()}` : raw;
  await browser.execute(
    (k, c, mods) => {
      const target = document.activeElement ?? document.body;
      target.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: k,
          code: c,
          ctrlKey: mods.includes("ctrl"),
          shiftKey: mods.includes("shift"),
          altKey: mods.includes("alt"),
          metaKey: mods.includes("meta"),
          bubbles: true,
          cancelable: true,
        })
      );
    },
    key,
    code,
    modifiers
  );
  // Навигация переключает вкладку через store.switchTab (IPC) — даём кадр.
  await browser.pause(120);
});

Then("в редакторе текст {string}", async (expected) => {
  await browser.waitUntil(
    async () =>
      (await browser.execute(() => document.getElementById("editor").value)) === expected,
    { timeout: 8000, timeoutMsg: `В редакторе не тот текст, ожидался ${JSON.stringify(expected)}` }
  );
});
