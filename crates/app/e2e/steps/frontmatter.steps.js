// Шаги YAML-шапки (md-core::frontmatter) в реальном предпросмотре.
// Таблица шапки отмечена классом `md-frontmatter` (в т.ч. вложенные), поэтому
// ищем её напрямую, а не через украшенные `.table-enhanced`.
import { Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

Then("в предпросмотре есть таблица шапки", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => !!document.querySelector("#preview table.md-frontmatter")),
    { timeout: 8000, timeoutMsg: "Таблица шапки (.md-frontmatter) не появилась" }
  );
});

Then("в предпросмотре нет таблицы шапки", async () => {
  const exists = await browser.execute(
    () => !!document.querySelector("#preview table.md-frontmatter")
  );
  if (exists) throw new Error("Ожидали отсутствие таблицы шапки, но она есть");
});

Then("шапка содержит ключ {string} со значением {string}", async (key, value) => {
  const found = await browser.execute(
    (k, v) => {
      const th = [...document.querySelectorAll("#preview table.md-frontmatter th.fm-key")].find(
        (e) => e.textContent.trim() === k
      );
      const td = th?.parentElement?.querySelector("td.fm-val");
      return !!td && td.textContent.includes(v);
    },
    key,
    value
  );
  if (!found) throw new Error(`В шапке нет ключа «${key}» со значением «${value}»`);
});

Then(
  "в таблице шапки для ключа {string} вложенная таблица содержит {string}",
  async (key, value) => {
    const found = await browser.execute(
      (k, v) => {
        const th = [...document.querySelectorAll("#preview table.md-frontmatter th.fm-key")].find(
          (e) => e.textContent.trim() === k
        );
        const td = th?.parentElement?.querySelector("td.fm-val");
        if (!td) return false;
        return [...td.querySelectorAll("table.md-frontmatter")].some((t) =>
          t.textContent.includes(v)
        );
      },
      key,
      value
    );
    if (!found) {
      throw new Error(`Для ключа «${key}» нет вложенной таблицы со значением «${value}»`);
    }
  }
);

Then("в таблице шапки для ключа {string} список содержит пункты:", async (key, table) => {
  const expected = table.raw().map((row) => row[0]);
  const actual = await browser.execute((k) => {
    const th = [...document.querySelectorAll("#preview table.md-frontmatter th.fm-key")].find(
      (e) => e.textContent.trim() === k
    );
    const td = th?.parentElement?.querySelector("td.fm-val");
    const ul = td?.querySelector("ul.fm-list");
    return ul ? [...ul.querySelectorAll(":scope > li")].map((li) => li.textContent.trim()) : [];
  }, key);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `Список ключа «${key}» не совпал.\nОжидалось: ${expected.join(", ")}\nПолучено:  ${actual.join(", ")}`
    );
  }
});

Then("шапка показана как исходник", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => !!document.querySelector("#preview pre.md-frontmatter-source")
      ),
    { timeout: 8000, timeoutMsg: "Fallback-исходник шапки не появился" }
  );
});

Then("таблица шапки не украшена Excel-панелью", async () => {
  const problem = await browser.execute(() => {
    const t = document.querySelector("#preview table.md-frontmatter");
    if (!t) return "таблица шапки не найдена";
    if (t.closest(".table-enhanced")) return "обёрнута в .table-enhanced";
    if (t.querySelector(".col-filter-btn")) return "внутри есть воронка фильтра";
    return "";
  });
  if (problem) throw new Error(`Шапка не должна украшаться: ${problem}`);
});

Then("в предпросмотре украшено таблиц: {int}", async (expected) => {
  const actual = await browser.execute(
    () => document.querySelectorAll("#preview .table-enhanced").length
  );
  if (actual !== expected) {
    throw new Error(`Украшенных таблиц ${actual}, ожидалось ${expected}`);
  }
});

Then("шапка размечена для инспектора", async () => {
  const ok = await browser.execute(
    () => !!document.querySelector("#preview .md-block[data-md] table.md-frontmatter")
  );
  if (!ok) throw new Error("Шапка не обёрнута в .md-block[data-md]");
});

When("я навожу мышь на шапку", async () => {
  const raw = await browser.execute(() => {
    const preview = document.getElementById("preview");
    preview.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
    const block = [...preview.querySelectorAll(".md-block[data-md]")].find((b) =>
      b.querySelector("table.md-frontmatter")
    );
    if (!block) return null;
    const box = block.getBoundingClientRect();
    block.dispatchEvent(
      new MouseEvent("mouseover", {
        bubbles: true,
        cancelable: true,
        clientX: box.left + 1,
        clientY: box.top + 1,
      })
    );
    return block.getAttribute("data-md");
  });
  if (raw === null) throw new Error("Блок шапки для инспектора не найден");
  await browser.pause(30);
});
