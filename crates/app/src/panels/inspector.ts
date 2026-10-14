// inspector.ts вЂ” РґРІСѓСЃС‚РѕСЂРѕРЅРЅСЏСЏ РїРѕРґСЃРІРµС‚РєР° В«Р±Р»РѕРє РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂР° в†” С„СЂР°РіРјРµРЅС‚
// РёСЃС…РѕРґРЅРёРєР° РІ СЂРµРґР°РєС‚РѕСЂРµВ». Р‘Р»РѕРєРё СЂР°Р·РјРµС‡Р°СЋС‚СЃСЏ РЅР° Rust-СЃС‚РѕСЂРѕРЅРµ
// (md-core::to_html_mapped) Р°С‚СЂРёР±СѓС‚РѕРј data-md="start,end", РіРґРµ start/end вЂ”
// Р±Р°Р№С‚РѕРІС‹Рµ СЃРјРµС‰РµРЅРёСЏ РІ РёСЃС…РѕРґРЅРѕРј markdown.
//
// Р“Р»Р°РІРЅР°СЏ СЃР»РѕР¶РЅРѕСЃС‚СЊ вЂ” РµРґРёРЅРёС†С‹ РёР·РјРµСЂРµРЅРёСЏ: textarea РѕРїРµСЂРёСЂСѓРµС‚ UTF-16 code units
// (selectionStart/End), Р° pulldown-cmark вЂ” Р±Р°Р№С‚Р°РјРё UTF-8. РљР°СЂС‚С‹ byteв†”UTF-16 Рё
// СЃРїРёСЃРѕРє Р±Р»РѕРєРѕРІ Р±РµСЂСѓС‚СЃСЏ РёР· РѕР±С‰РµРіРѕ RenderIndex: РёРЅРґРµРєСЃ СЃС‚СЂРѕРёС‚ previewView РїРѕ
// С‚РµРєСЃС‚Сѓ СЂРµРЅРґРµСЂР°, Р·РґРµСЃСЊ РѕРЅ С‚РѕР»СЊРєРѕ С‡РёС‚Р°РµС‚СЃСЏ.

import {
  type Block,
  type UnitMaps,
  bytesToUnits,
  findBlock,
  parseRange,
  unitsToBytes,
} from "../core/mapping";
import type { RenderIndex } from "../doc/renderIndex";

export interface Inspector {
  isActive(): boolean;
  enable(): void;
  disable(): void;


  onIndexChanged(): void;
  /** РЎРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ editorв†’preview; РІС‹Р·С‹РІР°РµС‚СЃСЏ РёР· РѕР±С‰РµРіРѕ С…СѓРєР° main.ts (T-11). */
  onEditorActivity(): void;
}

const INTERACTIVE_SELECTOR = "button, input, select, a, .col-filter-menu, th";

type InspectMode = "block" | "cell" | "row" | "col";

