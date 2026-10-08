// inspector.ts — режим инспектора: двусторонняя подсветка «блок предпросмотра
// ↔ фрагмент исходника в редакторе». Блоки размечаются на Rust-стороне
// (md-core::to_html_mapped) атрибутом data-md="start,end", где start/end —
// байтовые смещения в исходном markdown.
//
// Главная сложность — единицы измерения: textarea оперирует UTF-16 code units
// (selectionStart/End), а pulldown-cmark — байтами UTF-8. Карты byte↔UTF-16 и
// список блоков берутся из общего RenderIndex (Фаза 4): индекс строит
// previewView по тексту рендера, здесь он только читается. Синхронизация
// скролла использует тот же снимок, исключая дубль построения.

import {
  type Block,
  type UnitMaps,
  bytesToUnits,
  findBlock,
  parseRange,
  unitsToBytes,
} from "./mapping";
import type { RenderIndex } from "./renderIndex";

export interface Inspector {
  isActive(): boolean;
  enable(): void;
  disable(): void;
  /**
   * Индекс изменился (перестроен после рендера или сброшен при правке).
   * Данные берутся из RenderIndex: там уже лежит `source` рендера и посчитанные
   * по нему карты/блоки. Просто пересчитываем привязки либо сбрасываем их.
   */
  onIndexChanged(): void;
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
  /** Общий индекс рендера: карты byte↔UTF-16 и блоки (Фаза 4). */
  index: RenderIndex;
  /** Вызывается перед программным scrollIntoView (чтобы синхронизация скролла не тянула вторую панель). */
  beforeScrollIntoView?: () => void;
}): Inspector {
  const { editor, preview, statusEl, index, beforeScrollIntoView } = opts;

  let active = false;
  // Привязки читаются из снимка индекса; здесь — только текущие ссылки на
  // активные данные (валидны ровно пока индекс не перестроен/не сброшен).
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

  // BUG-003: закрепление выделения. Клик по блоку фиксирует подсветку и
  // выделение редактора — они переживают уход курсора (`mouseout`), наведение и
  // движение каретки. Повторный клик снимает закрепление. Сбрасываем закрепление
  // при перестройке индекса (правка/перерендер) и выходе из режима: байтовые
  // смещения блока после правки недействительны.
  let pinned = false;

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
    if (!active || pinned) return; // закреплённую подсветку наведение не меняет
    const el = e.target as Element | null;
    lastHover = el;
    lastHoverShift = e.shiftKey;
    const target = resolveTarget(el, e.shiftKey);
    if (!target) return;
    applyTarget(target, true);
  }

  function onMouseOut(e: MouseEvent) {
    if (!active) return;
    if (pinned) return; // закрепление переживает уход курсора (BUG-003)
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
    if (!active || pinned || e.key !== "Shift") return;
    if (!lastHover || !lastHover.isConnected) return;
    const shift = e.type !== "keyup";
    if (shift === lastHoverShift) return; // реального переключения не было
    lastHoverShift = shift;
    const target = resolveTarget(lastHover, shift);
    if (!target) return;
    applyTarget(target, true, true);
  }

  function onEditorSelection() {
    if (!active || pinned || !blocks || !maps) return;
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

  // Снимает закрепление и гасит подсветку/бэнды. Общее для повторного клика по
  // закреплённому блоку, клика по пустой области и клика по интерактивному
  // элементу (BUG-003).
  function releasePin() {
    pinned = false;
    lastHover = null;
    lastHoverShift = false;
    clearBlockHighlight();
  }

  function onClickCapture(e: MouseEvent) {
    if (!active) return;
    const targetEl = e.target as Element | null;
    const interactive = !!targetEl?.closest?.(INTERACTIVE_SELECTOR);
    if (interactive) {
      // Клик по кнопке/ссылке/`th`/меню снимает закрепление, но событие отдаём
      // дальше (сортировка, переход) — BUG-003.
      if (pinned) releasePin();
      return;
    }
    const target = resolveTarget(targetEl, e.shiftKey);
    if (pinned) {
      // Закреплено: клик по ДРУГОМУ блоку переносит закрепление; повторный клик
      // по тому же блоку или клик по пустой области снимает его (BUG-003).
      e.preventDefault();
      e.stopPropagation();
      if (!target || target.el === activeBlockEl) {
        releasePin();
        restoreSavedFocus();
        return;
      }
      applyTarget(target, true, true);
      lastHover = null;
      lastHoverShift = false;
      return;
    }
    if (!target) return;
    if (!blocks || !maps) return; // индекс устарел (правка до рендера) — закреплять нечего
    e.preventDefault();
    e.stopPropagation();
    // force: клик должен вернуть выделение и фокус в редактор, даже если блок
    // уже подсвечен (иначе focus уходит в предпросмотр, а выделение «гаснет»).
    applyTarget(target, true, true);
    pinned = true;
    // Курсор мог «устареть» за время закрепления — пересчёт по Shift после
    // снятия не должен опираться на блок, который мы запинили.
    lastHover = null;
    lastHoverShift = false;
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
    // Привязки могли появиться, пока режим был выключен, — подхватываем индекс.
    onIndexChanged();
  }

  function disable(): void {
    if (!active) return;
    active = false;
    pinned = false;
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

  function onIndexChanged(): void {
    pinned = false; // индекс перестроен: закреплённые смещения устарели
    clearBlockHighlight();
    blocks = null;
    maps = null;
    programmaticSel = { start: -1, end: -1 };
    // Индекс пуст (текст изменён до рендера) или режим выключен — только сброс.
    if (!active) return;
    const snap = index.current();
    if (!snap) return;
    blocks = snap.blocks;
    maps = snap.maps;
  }

  return { isActive, enable, disable, onIndexChanged, onEditorActivity: onEditorSelection };
}
