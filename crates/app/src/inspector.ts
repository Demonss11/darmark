// inspector.ts — режим инспектора: двусторонняя подсветка «блок предпросмотра
// ↔ фрагмент исходника в редакторе». Блоки размечаются на Rust-стороне
// (md-core::to_html_mapped) атрибутом data-md="start,end", где start/end —
// байтовые смещения в исходном markdown.
//
// Главная сложность — единицы измерения: textarea оперирует UTF-16 code units
// (selectionStart/End), а pulldown-cmark — байтами UTF-8. Карты byte↔UTF-16
// строятся по editor.value, который WebView может нормализовать по CRLF — именно
// он и есть источник истины. Общая логика вынесена в `mapping.ts`, чтобы её
// переиспользовала синхронизация скролла.

import {
  type Block,
  type UnitMaps,
  buildUnitMaps,
  bytesToUnits,
  collectBlocks,
  findBlock,
  parseRange,
  unitsToBytes,
} from "./mapping";

export interface Inspector {
  isActive(): boolean;
  enable(): void;
  disable(): void;
  /**
   * Вызывается после установки `preview.innerHTML`.
   * `markdown` — текст, который ушёл в рендер: именно по нему посчитаны
   * байтовые смещения в `data-md`, поэтому карту byte↔UTF-16 нужно строить
   * по нему, а не по текущему `editor.value` (который мог измениться, пока
   * асинхронный рендер был в полёте). Без аргумента — просто сброс подсветки.
   */
  onRendered(markdown?: string): void;
  /** Синхронизация editor→preview; вызывается из общего хука main.ts (T-11). */
  onEditorActivity(): void;
}

const INTERACTIVE_SELECTOR = "button, input, select, a, .col-filter-menu, th";

type InspectMode = "block" | "cell" | "row" | "col";

interface InspectTarget {
  el: HTMLElement;
  mode: InspectMode;
}

/**
 * Определяет цель подсветки по элементу под курсором и состоянию Shift.
 * Приоритет: `<th>` (столбец) > `Shift+td` (строка) > `td` (ячейка) >
 * строка-промежуток (строка) > `.md-block` (блок). Приоритет `<th>` выше
 * модификатора — столбец не переопределяется Shift (AC-3).
 */
function resolveTarget(target: Element | null, shiftKey: boolean): InspectTarget | null {
  if (!target || typeof target.closest !== "function") return null;
  const th = target.closest("th[data-md]") as HTMLElement | null;
  if (th) return { el: th, mode: "col" };
  const tr = target.closest("tr[data-md]") as HTMLElement | null;
  const td = target.closest("td[data-md]") as HTMLElement | null;
  if (td && !shiftKey) return { el: td, mode: "cell" };
  if (tr) return { el: tr, mode: "row" };
  if (td) return { el: td, mode: "cell" };
  const block = target.closest(".md-block[data-md]") as HTMLElement | null;
  if (block) return { el: block, mode: "block" };
  return null;
}

