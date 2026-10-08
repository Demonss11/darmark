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

Then("у элемента {string} нет фокус-ринга", async (selector) => {
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
  if (info.style !== "none" && info.width > 0) {
    throw new Error(
      `У ${selector} остался фокус-ринг (outline: ${info.style} ${info.width}px)`
    );
  }
});

// Переключение тумблера как кликом мыши: рамка не должна оставаться.
// Скрытый input (`pointer-events: none`, нулевая геометрия) нативным WebDriver
// кликнуть нельзя, поэтому кликаем по видимой обёртке label реальным указателем —
// это и переключает input, и переводит браузер в «мышиную» модальность
// (синтетические события и программный `.focus()` модальность не меняют).
When("я переключаю тумблер {string}", async (selector) => {
  // Детерминированный старт: снимаем фокус (предыдущий сценарий мог оставить
  // `:focus-visible`), чтобы pointer-клик задал «мышиную» модальность.
  await browser.execute(() => {
    const active = document.activeElement;
    if (active && typeof active.blur === "function") active.blur();
  });
  const labelId = await browser.execute((s) => {
    const input = document.querySelector(s);
    if (!input) return null;
    const label = input.closest("label");
    if (!label) return null;
    if (!label.id) label.id = "e2e-toggle-label";
    return label.id;
  }, selector);
  if (!labelId) throw new Error(`Тумблер ${selector} не найден`);
  await browser.$(`#${labelId}`).click();
  await browser.pause(50);
});

// Настоящее сочетание клавиш (не синтетическое событие): только оно переводит
// браузер в «клавиатурную» модальность и включает `:focus-visible`.
When("я нажимаю Shift+Tab", async () => {
  await browser.keys(["\uE008", "\uE004"]);
  await browser.pause(50);
});

// ---------- Регрессии M1/M3/M4/M5/M6/M7 ----------

Then(
  "у элемента {string} атрибут {string} равен {string}",
  async (selector, attr, expected) => {
    const actual = await browser.execute(
      (s, a) => document.querySelector(s)?.getAttribute(a) ?? null,
      selector,
      attr
    );
    if (actual !== expected) {
      throw new Error(
        `У ${selector} ${attr}=${JSON.stringify(actual)}, ожидалось ${JSON.stringify(expected)}`
      );
    }
  }
);

// M3: aria-sort на самом th (подпись столбца — по видимому тексту).
Then(
  "у заголовка {string} значение aria-sort равно {string}",
  async (name, expected) => {
    const actual = await browser.execute((col) => {
      const th = Array.from(
        document.querySelectorAll(".table-enhanced thead th")
      ).find((t) => t.textContent.includes(col));
      return th?.getAttribute("aria-sort") ?? null;
    }, name);
    if (actual !== expected) {
      throw new Error(
        `У заголовка ${JSON.stringify(name)} aria-sort=${JSON.stringify(actual)}, ожидалось ${JSON.stringify(expected)}`
      );
    }
  }
);

// M5: Escape в открытом поповере фильтра.
When("я нажимаю Escape", async () => {
  await browser.execute(() => {
    document.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true })
    );
  });
  await browser.pause(30);
});

// M5: фокус вернулся на воронку (кнопка `.col-filter-btn`).
Then("фокус вернулся на воронку фильтра", async () => {
  const onFunnel = await browser.execute(() => {
    const el = document.activeElement;
    return !!el && el.classList.contains("col-filter-btn");
  });
  if (!onFunnel) throw new Error("Фокус не вернулся на воронку фильтра");
});

// M1: не эмулируем медиафичу (WebDriver этого не умеет), но проверяем, что
// правило prefers-reduced-motion реально присутствует в подключённых стилях.
Then("в таблицах стилей есть медиазапрос prefers-reduced-motion", async () => {
  const found = await browser.execute(() => {
    for (const sheet of Array.from(document.styleSheets)) {
      let rules;
      try {
        rules = sheet.cssRules;
      } catch {
        continue; // недоступный (кросс-доменный) sheet
      }
      for (const rule of Array.from(rules ?? [])) {
        if (rule.media && /prefers-reduced-motion/i.test(rule.media.mediaText)) {
          return true;
        }
      }
    }
    return false;
  });
  if (!found) throw new Error("В стилях нет @media (prefers-reduced-motion)");
});
