// Шаги панели «Журнал» (S11, ADR-0029).
import { Given, Then } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

// ---------- Состояние панели ----------

Then("панель журнала открыта", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const panel = document.getElementById("logs-panel");
        return !!panel && !panel.hidden && !panel.classList.contains("collapsed");
      }),
    { timeout: 5_000, timeoutMsg: "Панель журнала не открылась" }
  );
});

Then("панель журнала свёрнута", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const panel = document.getElementById("logs-panel");
        return !!panel && !panel.hidden && panel.classList.contains("collapsed");
      }),
    { timeout: 5_000, timeoutMsg: "Панель журнала не свёрнута" }
  );
});

Then("статусбар не содержит флеш «Журнал — скоро»", async () => {
  const text = await browser.execute(
    () => document.getElementById("stat-msg")?.textContent ?? ""
  );
  if (text.includes("Журнал — скоро")) {
    throw new Error("Статусбар содержит флеш «Журнал — скоро»");
  }
});

// ---------- Рабочая область ----------

Given("я запоминаю высоту рабочей области", async () => {
  const height = await browser.execute(
    () => document.getElementById("editor").getBoundingClientRect().height
  );
  browser.e2e_editorHeightBefore = height;
});

Then("высота рабочей области уменьшилась", async () => {
  const height = await browser.execute(
    () => document.getElementById("editor").getBoundingClientRect().height
  );
  const before = browser.e2e_editorHeightBefore;
  if (!(height < before)) {
    throw new Error(`Высота рабочей области не уменьшилась: ${height} >= ${before}`);
  }
});

Then("редактор виден", async () => {
  const visible = await browser.execute(() => {
    const el = document.getElementById("editor");
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  });
  if (!visible) throw new Error("Редактор не виден");
});

Then("предпросмотр виден", async () => {
  const visible = await browser.execute(() => {
    const el = document.getElementById("preview");
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  });
  if (!visible) throw new Error("Предпросмотр не виден");
});

// ---------- Бейдж непрочитанных ----------

Then("бейдж журнала скрыт", async () => {
  const hidden = await browser.execute(() => {
    const el = document.getElementById("log-badge");
    return !!el && el.hidden;
  });
  if (!hidden) throw new Error("Бейдж журнала не скрыт");
});

Then("бейдж журнала виден", async () => {
  const visible = await browser.execute(() => {
    const el = document.getElementById("log-badge");
    return !!el && !el.hidden;
  });
  if (!visible) throw new Error("Бейдж журнала не виден");
});

Then("бейдж журнала показывает {string}", async (expected) => {
  const text = await browser.execute(
    () => document.getElementById("log-badge")?.textContent?.trim() ?? ""
  );
  if (text !== expected) {
    throw new Error(`Бейдж журнала показывает ${JSON.stringify(text)}, ожидалось ${JSON.stringify(expected)}`);
  }
});

// ---------- Лента: записи, фильтр, очистка, даты (Фаза 3, AC-8/9/11) ----------
//
// Записи приходят из Rust-событий (`document-event`, `plugin-status-changed`, …).
// Мы не подделываем их напрямую, а порождаем пользовательскими действиями без
// нативных диалогов: «Новый файл» (#btn-new) → запись info; включение падающего
// плагина e2e-fail → запись error. «Видимость» записи определяем по геометрии
// (`getClientRects`), а уровень — по стабильному атрибуту `data-level`.

/// Видимые (реально отрисованные) записи ленты: уровень + текст сообщения.
function visibleLogItems() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll("#logs-list .log-item"))
      .filter((el) => el.getClientRects().length > 0)
      .map((el) => ({
        level: el.dataset.level ?? "info",
        message: el.querySelector(".log-msg")?.textContent ?? "",
      }))
  );
}

Then("в ленте журнала есть записи", async () => {
  await browser.waitUntil(
    async () => (await visibleLogItems()).length > 0,
    { timeout: 5_000, timeoutMsg: "В ленте журнала нет записей" }
  );
});

Then("лента журнала пуста", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const list = document.getElementById("logs-list");
        return (
          !!list &&
          list.querySelectorAll(".log-item").length === 0 &&
          !!list.querySelector(".log-empty")
        );
      }),
    {
      timeout: 5_000,
      timeoutMsg: "Лента журнала не опустошилась (нет .log-empty или остались записи)",
    }
  );
});

Then("в ленте журнала есть запись уровня ошибки", async () => {
  await browser.waitUntil(
    async () => (await visibleLogItems()).some((it) => it.level === "error"),
    { timeout: 5_000, timeoutMsg: "В ленте нет записи уровня error" }
  );
});

Then("в ленте журнала есть информационная запись", async () => {
  await browser.waitUntil(
    async () => (await visibleLogItems()).some((it) => it.level === "info"),
    { timeout: 5_000, timeoutMsg: "В ленте нет информационной записи" }
  );
});

Then("среди видимых записей журнала нет информационных", async () => {
  const isFiltered = async () => {
    const items = await visibleLogItems();
    return items.length > 0 && items.every((it) => it.level !== "info");
  };
  try {
    await browser.waitUntil(isFiltered, { timeout: 5_000, interval: 100 });
  } catch {
    const items = await visibleLogItems();
    if (items.length === 0) {
      throw new Error(
        "Фильтр «Ошибки» опустошил ленту: записей warn/error не осталось"
      );
    }
    throw new Error(
      "Фильтр «Ошибки» показывает информационные записи: " +
        JSON.stringify(items.filter((it) => it.level === "info").map((it) => it.message))
    );
  }
});

Then("в ленте журнала есть разделитель даты «Сегодня»", async () => {
  await browser.waitUntil(
    async () => {
      const texts = await browser.execute(() =>
        Array.from(
          document.querySelectorAll("#logs-list .log-date-separator")
        ).map((el) => el.textContent.trim())
      );
      return texts.includes("Сегодня");
    },
    { timeout: 5_000, timeoutMsg: "В ленте нет разделителя даты «Сегодня»" }
  );
});
