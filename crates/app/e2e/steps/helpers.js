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

// ---------- Режим инспектора (AC-5, AC-10) ----------
//
// Инспектор вешается на событие `mouseover` предпросмотра и ставит выделение
// через editor.setSelectionRange. В E2E мы не двигаем настоящую мышь, а
// диспатчим MouseEvent на нужном [data-md]-блоке — это тот же обработчик.

/// Включает (true) или выключает (false) режим инспектора кнопкой тулбара.
///
/// На время активного инспектора предпросмотру отключается hit-testing
/// (`pointer-events: none`): иначе физический курсор, стоящий над предпросмотром,
/// порождает настоящие mouseover/mouseout (в том числе при перерисовке DOM
/// под курсором) и перебивает синтетический hover теста. `dispatchEvent`
/// доставляет события слушателям независимо от `pointer-events`.
export async function setInspectorActive(on) {
  await browser.execute((active) => {
    const btn = document.getElementById("btn-inspect");
    if (!btn) return;
    if (btn.classList.contains("active") !== active) btn.click();
  }, on);

  if (on) {
    await browser.execute(() => {
      const preview = document.getElementById("preview");
      if (preview) preview.style.pointerEvents = "none";
    });
    await browser.waitUntil(
      async () =>
        (await browser.execute(
          () => document.querySelectorAll("#preview [data-md]").length
        )) > 0,
      {
        timeout: 8000,
        timeoutMsg: "Режим инспектора не активировался (в предпросмотре нет data-md)",
      }
    );
  } else {
    await browser.execute(() => {
      const preview = document.getElementById("preview");
      if (preview) preview.style.pointerEvents = "";
    });
    await browser.pause(150); // даём main.ts перерисовать «чистый» HTML
  }
}

/// Список индексированных блоков: { raw: "start,end", text: видимый текст }.
export function inspectorBlocks() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll("#preview [data-md]")).map((el) => ({
      raw: el.getAttribute("data-md"),
      text: el.textContent.trim(),
    }))
  );
}

/// Наводит «мышь» на блок по точному значению data-md. Возвращает data-md.
export async function hoverBlockByRaw(raw) {
  const ok = await browser.execute((r) => {
    // Сбрасываем активный блок приложения: иначе его hysteresis
    // (`activeBlockEl === el`) может проигнорировать hover.
    const preview = document.getElementById("preview");
    preview.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
    const el = [...document.querySelectorAll("#preview [data-md]")].find(
      (e) => e.getAttribute("data-md") === r
    );
    if (!el) return false;
    const box = el.getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent("mouseover", {
        bubbles: true,
        clientX: box.left + 1,
        clientY: box.top + 1,
      })
    );
    return true;
  }, raw);
  if (!ok) throw new Error(`Блок data-md="${raw}" не найден в предпросмотре`);
  await browser.pause(30);
}

/// Наводит «мышь» на блок по его видимому тексту (для простых абзацев, где
/// textContent совпадает с исходником). Возвращает data-md блока.
export async function hoverBlockByText(text) {
  const raw = await browser.execute((t) => {
    const preview = document.getElementById("preview");
    preview.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
    const el = [...document.querySelectorAll("#preview [data-md]")].find(
      (e) => e.textContent.trim() === t
    );
    if (!el) return null;
    const box = el.getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent("mouseover", {
        bubbles: true,
        clientX: box.left + 1,
        clientY: box.top + 1,
      })
    );
    return el.getAttribute("data-md");
  }, text);
  if (raw === null) {
    throw new Error(`Блок с текстом ${JSON.stringify(text)} не найден`);
  }
  await browser.pause(30);
  return raw;
}

/// Текущее выделение textarea: UTF-16 индексы и выделенный текст.
export function editorSelection() {
  return browser.execute(() => {
    const editor = document.getElementById("editor");
    return {
      start: editor.selectionStart,
      end: editor.selectionEnd,
      text: editor.value.slice(editor.selectionStart, editor.selectionEnd),
    };
  });
}

/// Программно выделяет диапазон в редакторе. `setSelectionRange` не генерирует
/// `selectionchange`, поэтому уведомляем document вручную — обработчик тот же.
export async function selectEditorRange(start, end) {
  await browser.execute(
    (s, e) => {
      const editor = document.getElementById("editor");
      editor.focus();
      editor.setSelectionRange(s, e);
      document.dispatchEvent(new Event("selectionchange", { bubbles: true }));
    },
    start,
    end
  );
  await browser.pause(30);
}

/// Кликает по блоку предпросмотра (по видимому тексту): mousedown/mouseup/click.
export async function clickBlockByText(text) {
  const ok = await browser.execute((t) => {
    const el = [...document.querySelectorAll("#preview [data-md]")].find(
      (e) => e.textContent.trim() === t
    );
    if (!el) return false;
    const box = el.getBoundingClientRect();
    const opts = {
      bubbles: true,
      cancelable: true,
      clientX: box.left + 1,
      clientY: box.top + 1,
    };
    el.dispatchEvent(new MouseEvent("mousedown", opts));
    el.dispatchEvent(new MouseEvent("mouseup", opts));
    el.dispatchEvent(new MouseEvent("click", opts));
    return true;
  }, text);
  if (!ok) throw new Error(`Блок ${JSON.stringify(text)} не найден в предпросмотре`);
  await browser.pause(30);
}

/// Текст блока предпросмотра с классом `.inspect-active` (или null).
export function activeInspectText() {
  return browser.execute(() => {
    const el = document.querySelector("#preview .inspect-active");
    return el ? el.textContent.trim() : null;
  });
}

/// id элемента в фокусе ("" — фокус вне DOM-элемента с id).
export function activeElementId() {
  return browser.execute(() => document.activeElement?.id ?? "");
}

/// Эталонный перевод байтового смещения (UTF-8) в UTF-16-индекс по значению
/// редактора. Независимая реализация для сверки с inspector.ts.
export function refBytesToUnits(byteStart, byteEnd) {
  return browser.execute(
    (bStart, bEnd) => {
      const value = document.getElementById("editor").value;
      const utf8Len = (cp) => (cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4);
      const b2u = (target) => {
        let bytes = 0;
        let units = 0;
        for (const ch of value) {
          if (bytes >= target) break;
          const cp = ch.codePointAt(0);
          bytes += utf8Len(cp);
          units += cp > 0xffff ? 2 : 1;
        }
        return units;
      };
      return { start: b2u(bStart), end: b2u(bEnd) };
    },
    byteStart,
    byteEnd
  );
}
