// Шаги палитры команд и диалог-инфраструктуры.
//
// Открытие — реальным фокусом и кликом по `#palette-trigger` (как пользователь):
// тогда инициатор фокуса — сама кнопка, и Escape обязан вернуть фокус на неё.
// Клавиши отправляем синтетическими `keydown` в текущий активный элемент —
// это те же обработчики, что у реальной клавиатуры (dialog.ts/palette.ts).
import { Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

const PALETTE_INPUT = ".dialog .palette-input";

function paletteOpen() {
  return browser.execute(
    (sel) => !!document.querySelector(sel),
    PALETTE_INPUT
  );
}

async function waitPaletteOpen() {
  await browser.waitUntil(paletteOpen, {
    timeout: 5000,
    timeoutMsg: "Палитра команд не открылась",
  });
}

When("я открываю палитру команд", async () => {
  const ok = await browser.execute(() => {
    const btn = document.getElementById("palette-trigger");
    if (!btn) return false;
    btn.focus(); // как реальный пользователь: фокус на кнопке до клика
    btn.click();
    return true;
  });
  if (!ok) throw new Error("#palette-trigger не найдена");
  await waitPaletteOpen();
});

When("я нажимаю Ctrl+K", async () => {
  await browser.execute(() => {
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: "k",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      })
    );
  });
  await waitPaletteOpen();
});

When("я ввожу в палитру {string}", async (query) => {
  const ok = await browser.execute((q) => {
    const input = document.querySelector(".palette-input");
    if (!input) return false;
    input.value = q;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  }, query);
  if (!ok) throw new Error("Поле ввода палитры не найдено");
  await browser.pause(30);
});

When("я нажимаю в палитре {string}", async (spec) => {
  const ok = await browser.execute((keys) => {
    const target = document.activeElement ?? document.body;
    const parts = keys.split("+");
    const shiftKey = parts.includes("Shift");
    const ctrlKey = parts.includes("Ctrl");
    const altKey = parts.includes("Alt");
    const metaKey = parts.includes("Meta");
    const key = parts[parts.length - 1];
    target.dispatchEvent(
      new KeyboardEvent("keydown", {
        key,
        shiftKey,
        ctrlKey,
        altKey,
        metaKey,
        bubbles: true,
        cancelable: true,
      })
    );
    return true;
  }, spec);
  if (!ok) throw new Error(`Не удалось нажать ${spec}`);
  await browser.pause(30);
});

When("я кликаю по подложке диалога", async () => {
  const ok = await browser.execute(() => {
    const backdrop = document.querySelector(".dialog-backdrop");
    if (!backdrop) return false;
    // Клик именно по подложке (target === overlay), как настоящая мышь вне панели.
    backdrop.click();
    return true;
  });
  if (!ok) throw new Error("Подложка диалога не найдена");
  await browser.pause(30);
});

Then("палитра команд открыта", async () => {
  if (!(await paletteOpen())) throw new Error("Палитра команд не открыта");
});

Then("палитра команд закрыта", async () => {
  await browser.waitUntil(
    async () => !(await paletteOpen()),
    { timeout: 3000, timeoutMsg: "Палитра команд осталась открытой" }
  );
});

Then("элемент {string} инертен", async (selector) => {
  const inert = await browser.execute(
    (s) => document.querySelector(s)?.hasAttribute("inert") ?? false,
    selector
  );
  if (!inert) throw new Error(`${selector} не инертен`);
});

Then("элемент {string} не инертен", async (selector) => {
  const inert = await browser.execute(
    (s) => document.querySelector(s)?.hasAttribute("inert") ?? false,
    selector
  );
  if (inert) throw new Error(`${selector} остался инертным`);
});

Then("фокус в палитре на поле ввода", async () => {
  const ok = await browser.execute(
    () => document.activeElement?.classList.contains("palette-input") ?? false
  );
  if (!ok) throw new Error("Фокус не на поле ввода палитры");
});