export function createInspector(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  statusEl: HTMLElement;
  /** Вызывается перед программным scrollIntoView (чтобы синхронизация скролла не тянула вторую панель). */
  beforeScrollIntoView?: () => void;
}): Inspector {
  const { editor, preview, statusEl, beforeScrollIntoView } = opts;

  let active = false;
  let blocks: Block[] | null = null;
  let maps: UnitMaps | null = null;
  let activeBlockEl: HTMLElement | null = null;
  // Бэнды строки/столбца (транзиентные классы), список — для идемпотентной очистки.
  let bandEls: HTMLElement[] = [];
  // Последняя наведённая цель (для пересчёта уровня при Shift без движения мыши).
  let lastHover: Element | null = null;
  let lastHoverShift = false;
  // Выделение, выставленное инспектором программно. `selectionchange` приходит
  // асинхронно, поэтому по совпадению значений отличаем своё выделение от
  // пользовательского: иначе обработчик каретки перебил бы подсветку ячейки
  // блочной (он ищет `.md-block`, а не `tr/th/td`).
  let programmaticSel = { start: -1, end: -1 };

  // Состояние textarea и фокуса до включения режима (восстанавливаем на выходе).
  let savedSelection = { start: 0, end: 0 };
  let savedScrollTop = 0;
  let savedFocus: HTMLElement | null = null;

  let lastScrollAt = 0;

  // ---------- подсветка ----------

  function clearBands() {
    for (const el of bandEls) el.classList.remove("inspect-col", "inspect-row");
    bandEls = [];
  }

  function clearBlockHighlight() {
    if (activeBlockEl) {
      activeBlockEl.classList.remove("inspect-active");
      activeBlockEl = null;
    }
    // Бэнды — часть того же цикла очистки (F-6): disable/onRendered/mouseout
    // обязаны снимать и подсветку, и подсветку строки/столбца.
    clearBands();
  }

  function scrollIntoViewIfNeeded(el: HTMLElement) {
    const pr = preview.getBoundingClientRect();
    const er = el.getBoundingClientRect();
    if (er.top >= pr.top && er.bottom <= pr.bottom) return; // уже виден
    const now = performance.now();
    if (now - lastScrollAt < 100) return; // throttle, чтобы не дёргать скролл
    lastScrollAt = now;
    // Гасим синхронизацию: программный scrollIntoView не должен тянуть редактор.
    beforeScrollIntoView?.();
    el.scrollIntoView({ block: "nearest" });
  }

  function applyBlock(el: HTMLElement, focus: boolean, force = false) {
    if (!force && activeBlockEl === el) return; // гистерезис — не дёргать на том же блоке
    clearBlockHighlight();
    const range = parseRange(el);
    if (!range || !maps) return;
    const start = bytesToUnits(maps, range.start);
    const end = bytesToUnits(maps, range.end);
    el.classList.add("inspect-active");
    activeBlockEl = el;
    // Сначала фокус, потом диапазон: focus() может восстановить сохранённое
    // выделение textarea и затереть только что выставленный диапазон.
    if (focus) editor.focus({ preventScroll: true });
    editor.setSelectionRange(start, end);
    programmaticSel = { start, end };
    scrollIntoViewIfNeeded(el);
  }

  /**
   * Подсвечивает столбец целиком: `.inspect-col` на всех ячейках с тем же
   * `cellIndex`. Индекс ячейки стабилен после переупорядочивания строк
   * `tables.ts` (AC-8), поэтому столбец остаётся корректным.
   */
  function bandColumn(th: HTMLTableCellElement) {
    const table = th.closest("table") as HTMLTableElement | null;
    if (!table) return;
    const col = th.cellIndex;
    if (col < 0) return;
    for (const row of Array.from(table.rows)) {
      const cell = row.cells[col];
      if (cell) {
        cell.classList.add("inspect-col");
        bandEls.push(cell);
      }
    }
  }

  /**
   * Применяет цель: подсветка источника (через `applyBlock`) + бэнды строки/
   * столбца. Бэнды всегда пересобираются с нуля — иначе повторный hover по той
   * же ячейке (когда `applyBlock` сработал по гистерезису) копил бы список.
   */
  function applyTarget(target: InspectTarget, focus: boolean, force = false) {
    clearBands();
    applyBlock(target.el, focus, force);
    if (target.mode === "row") {
      target.el.classList.add("inspect-row");
      bandEls.push(target.el);
    } else if (target.mode === "col") {
      bandColumn(target.el as HTMLTableCellElement);
    }
  }

  function restoreSavedFocus() {
    if (savedFocus && savedFocus.isConnected && typeof savedFocus.focus === "function") {
      savedFocus.focus({ preventScroll: true });
    }
  }

  // ---------- слушатели ----------

  function onMouseOver(e: MouseEvent) {
    if (!active) return;
    const el = e.target as Element | null;
    lastHover = el;
    lastHoverShift = e.shiftKey;
    const target = resolveTarget(el, e.shiftKey);
    if (!target) return;
    applyTarget(target, true);
  }

  function onMouseOut(e: MouseEvent) {
    if (!active) return;
    const rel = e.relatedTarget as Node | null;
    if (rel && preview.contains(rel)) return; // всё ещё внутри preview
    lastHover = null;
    clearBlockHighlight(); // снимает и подсветку источника, и бэнды (AC-6)
    restoreSavedFocus();
  }

  /**
   * AC-5: Shift нажат/отпущен без движения мыши — пересчитываем уровень для
   * последней наведённой цели. `e.shiftKey` у модификаторных keydown/keyup
   * нестабилен, поэтому состояние выводим из типа события.
   */
  function onShiftKey(e: KeyboardEvent) {
    if (!active || e.key !== "Shift") return;
    if (!lastHover || !lastHover.isConnected) return;
    const shift = e.type !== "keyup";
    if (shift === lastHoverShift) return; // реального переключения не было
    lastHoverShift = shift;
    const target = resolveTarget(lastHover, shift);
    if (!target) return;
    applyTarget(target, true, true);
  }

  function onEditorSelection() {
    if (!active || !blocks || !maps) return;
    if (document.activeElement !== editor) return;
    // Асинхронный `selectionchange` от программного выделения инспектора не
    // должен перебивать гранулярную подсветку ячейки/строки/столбца.
    if (
      (editor.selectionStart ?? 0) === programmaticSel.start &&
      (editor.selectionEnd ?? 0) === programmaticSel.end
    ) {
      return;
    }
    // «Каретка» = активный край выделения. При протяжке мышью/Shift+стрелках
    // selectionStart фиксирован (якорь), а двигается противоположный край,
    // поэтому ориентируемся на selectionDirection (AC-5).
    const backward = editor.selectionDirection === "backward";
    const caretPos = backward
      ? editor.selectionStart ?? 0
      : editor.selectionEnd ?? 0;
    const block = findBlock(blocks, unitsToBytes(maps, caretPos));
    if (!block) {
      clearBlockHighlight();
      return;
    }
    if (activeBlockEl === block.el) return;
    // Только подсветка в предпросмотре; выделение в textarea не зеркалим (T-11).
    clearBlockHighlight();
    block.el.classList.add("inspect-active");
    activeBlockEl = block.el;
    scrollIntoViewIfNeeded(block.el);
  }

  function onClickCapture(e: MouseEvent) {
    if (!active) return;
    const targetEl = e.target as Element | null;
    if (targetEl?.closest?.(INTERACTIVE_SELECTOR)) return; // дать tables.ts/ссылкам работать
    const target = resolveTarget(targetEl, e.shiftKey);
    if (!target) return;
    e.preventDefault();
    e.stopPropagation();
    // force: клик должен вернуть выделение и фокус в редактор, даже если блок
    // уже подсвечен (иначе focus уходит в предпросмотр, а выделение «гаснет»).
    applyTarget(target, true, true);
  }

  // ---------- API ----------

  function isActive(): boolean {
    return active;
  }

  function enable(): void {
    if (active) return;
    active = true;
    savedSelection = { start: editor.selectionStart ?? 0, end: editor.selectionEnd ?? 0 };
    savedScrollTop = editor.scrollTop;
    savedFocus = document.activeElement as HTMLElement | null;
    statusEl.hidden = false;
    preview.addEventListener("mouseover", onMouseOver);
    preview.addEventListener("mouseout", onMouseOut);
    preview.addEventListener("click", onClickCapture, true);
    document.addEventListener("selectionchange", onEditorSelection);
    document.addEventListener("keydown", onShiftKey);
    document.addEventListener("keyup", onShiftKey);
  }

  function disable(): void {
    if (!active) return;
    active = false;
    clearBlockHighlight();
    blocks = null;
    maps = null;
    lastHover = null;
    lastHoverShift = false;
    programmaticSel = { start: -1, end: -1 };
    statusEl.hidden = true;
    preview.removeEventListener("mouseover", onMouseOver);
    preview.removeEventListener("mouseout", onMouseOut);
    preview.removeEventListener("click", onClickCapture, true);
    document.removeEventListener("selectionchange", onEditorSelection);
    document.removeEventListener("keydown", onShiftKey);
    document.removeEventListener("keyup", onShiftKey);
    // OQ-3: восстанавливаем состояние редактора и фокус.
    editor.setSelectionRange(savedSelection.start, savedSelection.end);
    editor.scrollTop = savedScrollTop;
    restoreSavedFocus();
  }

  function onRendered(markdown?: string): void {
    clearBlockHighlight();
    blocks = null;
    maps = null;
    programmaticSel = { start: -1, end: -1 };
    // Без аргумента — только сброс подсветки (вызывается из input-хендлера:
    // DOM ещё старый, переиндексация будет после установки нового innerHTML).
    if (!active || markdown === undefined) return;
    blocks = collectBlocks(preview);
    // Смещения в data-md посчитаны по тексту рендера; строим карту по нему же.
    maps = buildUnitMaps(markdown);
  }

  return { isActive, enable, disable, onRendered, onEditorActivity: onEditorSelection };
}