interface InspectTarget {
  el: HTMLElement;
  mode: InspectMode;
}



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
  /** РћР±С‰РёР№ РёРЅРґРµРєСЃ СЂРµРЅРґРµСЂР°: РєР°СЂС‚С‹ byteв†”UTF-16 Рё Р±Р»РѕРєРё. */
  index: RenderIndex;
  /** Р’С‹Р·С‹РІР°РµС‚СЃСЏ РїРµСЂРµРґ РїСЂРѕРіСЂР°РјРјРЅС‹Рј scrollIntoView (С‡С‚РѕР±С‹ СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ СЃРєСЂРѕР»Р»Р° РЅРµ С‚СЏРЅСѓР»Р° РІС‚РѕСЂСѓСЋ РїР°РЅРµР»СЊ). */
  beforeScrollIntoView?: () => void;
}): Inspector {
  const { editor, preview, statusEl, index, beforeScrollIntoView } = opts;

  let active = false;


  let blocks: Block[] | null = null;
  let maps: UnitMaps | null = null;
  let activeBlockEl: HTMLElement | null = null;

  let bandEls: HTMLElement[] = [];

  let lastHover: Element | null = null;
  let lastHoverShift = false;




  let programmaticSel = { start: -1, end: -1 };





  let pinned = false;

  let transient = false;


  let savedSelection = { start: 0, end: 0 };
  let savedScrollTop = 0;
  let savedFocus: HTMLElement | null = null;

  let lastScrollAt = 0;



  function clearBands() {
    for (const el of bandEls) el.classList.remove("inspect-col", "inspect-row");
    bandEls = [];
  }

  function clearBlockHighlight() {
    if (activeBlockEl) {
      activeBlockEl.classList.remove("inspect-active");
      activeBlockEl = null;
    }


    clearBands();
  }

  function scrollIntoViewIfNeeded(el: HTMLElement) {
    const pr = preview.getBoundingClientRect();
    const er = el.getBoundingClientRect();
    if (er.top >= pr.top && er.bottom <= pr.bottom) return; // СѓР¶Рµ РІРёРґРµРЅ
    const now = performance.now();
    if (now - lastScrollAt < 100) return; // throttle, С‡С‚РѕР±С‹ РЅРµ РґС‘СЂРіР°С‚СЊ СЃРєСЂРѕР»Р»
    lastScrollAt = now;

    beforeScrollIntoView?.();
    el.scrollIntoView({ block: "nearest" });
  }

  function applyBlock(el: HTMLElement, focus: boolean, force = false) {
    if (!force && activeBlockEl === el) return; // РіРёСЃС‚РµСЂРµР·РёСЃ вЂ” РЅРµ РґС‘СЂРіР°С‚СЊ РЅР° С‚РѕРј Р¶Рµ Р±Р»РѕРєРµ
    clearBlockHighlight();
    const range = parseRange(el);
    if (!range || !maps) return;
    const start = bytesToUnits(maps, range.start);
    const end = bytesToUnits(maps, range.end);
    el.classList.add("inspect-active");
    activeBlockEl = el;


    if (focus) editor.focus({ preventScroll: true });
    editor.setSelectionRange(start, end);
    programmaticSel = { start, end };
    scrollIntoViewIfNeeded(el);
  }



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



  /** РЎРЅРёРјРѕРє РёРЅРґРµРєСЃР° РґР»СЏ click-to-inspect РІРЅРµ Р°РєС‚РёРІРЅРѕРіРѕ СЂРµР¶РёРјР°. */
  function getIndex(): { blocks: Block[]; maps: UnitMaps } | null {
    const snap = index.current();
    if (!snap) return null;
    return { blocks: snap.blocks, maps: snap.maps };
  }

  function onMouseOver(e: MouseEvent) {
    if (!active || pinned) return; // Р·Р°РєСЂРµРїР»С‘РЅРЅСѓСЋ РїРѕРґСЃРІРµС‚РєСѓ РЅР°РІРµРґРµРЅРёРµ РЅРµ РјРµРЅСЏРµС‚
    const el = e.target as Element | null;
    lastHover = el;
    lastHoverShift = e.shiftKey;
    const target = resolveTarget(el, e.shiftKey);
    if (!target) return;
    applyTarget(target, true);
  }

  function onMouseOut(e: MouseEvent) {
    if (!active) return;
    if (pinned) return; // Р·Р°РєСЂРµРїР»РµРЅРёРµ РїРµСЂРµР¶РёРІР°РµС‚ СѓС…РѕРґ РєСѓСЂСЃРѕСЂР° (BUG-003)
    const rel = e.relatedTarget as Node | null;
    if (rel && preview.contains(rel)) return; // РІСЃС‘ РµС‰С‘ РІРЅСѓС‚СЂРё preview
    lastHover = null;
    clearBlockHighlight(); // СЃРЅРёРјР°РµС‚ Рё РїРѕРґСЃРІРµС‚РєСѓ РёСЃС‚РѕС‡РЅРёРєР°, Рё Р±СЌРЅРґС‹ (AC-6)
    restoreSavedFocus();
  }



  function onDocumentClick(e: MouseEvent) {
    if (!active && !transient) return;
    const targetEl = e.target as Element | null;
    if (preview.contains(targetEl)) return;
    releasePin();
    if (active) restoreSavedFocus();
  }



  function onClickPreview(e: MouseEvent) {
    if (active) return; // РІ СЂРµР¶РёРјРµ РёРЅСЃРїРµРєС‚РѕСЂР° СЂР°Р±РѕС‚Р°РµС‚ onClickCapture
    const targetEl = e.target as Element | null;
    const interactive = !!targetEl?.closest?.(INTERACTIVE_SELECTOR);
    if (interactive) return;

    const target = resolveTarget(targetEl, e.shiftKey);
    if (!target) return;

    const idx = getIndex();
    if (!idx) return;

    clearBlockHighlight();
    const range = parseRange(target.el);
    if (!range) return;
    const start = bytesToUnits(idx.maps, range.start);
    const end = bytesToUnits(idx.maps, range.end);
    target.el.classList.add("inspect-active");
    activeBlockEl = target.el;
    transient = true;

    editor.focus();
    editor.setSelectionRange(start, end);
    programmaticSel = { start, end };
  }



  function onShiftKey(e: KeyboardEvent) {
    if (!active || pinned || e.key !== "Shift") return;
    if (!lastHover || !lastHover.isConnected) return;
    const shift = e.type !== "keyup";
    if (shift === lastHoverShift) return; // СЂРµР°Р»СЊРЅРѕРіРѕ РїРµСЂРµРєР»СЋС‡РµРЅРёСЏ РЅРµ Р±С‹Р»Рѕ
    lastHoverShift = shift;
    const target = resolveTarget(lastHover, shift);
    if (!target) return;
    applyTarget(target, true, true);
  }

  function onEditorSelection() {
    if (!active || pinned || !blocks || !maps) return;
    if (document.activeElement !== editor) return;


    if (
      (editor.selectionStart ?? 0) === programmaticSel.start &&
      (editor.selectionEnd ?? 0) === programmaticSel.end
    ) {
      return;
    }



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

    clearBlockHighlight();
    block.el.classList.add("inspect-active");
    activeBlockEl = block.el;
    scrollIntoViewIfNeeded(block.el);
  }

  function releasePin() {
    pinned = false;
    transient = false;
    lastHover = null;
    lastHoverShift = false;
    clearBlockHighlight();
  }

  function onClickCapture(e: MouseEvent) {
    if (!active) return;
    const targetEl = e.target as Element | null;
    const interactive = !!targetEl?.closest?.(INTERACTIVE_SELECTOR);
    if (interactive) {


      if (pinned) releasePin();
      return;
    }
    const target = resolveTarget(targetEl, e.shiftKey);
    if (pinned) {


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
    if (!blocks || !maps) return; // РёРЅРґРµРєСЃ СѓСЃС‚Р°СЂРµР» (РїСЂР°РІРєР° РґРѕ СЂРµРЅРґРµСЂР°) вЂ” Р·Р°РєСЂРµРїР»СЏС‚СЊ РЅРµС‡РµРіРѕ
    e.preventDefault();
    e.stopPropagation();


    applyTarget(target, true, true);
    pinned = true;


    lastHover = null;
    lastHoverShift = false;
  }



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


    statusEl.textContent = "Р РµР¶РёРј РёРЅСЃРїРµРєС‚РѕСЂР°";
    preview.addEventListener("mouseover", onMouseOver);
    preview.addEventListener("mouseout", onMouseOut);
    preview.addEventListener("click", onClickCapture, true);
    preview.addEventListener("click", onClickPreview);
    document.addEventListener("click", onDocumentClick);
    document.addEventListener("selectionchange", onEditorSelection);
    document.addEventListener("keydown", onShiftKey);
    document.addEventListener("keyup", onShiftKey);

    onIndexChanged();
  }

  function disable(): void {
    if (!active) return;
    active = false;
    pinned = false;
    transient = false;
    clearBlockHighlight();
    blocks = null;
    maps = null;
    lastHover = null;
    lastHoverShift = false;
    programmaticSel = { start: -1, end: -1 };
    statusEl.hidden = true;
    statusEl.textContent = ""; // СЃР±СЂРѕСЃ, С‡С‚РѕР±С‹ СЃР»РµРґСѓСЋС‰РµРµ РІРєР»СЋС‡РµРЅРёРµ СЃРЅРѕРІР° РѕР±СЉСЏРІРёР»РѕСЃСЊ (M6)
    preview.removeEventListener("mouseover", onMouseOver);
    preview.removeEventListener("mouseout", onMouseOut);
    preview.removeEventListener("click", onClickCapture, true);
    preview.removeEventListener("click", onClickPreview);
    document.removeEventListener("click", onDocumentClick);
    document.removeEventListener("selectionchange", onEditorSelection);
    document.removeEventListener("keydown", onShiftKey);
    document.removeEventListener("keyup", onShiftKey);

    editor.setSelectionRange(savedSelection.start, savedSelection.end);
    editor.scrollTop = savedScrollTop;
    restoreSavedFocus();
  }

  function onIndexChanged(): void {

    pinned = false;
    transient = false;
    clearBlockHighlight();
    blocks = null;
    maps = null;
    programmaticSel = { start: -1, end: -1 };
    if (!active) return;
    const snap = index.current();
    if (!snap) return;
    blocks = snap.blocks;
    maps = snap.maps;
  }

  return { isActive, enable, disable, onIndexChanged, onEditorActivity: onEditorSelection };
}
