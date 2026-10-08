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
    // Сортировка — на кнопке внутри th (сам th сохраняет роль columnheader).
    const btn = th.querySelector(".col-sort-btn");
    (btn ?? th).click();
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
          () => document.querySelectorAll("#preview .md-block[data-md]").length
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
    Array.from(document.querySelectorAll("#preview .md-block[data-md]")).map((el) => ({
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
    const el = [...document.querySelectorAll("#preview .md-block[data-md]")].find(
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
    const el = [...document.querySelectorAll("#preview .md-block[data-md]")].find(
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
    const el = [...document.querySelectorAll("#preview .md-block[data-md]")].find(
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

// ---------- Синхронизация скролла (TZ-scroll-sync-v2) ----------
//
// Скролл задаётся программно (`scrollTop`) + синтетическим событием `scroll`,
// потому что нативное событие от программной установки приходит асинхронно и
// не гарантирует порядок с rAF-записью sync. Пауза 160 мс пережидает окно
// игнорирования эха (ECHO_MS = 100 мс) и кадр записи.

const ECHO_SLACK = 160;

/// Включает/выключает тумблер «синхронно» (#chk-sync).
export async function setSyncEnabled(on) {
  await browser.execute((v) => {
    const cb = document.getElementById("chk-sync");
    if (cb.checked !== v) cb.click();
  }, on);
  await browser.pause(ECHO_SLACK);
}

export function setPreviewVisible(on) {
  return browser.execute((v) => {
    const cb = document.getElementById("chk-preview");
    if (cb.checked !== v) cb.click();
  }, on);
}

/// Метрики панели: scrollTop/max, измеренные lineHeight и paddingTop редактора.
export function scrollState(which) {
  return browser.execute((w) => {
    const el = document.getElementById(w === "editor" ? "editor" : "preview");
    const cs = getComputedStyle(el);
    const lh = parseFloat(cs.lineHeight);
    const fs = parseFloat(cs.fontSize);
    const r = el.getBoundingClientRect();
    return {
      scrollTop: el.scrollTop,
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
      max: el.scrollHeight - el.clientHeight,
      lineHeight: Number.isFinite(lh) && lh > 0 ? lh : Number.isFinite(fs) ? fs * 1.2 : 21,
      paddingTop: parseFloat(cs.paddingTop) || 0,
      scrollWidth: el.scrollWidth,
      clientWidth: el.clientWidth,
      rectTop: r.top,
      rectBottom: r.bottom,
    };
  }, which);
}

export async function setEditorScrollTop(px) {
  await browser.pause(ECHO_SLACK);
  await browser.execute((p) => {
    const el = document.getElementById("editor");
    el.scrollTop = p;
    el.dispatchEvent(new Event("scroll", { bubbles: false }));
  }, px);
  await browser.pause(ECHO_SLACK);
}

/// Вставляет n строк в начало документа и дожидается перерисовки.
export async function insertLinesAtTop(n) {
  await browser.execute((count) => {
    const el = document.getElementById("editor");
    el.value = "Строка-для-сдвига.\n".repeat(count) + el.value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }, n);
  await browser.pause(250); // debounce 120 мс + рендер
}

/// Вставляет n строк в начало, сразу прокручивает редактор на pct% и
/// возвращает долю предпросмотра, замеренную ДО истечения debounce-рендера.
/// Два rAF (~32 мс) дают sync записать scrollTop и снять syncing, но не дают
/// сработать debounce (120 мс). Всё — в одном execute, иначе round-trip
/// WebDriver сам переждёт дебаунс и рендер успеет пройти.
export async function insertLinesAtTopAndScrollImmediately(n, pct) {
  return browser.execute(async (count, p) => {
    const editor = document.getElementById("editor");
    const preview = document.getElementById("preview");

    editor.value = "Строка-для-сдвига.\n".repeat(count) + editor.value;
    editor.dispatchEvent(new Event("input", { bubbles: true }));

    const max = editor.scrollHeight - editor.clientHeight;
    editor.scrollTop = (max * p) / 100;
    editor.dispatchEvent(new Event("scroll", { bubbles: false }));

    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));

    const pmax = preview.scrollHeight - preview.clientHeight;
    return {
      editorRatio: max > 0 ? editor.scrollTop / max : 0,
      previewRatio: pmax > 0 ? preview.scrollTop / pmax : 0,
    };
  }, n, pct);
}

/// Позиция первой (0-based) логической строки, содержащей подстроку.
export function lineOfSubstring(text) {
  return browser.execute((t) => {
    const v = document.getElementById("editor").value;
    const i = v.indexOf(t);
    return i < 0 ? -1 : v.slice(0, i).split("\n").length - 1;
  }, text);
}

/// Прокручивает редактор так, чтобы строка `lineIndex` стала верхней
/// (повторяет формулу sync: topLine = floor(scrollTop / lineH)).
export async function scrollEditorToLine(lineIndex) {
  const st = await scrollState("editor");
  const target = Math.max(0, Math.min(st.max, lineIndex * st.lineHeight));
  await setEditorScrollTop(target);
}

/// Прокручивает редактор к началу строки, содержащей подстроку.
export async function scrollEditorToSubstring(text) {
  const line = await lineOfSubstring(text);
  if (line < 0) throw new Error(`В редакторе нет подстроки ${JSON.stringify(text)}`);
  await scrollEditorToLine(line);
}

/// Независимый перевод номера логической строки в байтовое смещение её начала.
export function byteAtEditorLine(line) {
  return browser.execute((target) => {
    if (target <= 0) return 0;
    const v = document.getElementById("editor").value;
    const len = (cp) => (cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4);
    let bytes = 0;
    let lineNo = 0;
    for (const ch of v) {
      bytes += len(ch.codePointAt(0));
      if (ch === "\n") {
        lineNo++;
        if (lineNo === target) return bytes;
      }
    }
    return bytes;
  }, line);
}

/// Номер верхней логической строки редактора по его scrollTop (формула §5.1:
/// topLine = floor(scrollTop / lineH)). Она же — инверсия формулы §5.2, поэтому
/// обе стороны синхронизации должны сходиться на одном значении.
export function editorTopLine() {
  return browser.execute(() => {
    const el = document.getElementById("editor");
    const cs = getComputedStyle(el);
    let lh = parseFloat(cs.lineHeight);
    if (!(lh > 0)) lh = parseFloat(cs.fontSize) * 1.2;
    return Math.floor(el.scrollTop / lh);
  });
}

/// Блок в начале абзаца, содержащего байтовое смещение `byteOffset` (независимо).
export function expectedBlockTextAtByte(byteOffset) {
  return browser.execute((b) => {
    const blocks = [...document.querySelectorAll("#preview .md-block[data-md]")]
      .map((el) => {
        const [s, e] = el.getAttribute("data-md").split(",").map(Number);
        return { start: s, end: e, text: el.textContent.trim() };
      })
      .sort((a, c) => a.start - c.start);
    for (const bl of blocks) if (b >= bl.start && b < bl.end) return bl.text;
    for (const bl of blocks) if (bl.start > b) return bl.text;
    return null;
  }, byteOffset);
}

/// Верхний видимый [data-md]-блок предпросмотра.
export function previewTopBlockText() {
  return browser.execute(() => {
    const preview = document.getElementById("preview");
    const prTop = preview.getBoundingClientRect().top;
    const blocks = [...preview.querySelectorAll(".md-block[data-md]")];
    let found = null;
    for (const el of blocks) {
      if (el.getBoundingClientRect().bottom >= prTop) {
        found = el;
        break;
      }
    }
    if (!found && blocks.length) found = blocks[blocks.length - 1];
    return found ? found.textContent.trim() : null;
  });
}

/// Прокручивает предпросмотр так, чтобы блок `text` оказался сверху.
export async function scrollPreviewToBlock(text) {
  await browser.pause(ECHO_SLACK);
  const ok = await browser.execute((t) => {
    const preview = document.getElementById("preview");
    const el = [...preview.querySelectorAll(".md-block[data-md]")].find(
      (e) => e.textContent.trim() === t
    );
    if (!el) return false;
    const pr = preview.getBoundingClientRect();
    const br = el.getBoundingClientRect();
    const pad = parseFloat(getComputedStyle(preview).paddingTop) || 0;
    preview.scrollTop = preview.scrollTop + (br.top - pr.top) - pad;
    preview.dispatchEvent(new Event("scroll", { bubbles: false }));
    return true;
  }, text);
  if (!ok) throw new Error(`Блок ${JSON.stringify(text)} не найден в предпросмотре`);
  await browser.pause(ECHO_SLACK);
}

/// Прокручивает внутреннюю область таблицы (не панель предпросмотра).
export async function scrollInnerTable() {
  await browser.pause(ECHO_SLACK);
  const ok = await browser.execute(() => {
    const scroller = document.querySelector("#preview .table-scroll");
    if (!scroller) return false;
    scroller.scrollTop = 120;
    scroller.dispatchEvent(new Event("scroll", { bubbles: false }));
    return true;
  });
  if (!ok) throw new Error("В предпросмотре нет .table-scroll");
  await browser.pause(ECHO_SLACK);
}

/// Ошибки, пойманные глобальным обработчиком `error` (см. hooks.js).
export function capturedErrors() {
  return browser.execute(() => (window.__errors ?? []).slice());
}

/// Быстрая серия прокруток редактора (имитация колеса): несколько позиций
/// подряд в одном кадре. Возвращает { target, actual } финальной позиции.
export async function rapidScrollEditorToSubstring(text) {
  await browser.pause(ECHO_SLACK);
  const res = await browser.execute((t) => {
    const el = document.getElementById("editor");
    const cs = getComputedStyle(el);
    let lh = parseFloat(cs.lineHeight);
    if (!(lh > 0)) lh = parseFloat(cs.fontSize) * 1.2;
    const v = el.value;
    const idx = v.indexOf(t);
    if (idx < 0) return { target: -1 };
    const line = v.slice(0, idx).split("\n").length - 1;
    const max = el.scrollHeight - el.clientHeight;
    const target = Math.max(0, Math.min(max, line * lh));
    for (let f = 0.2; f <= 1.0001; f += 0.1) {
      el.scrollTop = target * f;
      el.dispatchEvent(new Event("scroll", { bubbles: false }));
    }
    el.scrollTop = target;
    el.dispatchEvent(new Event("scroll", { bubbles: false }));
    return { target, actual: el.scrollTop };
  }, text);
  if (res.target < 0) throw new Error(`В редакторе нет ${JSON.stringify(text)}`);
  await browser.pause(400); // дать sync досчитать после серии
  return res;
}

/// Наводит «мышь» на далёкий блок по видимому тексту (через инспектор).
export async function hoverBlockFar(text) {
  await browser.pause(ECHO_SLACK);
  const raw = await browser.execute((t) => {
    const preview = document.getElementById("preview");
    preview.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
    const el = [...preview.querySelectorAll(".md-block[data-md]")].find(
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
  if (raw === null) throw new Error(`Блок ${JSON.stringify(text)} не найден`);
  await browser.pause(400); // rAF + suspend-кадр + scrollIntoView
  return raw;
}


// ---------- Гранулярность инспектора для таблиц (TZ-inspect-tables) ----------
//
// После разметки tr/td/th атрибутом data-md инспектор должен подсвечивать
// ячейку/строку/столбец, а не всю таблицу. Наведение, как и в блоковых шагах,
// эмулируется синтетическим mouseover прямо на целевом элементе. Shift
// выставляется в свойстве shiftKey события (для выбора строки).

/// Наводит «мышь» на ячейку <td> по её видимому тексту. shift=true → строка.
export async function hoverCellByText(text, shift = false) {
  const raw = await browser.execute(
    (t, sh) => {
      const preview = document.getElementById("preview");
      preview.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
      const td = [...document.querySelectorAll("#preview .table-enhanced td[data-md]")].find(
        (e) => e.textContent.trim() === t
      );
      if (!td) return null;
      const box = td.getBoundingClientRect();
      td.dispatchEvent(
        new MouseEvent("mouseover", {
          bubbles: true,
          cancelable: true,
          shiftKey: sh,
          clientX: box.left + 1,
          clientY: box.top + 1,
        })
      );
      return td.getAttribute("data-md");
    },
    text,
    shift
  );
  if (raw === null) throw new Error(`Ячейка ${JSON.stringify(text)} не найдена`);
  await browser.pause(30);
  return raw;
}

/// Наводит «мышь» на заголовок столбца <th> по видимому тексту (без воронки ▾).
export async function hoverHeaderByText(name, shift = false) {
  const raw = await browser.execute(
    (n, sh) => {
      const preview = document.getElementById("preview");
      preview.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
      const th = [...document.querySelectorAll("#preview .table-enhanced thead th[data-md]")].find(
        (e) => e.textContent.replace("▾", "").trim() === n
      );
      if (!th) return null;
      const box = th.getBoundingClientRect();
      th.dispatchEvent(
        new MouseEvent("mouseover", {
          bubbles: true,
          cancelable: true,
          shiftKey: sh,
          clientX: box.left + 1,
          clientY: box.top + 1,
        })
      );
      return th.getAttribute("data-md");
    },
    name,
    shift
  );
  if (raw === null) throw new Error(`Заголовок ${JSON.stringify(name)} не найден`);
  await browser.pause(30);
  return raw;
}

/// Наводит «мышь» на строку <tr> (tbody), содержащую ячейку с текстом.
/// Целевой элемент — сам <tr>, поэтому модификатор не обязателен.
export async function hoverRowByText(text) {
  const raw = await browser.execute((t) => {
    const preview = document.getElementById("preview");
    preview.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
    const tr = [...document.querySelectorAll("#preview .table-enhanced tbody tr[data-md]")].find(
      (row) => [...row.cells].some((c) => c.textContent.trim() === t)
    );
    if (!tr) return null;
    const box = tr.getBoundingClientRect();
    tr.dispatchEvent(
      new MouseEvent("mouseover", {
        bubbles: true,
        cancelable: true,
        shiftKey: true,
        clientX: box.left + 1,
        clientY: box.top + 1,
      })
    );
    return tr.getAttribute("data-md");
  }, text);
  if (raw === null) throw new Error(`Строка с ячейкой ${JSON.stringify(text)} не найдена`);
  await browser.pause(30);
  return raw;
}

/// Наводит «мышь» на блок таблицы целиком (вне ячеек): target — сам .md-block.
export async function hoverTableBlock() {
  const raw = await browser.execute(() => {
    const preview = document.getElementById("preview");
    preview.dispatchEvent(new MouseEvent("mouseout", { bubbles: true }));
    const block = [...document.querySelectorAll("#preview .md-block[data-md]")].find((b) =>
      b.querySelector("table")
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
  if (raw === null) throw new Error("В предпросмотре нет блока с таблицей");
  await browser.pause(30);
  return raw;
}

/// Кликает по ячейке (mousedown/mouseup/click), как clickBlockByText.
export async function clickCellByText(text) {
  const ok = await browser.execute((t) => {
    const td = [...document.querySelectorAll("#preview .table-enhanced td[data-md]")].find(
      (e) => e.textContent.trim() === t
    );
    if (!td) return false;
    const box = td.getBoundingClientRect();
    const opts = {
      bubbles: true,
      cancelable: true,
      clientX: box.left + 1,
      clientY: box.top + 1,
    };
    td.dispatchEvent(new MouseEvent("mousedown", opts));
    td.dispatchEvent(new MouseEvent("mouseup", opts));
    td.dispatchEvent(new MouseEvent("click", opts));
    return true;
  }, text);
  if (!ok) throw new Error(`Ячейка ${JSON.stringify(text)} не найдена`);
  await browser.pause(30);
}

/// Активный элемент инспектора: тег и видимый текст (или null).
export function activeInspectInfo() {
  return browser.execute(() => {
    const el = document.querySelector("#preview .inspect-active");
    return el ? { tag: el.tagName, text: el.textContent.trim() } : null;
  });
}

/// Число ячеек, подсвеченных бэндом столбца (`.inspect-col`).
export function columnBandCount() {
  return browser.execute(
    () => document.querySelectorAll("#preview .inspect-col").length
  );
}

/// Текст строки, подсвеченной бэндом (`.inspect-row`), или null.
export function rowBandText() {
  return browser.execute(() => {
    const tr = document.querySelector("#preview tr.inspect-row");
    return tr ? tr.textContent.trim() : null;
  });
}

/// Нажимает Shift без движения мыши: keydown на document. Инспектор должен
/// пересчитать уровень последней наведённой цели (AC-5).
export async function pressShift() {
  await browser.execute(() => {
    document.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Shift", bubbles: true })
    );
  });
  await browser.pause(30);
}

/// Отпускает Shift: keyup на document — обратный пересчёт уровня (AC-5).
export async function releaseShift() {
  await browser.execute(() => {
    document.dispatchEvent(
      new KeyboardEvent("keyup", { key: "Shift", bubbles: true })
    );
  });
  await browser.pause(30);
}

/// Эмулирует уход курсора из предпросмотра: mouseout с `relatedTarget` вне
/// `#preview` (инспектор снимает подсветку источника и бэнды, AC-6).
export async function leavePreview() {
  await browser.execute(() => {
    const preview = document.getElementById("preview");
    const editor = document.getElementById("editor");
    const from = document.querySelector("#preview .inspect-active") ?? preview;
    from.dispatchEvent(
      new MouseEvent("mouseout", {
        bubbles: true,
        relatedTarget: editor,
      })
    );
  });
  await browser.pause(30);
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
