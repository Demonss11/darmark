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
