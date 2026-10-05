// Общие действия E2E-сценариев: работа с редактором, предпросмотром и таблицами.
// Всё через `browser.execute` в контексте webview — без хрупких CSS-цепочек там,
// где проще обратиться к DOM напрямую.
import { browser } from "@wdio/globals";

/// Устанавливает markdown в редактор и дожидается перерисовки предпросмотра.
/// Пустой документ перед рабочим — чтобы гарантированно обойти защиту
/// `lastRenderedHtml` в main.ts (иначе DOM таблиц не перестраивается).
export async function setMarkdown(markdown) {
  await setEditorValue("");
  await browser.pause(250);
  await setEditorValue(markdown);
  await browser.pause(250);
}

async function setEditorValue(value) {
  await browser.execute((v) => {
    const el = document.getElementById("editor");
    el.value = v;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }, value);
}

export function previewHtml() {
  return browser.execute(() => document.getElementById("preview").innerHTML);
}

export async function waitPreviewContains(fragment) {
  await browser.waitUntil(
    async () => (await previewHtml()).includes(fragment),
    { timeout: 8000, timeoutMsg: `В предпросмотре не появилось: ${fragment}` }
  );
}

/// Видимые (не скрытые фильтром) строки украшенной таблицы, как массив ячеек.
export function visibleRows() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".table-enhanced tbody tr"))
      .filter((row) => !row.hidden)
      .map((row) => Array.from(row.cells).map((cell) => cell.textContent.trim()))
  );
}

export function tableCounter() {
  return browser.execute(
    () => document.querySelector(".table-count")?.textContent?.trim() ?? ""
  );
}

export async function waitTableEnhanced() {
  await browser.waitUntil(
    async () =>
      (await browser.execute(
        () => document.querySelectorAll(".table-enhanced .col-filter-btn").length
      )) > 0,
    { timeout: 8000, timeoutMsg: "Таблица в предпросмотре так и не украшена" }
  );
}

export async function clickHeader(name) {
  const ok = await browser.execute((col) => {
    const th = Array.from(
      document.querySelectorAll(".table-enhanced thead th")
    ).find((t) => t.textContent.includes(col));
    if (!th) return false;
    th.click();
    return true;
  }, name);
  if (!ok) throw new Error(`Не найден заголовок столбца: ${name}`);
}

export async function openFilter(name) {
  const ok = await browser.execute((col) => {
    const th = Array.from(
      document.querySelectorAll(".table-enhanced thead th")
    ).find((t) => t.textContent.includes(col));
    const btn = th?.querySelector(".col-filter-btn");
    if (!btn) return false;
    btn.click();
    return true;
  }, name);
  if (!ok) throw new Error(`Не найдена воронка фильтра для столбца: ${name}`);
  await browser.waitUntil(
    async () =>
      (await browser.execute(
        () => document.querySelectorAll(".col-filter-menu").length
      )) > 0,
    { timeout: 5000, timeoutMsg: "Меню фильтра не открылось" }
  );
}

export async function setFilterMiniSearch(query) {
  await browser.execute((q) => {
    const input = document.querySelector('.col-filter-menu input[type="search"]');
    input.value = q;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, query);
}

export function visibleFilterItems() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll(".col-filter-item"))
      .filter((item) => !item.hidden)
      .map((item) => item.dataset.value ?? "")
  );
}

export async function clickFilterButton(label) {
  const ok = await browser.execute((text) => {
    const btn = Array.from(
      document.querySelectorAll(".col-filter-footer button")
    ).find((b) => b.textContent.trim() === text);
    if (!btn) return false;
    btn.click();
    return true;
  }, label);
  if (!ok) throw new Error(`Не найдена кнопка фильтра: ${label}`);
}
