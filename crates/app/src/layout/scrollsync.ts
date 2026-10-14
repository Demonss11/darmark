// scrollsync.ts — синхронизация вертикальной прокрутки редактора и предпросмотра.
//
// Анкорная привязка через `data-md` работает, когда в DOM есть размеченные блоки
// (mapped-рендер, т.е. включён инспектор): редактор → блок, содержащий верхнюю
// видимую логическую строку; предпросмотр → строка начала верхнего видимого
// блока. Иначе (и между вводом и перерендером) — пропорциональный fallback.
//
// Карты byte↔UTF-16, начала строк и блоки берутся из общего RenderIndex, здесь
// только читается снимок — тот же, что видит инспектор (без дубля построения).
//
// Логические строки тождественны визуальным: мягкий перенос у редактора отключён
// (`wrap="off"` + `white-space: pre` в style.css), поэтому `scrollTop/lineH`
// точно указывает на логическую строку, а `lineStarts` (по `\n`) и `lineH`
// согласованы. При включённом переносе формула теряет смысл.
//
// Защита от обратной связи двухуровневая: флаг `syncing` поднимается ДО записи
// scrollTop и снимается в rAF (гасит синхронное эхо), окно `ECHO_MS` после
// записи гасит инерционное эхо WebView2, приходящее уже после снятия флага.
// Игнорируются события только той панели, в которую писала синхронизация
// (`lastWriteTarget`), иначе быстрый скролл источника сам себя глушил бы.

import {
  type Block,
  type UnitMaps,
  bytesToUnits,
  findBlockContaining,
  findLineAt,
  nextBlockAfter,
  unitsToBytes,
} from "../core/mapping";
import type { RenderIndex } from "../doc/renderIndex";

export interface ScrollSync {
  isEnabled(): boolean;
  setEnabled(enabled: boolean): void;
  onIndexChanged(): void;
  suspend(): void;
}

const ECHO_MS = 100;

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