Then("фокус на элементе {string}", async (selector) => {
  const ok = await browser.execute(
    (s) => document.activeElement === document.querySelector(s),
    selector
  );
  if (!ok) throw new Error(`Фокус не на ${selector}`);
});

Then("фокус внутри {string}", async (selector) => {
  const ok = await browser.execute((s) => {
    const root = document.querySelector(s);
    return !!root && !!document.activeElement && root.contains(document.activeElement);
  }, selector);
  if (!ok) throw new Error(`Фокус вне ${selector}`);
});

/// Заголовки пунктов палитры (сравнение по точному тексту).
function paletteTitles() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".palette-item .palette-title")).map((el) =>
      el.textContent.trim()
    )
  );
}

Then("в палитре есть пункт {string}", async (title) => {
  await browser.waitUntil(
    async () => (await paletteTitles()).includes(title),
    { timeout: 5000, timeoutMsg: `В палитре нет пункта ${title}` }
  );
});

Then("в палитре нет пункта {string}", async (title) => {
  if ((await paletteTitles()).includes(title)) {
    throw new Error(`В палитре неожиданно есть пункт ${title}`);
  }
});

Then("в палитре виден пустой результат", async () => {
  const ok = await browser.execute(() => !!document.querySelector(".palette-empty"));
  if (!ok) throw new Error("Нет заглушки пустого результата");
});

/// Заголовок пункта с aria-selected="true" (текущий выбор клавиатуры).
function paletteActiveTitle() {
  return browser.execute(() => {
    const item = document.querySelector('.palette-item[aria-selected="true"]');
    return item?.querySelector(".palette-title")?.textContent?.trim() ?? null;
  });
}

Then("активный пункт палитры {string}", async (title) => {
  const actual = await paletteActiveTitle();
  if (actual !== title) {
    throw new Error(
      `Активный пункт палитры = ${JSON.stringify(actual)}, ожидался ${JSON.stringify(title)}`
    );
  }
});

/// Наводит «мышь» на пункт палитры по заголовку (событие mouseover, как у курсора).
When("я навожу мышь на пункт палитры {string}", async (title) => {
  const ok = await browser.execute((t) => {
    const item = Array.from(document.querySelectorAll(".palette-item")).find(
      (el) => el.querySelector(".palette-title")?.textContent?.trim() === t
    );
    if (!item) return false;
    item.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
    return true;
  }, title);
  if (!ok) throw new Error(`Пункт палитры ${JSON.stringify(title)} не найден`);
  await browser.pause(30);
});

/// aria-activedescendant поля обязан указывать на текущий активный пункт:
/// иначе скринридер не свяжет ввод со списком.
Then("у поля палитры aria-activedescendant указывает на активный пункт", async () => {
  const ok = await browser.execute(() => {
    const input = document.querySelector(".palette-input");
    const id = input?.getAttribute("aria-activedescendant");
    if (!id) return false;
    const active = document.querySelector('.palette-item[aria-selected="true"]');
    return !!active && active.id === id;
  });
  if (!ok) throw new Error("aria-activedescendant не указывает на активный пункт");
});

Then("тумблер {string} выключен", async (selector) => {
  const checked = await browser.execute(
    (s) => document.querySelector(s)?.checked ?? false,
    selector
  );
  if (checked) throw new Error(`Тумблер ${selector} остался включён`);
});

Then("режим инспектора активен", async () => {
  const active = await browser.execute(
    () => document.getElementById("btn-inspect")?.classList.contains("active") ?? false
  );
  if (!active) throw new Error("Режим инспектора не активен");
});

Then("виден тост ошибки", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => !!document.querySelector("#toast-host .toast-error")),
    { timeout: 3000, timeoutMsg: "Тост ошибки не появился" }
  );
});

Then("в runtime нет необработанных ошибок", async () => {
  const errors = await browser.execute(() => (window.__errors ?? []).slice());
  if (errors.length) throw new Error(`Ошибки в runtime: ${errors.join(" | ")}`);
});