export function createScrollSync(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  index: RenderIndex;
}): ScrollSync {
  const { editor, preview, index } = opts;

  let enabled = true;
  let syncing = false;
  let rafId = 0;
  let pending: (() => void) | null = null;
  let clearRaf = 0;
  let lastWriteAt = 0;
  let lastWriteTarget: HTMLElement | null = null;

  let blocks: Block[] | null = null;
  let maps: UnitMaps | null = null;
  let lineStarts: Int32Array | null = null;
  let lineH = 0;
  let previewPadTop = 0;

  function hasAnchors(): boolean {
    return !!blocks && blocks.length > 0 && !!maps && !!lineStarts && lineH > 0;
  }

  function maxScroll(el: HTMLElement): number {
    return el.scrollHeight - el.clientHeight;
  }

  function isEcho(target: EventTarget | null): boolean {
    if (target !== lastWriteTarget) return false;
    return syncing || performance.now() - lastWriteAt < ECHO_MS;
  }

  function writeScrollTop(to: HTMLElement, target: number) {
    if (to.clientHeight === 0) return;
    const hi = maxScroll(to);
    if (hi <= 0) return;
    const next = clamp(target, 0, hi);
    if (Math.abs(to.scrollTop - next) < 1) return;
    syncing = true;
    lastWriteAt = performance.now();
    lastWriteTarget = to;
    to.scrollTop = next;
    if (clearRaf) cancelAnimationFrame(clearRaf);
    clearRaf = requestAnimationFrame(() => {
      syncing = false;
      clearRaf = 0;
    });
  }

  function schedule(fn: () => void) {
    pending = fn;
    if (rafId) return;
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      const run = pending;
      pending = null;
      if (run) run();
    });
  }

  function syncPair(from: HTMLElement, to: HTMLElement) {
    const fromMax = maxScroll(from);
    const toMax = maxScroll(to);
    if (fromMax <= 0 || toMax <= 0) return;
    schedule(() => {
      if (!enabled) return;
      writeScrollTop(to, (from.scrollTop / fromMax) * toMax);
    });
  }


  function alignEditorToPreview() {
    if (!hasAnchors()) {
      syncPair(editor, preview);
      return;
    }
    const ls = lineStarts!;
    const mp = maps!;
    const bl = blocks!;
    const topLine = clamp(Math.floor(editor.scrollTop / lineH), 0, ls.length - 1);
    if (topLine === 0) {
      schedule(() => {
        if (enabled) writeScrollTop(preview, 0);
      });
      return;
    }
    const bytePos = unitsToBytes(mp, ls[topLine]!);
    const block = findBlockContaining(bl, bytePos) ?? nextBlockAfter(bl, bytePos);
    if (!block) {
      syncPair(editor, preview);
      return;
    }
    schedule(() => {
      if (!enabled) return;
      const pr = preview.getBoundingClientRect();
      const br = block.el.getBoundingClientRect();
      const delta = br.top - pr.top;
      writeScrollTop(preview, preview.scrollTop + delta - previewPadTop);
    });
  }


  function alignPreviewToEditor() {
    if (!hasAnchors()) {
      syncPair(preview, editor);
      return;
    }
    const bl = blocks!;
    const prTop = preview.getBoundingClientRect().top;
    let block: Block | null = null;
    for (const b of bl) {
      if (b.el.getBoundingClientRect().bottom >= prTop) {
        block = b;
        break;
      }
    }
    if (!block) block = bl[bl.length - 1] ?? null;
    if (!block) {
      syncPair(preview, editor);
      return;
    }
    const mp = maps!;
    const bytePos = block.start;
    const line = findLineAt(lineStarts!, bytesToUnits(mp, bytePos));
    schedule(() => {
      if (!enabled) return;
      writeScrollTop(editor, line * lineH);
    });
  }

  function measure() {
    const cs = getComputedStyle(editor);
    const lh = parseFloat(cs.lineHeight);
    const fs = parseFloat(cs.fontSize);
    lineH = Number.isFinite(lh) && lh > 0 ? lh : Number.isFinite(fs) ? fs * 1.2 : 21;
    previewPadTop = parseFloat(getComputedStyle(preview).paddingTop) || 0;
  }

  const onEditorScroll = (e: Event) => {
    if (!enabled || !e.target || e.target !== editor) return;
    if (isEcho(editor)) return;
    alignEditorToPreview();
  };
  const onPreviewScroll = (e: Event) => {
    if (!enabled || !e.target || e.target !== preview) return;
    if (isEcho(preview)) return;
    alignPreviewToEditor();
  };

  editor.addEventListener("scroll", onEditorScroll, { passive: true });
  preview.addEventListener("scroll", onPreviewScroll, { passive: true });

  function cancelPending() {
    if (rafId) {
      cancelAnimationFrame(rafId);
      rafId = 0;
    }
    pending = null;
    if (clearRaf) {
      cancelAnimationFrame(clearRaf);
      clearRaf = 0;
    }
    syncing = false;
  }

  return {
    isEnabled: () => enabled,
    setEnabled(value: boolean) {
      if (value === enabled) {
        if (value) alignEditorToPreview();
        return;
      }
      enabled = value;
      if (!value) {
        cancelPending();
        return;
      }
      alignEditorToPreview();
    },
    onIndexChanged() {
      const snap = index.current();
      if (!snap) {
        blocks = null;
        maps = null;
        lineStarts = null;
        if (enabled) syncPair(editor, preview);
        return;
      }
      if (!enabled) return;
      measure();
      maps = snap.maps;
      lineStarts = snap.lineStarts;
      blocks = snap.blocks;
      alignEditorToPreview();
    },
    suspend() {
      syncing = true;
      lastWriteAt = performance.now();
      lastWriteTarget = preview;
      if (clearRaf) cancelAnimationFrame(clearRaf);
      clearRaf = requestAnimationFrame(() => {
        syncing = false;
        clearRaf = 0;
      });
    },
  };
}
