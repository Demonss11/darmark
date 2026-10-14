// scrollsync.ts вЂ” СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ РІРµСЂС‚РёРєР°Р»СЊРЅРѕР№ РїСЂРѕРєСЂСѓС‚РєРё СЂРµРґР°РєС‚РѕСЂР° Рё РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂР°.
//
// РђРЅРєРѕСЂРЅР°СЏ РїСЂРёРІСЏР·РєР° С‡РµСЂРµР· `data-md` СЂР°Р±РѕС‚Р°РµС‚, РєРѕРіРґР° РІ DOM РµСЃС‚СЊ СЂР°Р·РјРµС‡РµРЅРЅС‹Рµ Р±Р»РѕРєРё
// (mapped-СЂРµРЅРґРµСЂ, С‚.Рµ. РІРєР»СЋС‡С‘РЅ РёРЅСЃРїРµРєС‚РѕСЂ): СЂРµРґР°РєС‚РѕСЂ в†’ Р±Р»РѕРє, СЃРѕРґРµСЂР¶Р°С‰РёР№ РІРµСЂС…РЅСЋСЋ
// РІРёРґРёРјСѓСЋ Р»РѕРіРёС‡РµСЃРєСѓСЋ СЃС‚СЂРѕРєСѓ; РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂ в†’ СЃС‚СЂРѕРєР° РЅР°С‡Р°Р»Р° РІРµСЂС…РЅРµРіРѕ РІРёРґРёРјРѕРіРѕ
// Р±Р»РѕРєР°. РРЅР°С‡Рµ (Рё РјРµР¶РґСѓ РІРІРѕРґРѕРј Рё РїРµСЂРµСЂРµРЅРґРµСЂРѕРј) вЂ” РїСЂРѕРїРѕСЂС†РёРѕРЅР°Р»СЊРЅС‹Р№ fallback.
//
// РљР°СЂС‚С‹ byteв†”UTF-16, РЅР°С‡Р°Р»Р° СЃС‚СЂРѕРє Рё Р±Р»РѕРєРё Р±РµСЂСѓС‚СЃСЏ РёР· РѕР±С‰РµРіРѕ RenderIndex, Р·РґРµСЃСЊ
// С‚РѕР»СЊРєРѕ С‡РёС‚Р°РµС‚СЃСЏ СЃРЅРёРјРѕРє вЂ” С‚РѕС‚ Р¶Рµ, С‡С‚Рѕ РІРёРґРёС‚ РёРЅСЃРїРµРєС‚РѕСЂ (Р±РµР· РґСѓР±Р»СЏ РїРѕСЃС‚СЂРѕРµРЅРёСЏ).
//
// Р›РѕРіРёС‡РµСЃРєРёРµ СЃС‚СЂРѕРєРё С‚РѕР¶РґРµСЃС‚РІРµРЅРЅС‹ РІРёР·СѓР°Р»СЊРЅС‹Рј: РјСЏРіРєРёР№ РїРµСЂРµРЅРѕСЃ Сѓ СЂРµРґР°РєС‚РѕСЂР° РѕС‚РєР»СЋС‡С‘РЅ
// (`wrap="off"` + `white-space: pre` РІ style.css), РїРѕСЌС‚РѕРјСѓ `scrollTop/lineH`
// С‚РѕС‡РЅРѕ СѓРєР°Р·С‹РІР°РµС‚ РЅР° Р»РѕРіРёС‡РµСЃРєСѓСЋ СЃС‚СЂРѕРєСѓ, Р° `lineStarts` (РїРѕ `\n`) Рё `lineH`
// СЃРѕРіР»Р°СЃРѕРІР°РЅС‹. РџСЂРё РІРєР»СЋС‡С‘РЅРЅРѕРј РїРµСЂРµРЅРѕСЃРµ С„РѕСЂРјСѓР»Р° С‚РµСЂСЏРµС‚ СЃРјС‹СЃР».
//
// Р—Р°С‰РёС‚Р° РѕС‚ РѕР±СЂР°С‚РЅРѕР№ СЃРІСЏР·Рё РґРІСѓС…СѓСЂРѕРІРЅРµРІР°СЏ: С„Р»Р°Рі `syncing` РїРѕРґРЅРёРјР°РµС‚СЃСЏ Р”Рћ Р·Р°РїРёСЃРё
// scrollTop Рё СЃРЅРёРјР°РµС‚СЃСЏ РІ rAF (РіР°СЃРёС‚ СЃРёРЅС…СЂРѕРЅРЅРѕРµ СЌС…Рѕ), РѕРєРЅРѕ `ECHO_MS` РїРѕСЃР»Рµ
// Р·Р°РїРёСЃРё РіР°СЃРёС‚ РёРЅРµСЂС†РёРѕРЅРЅРѕРµ СЌС…Рѕ WebView2, РїСЂРёС…РѕРґСЏС‰РµРµ СѓР¶Рµ РїРѕСЃР»Рµ СЃРЅСЏС‚РёСЏ С„Р»Р°РіР°.
// РРіРЅРѕСЂРёСЂСѓСЋС‚СЃСЏ СЃРѕР±С‹С‚РёСЏ С‚РѕР»СЊРєРѕ С‚РѕР№ РїР°РЅРµР»Рё, РІ РєРѕС‚РѕСЂСѓСЋ РїРёСЃР°Р»Р° СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ
// (`lastWriteTarget`), РёРЅР°С‡Рµ Р±С‹СЃС‚СЂС‹Р№ СЃРєСЂРѕР»Р» РёСЃС‚РѕС‡РЅРёРєР° СЃР°Рј СЃРµР±СЏ РіР»СѓС€РёР» Р±С‹.

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
  /** Р’РєР»СЋС‡РµРЅРёРµ/РІС‹РєР»СЋС‡РµРЅРёРµ; РїСЂРё РІРєР»СЋС‡РµРЅРёРё РїР°РЅРµР»Рё СЃСЂР°Р·Сѓ РІС‹СЂР°РІРЅРёРІР°СЋС‚СЃСЏ. */
  setEnabled(enabled: boolean): void;


  onIndexChanged(): void;


  suspend(): void;
}

/** РћРєРЅРѕ РёРіРЅРѕСЂРёСЂРѕРІР°РЅРёСЏ СЌС…Р° WebView2 РїРѕСЃР»Рµ РїСЂРѕРіСЂР°РјРјРЅРѕР№ Р·Р°РїРёСЃРё scrollTop, РјСЃ. */
const ECHO_MS = 100;

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

export function createScrollSync(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  /** РћР±С‰РёР№ РёРЅРґРµРєСЃ СЂРµРЅРґРµСЂР°: РєР°СЂС‚С‹, СЃС‚СЂРѕРєРё, Р±Р»РѕРєРё (Р¤Р°Р·Р° 4). */
  index: RenderIndex;
}): ScrollSync {
  const { editor, preview, index } = opts;

  let enabled = true;
  let syncing = false;          // РїРѕРґРЅСЏС‚ РЅР° РІСЂРµРјСЏ РїСЂРѕРіСЂР°РјРјРЅРѕР№ Р·Р°РїРёСЃРё scrollTop
  let rafId = 0;                // РѕС‚Р»РѕР¶РµРЅРЅР°СЏ Р·Р°РїРёСЃСЊ (throttle РЅР° РєР°РґСЂ)
  let pending: (() => void) | null = null; // РїРѕСЃР»РµРґРЅСЏСЏ Р·Р°РїСЂРѕС€РµРЅРЅР°СЏ РѕРїРµСЂР°С†РёСЏ
  let clearRaf = 0;             // СЃРЅСЏС‚РёРµ С„Р»Р°РіР° syncing РІ СЃР»РµРґСѓСЋС‰РµРј РєР°РґСЂРµ
  let lastWriteAt = 0;          // timestamp РїРѕСЃР»РµРґРЅРµР№ РїСЂРѕРіСЂР°РјРјРЅРѕР№ Р·Р°РїРёСЃРё
  let lastWriteTarget: HTMLElement | null = null; // РІ РєР°РєСѓСЋ РїР°РЅРµР»СЊ РїРёСЃР°Р»Рё

  let blocks: Block[] | null = null;
  let maps: UnitMaps | null = null;
  let lineStarts: Int32Array | null = null;
  let lineH = 0;                // РёР·РјРµСЂРµРЅРЅР°СЏ РІС‹СЃРѕС‚Р° СЃС‚СЂРѕРєРё СЂРµРґР°РєС‚РѕСЂР°, px
  let previewPadTop = 0;        // РІРµСЂС…РЅРёР№ padding РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂР°, px

  /** РђРЅРєРѕСЂРЅР°СЏ РїСЂРёРІСЏР·РєР° РґРѕСЃС‚СѓРїРЅР° С‚РѕР»СЊРєРѕ РєРѕРіРґР° РІ DOM РµСЃС‚СЊ СЂР°Р·РјРµС‡РµРЅРЅС‹Рµ Р±Р»РѕРєРё. */
  function hasAnchors(): boolean {
    return !!blocks && blocks.length > 0 && !!maps && !!lineStarts && lineH > 0;
  }

  function maxScroll(el: HTMLElement): number {
    return el.scrollHeight - el.clientHeight;
  }

  /** Р­С…Рѕ РІРѕР·РјРѕР¶РЅРѕ С‚РѕР»СЊРєРѕ РѕС‚ РїР°РЅРµР»Рё, РІ РєРѕС‚РѕСЂСѓСЋ РїРёСЃР°Р»Рё (`lastWriteTarget`). */
  function isEcho(target: EventTarget | null): boolean {
    if (target !== lastWriteTarget) return false;
    return syncing || performance.now() - lastWriteAt < ECHO_MS;
  }

  /** Р—Р°РїРёСЃСЊ scrollTop РїРѕРґ С„Р»Р°РіРѕРј syncing; С„Р»Р°Рі СЃРЅРёРјР°РµС‚СЃСЏ РІ СЃР»РµРґСѓСЋС‰РµРј РєР°РґСЂРµ. */
  function writeScrollTop(to: HTMLElement, target: number) {
    if (to.clientHeight === 0) return; // РїР°РЅРµР»СЊ СЃРєСЂС‹С‚Р° вЂ” РЅРµС‡РµРіРѕ СЃРёРЅС…СЂРѕРЅРёР·РёСЂРѕРІР°С‚СЊ
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

  /** РЎС‚Р°РІРёС‚ РѕРїРµСЂР°С†РёСЋ РЅР° СЃР»РµРґСѓСЋС‰РёР№ РєР°РґСЂ; РїСЂРё С‡Р°СЃС‚С‹С… СЃРєСЂРѕР»Р»Р°С… РїРѕР±РµР¶РґР°РµС‚ РїРѕСЃР»РµРґРЅСЏСЏ. */
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

  /** РџСЂРѕРїРѕСЂС†РёСЏ вЂ” fallback, РєРѕРіРґР° Р°РЅРєРѕСЂРѕРІ РЅРµС‚. */
  function syncPair(from: HTMLElement, to: HTMLElement) {
    const fromMax = maxScroll(from);
    const toMax = maxScroll(to);
    if (fromMax <= 0 || toMax <= 0) return; // РїР°РЅРµР»СЊ СЃРєСЂС‹С‚Р°/Р±РµР· СЃРєСЂРѕР»Р»Р°
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
        block = b; // РІРµСЂС…РЅРёР№ РІРёРґРёРјС‹Р№ Р±Р»РѕРє
        break;
      }
    }
    if (!block) block = bl[bl.length - 1] ?? null; // РІСЃС‘ РІС‹С€Рµ вЂ” РїРѕСЃР»РµРґРЅРёР№
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
      alignEditorToPreview(); // СЂРµРґР°РєС‚РѕСЂ вЂ” РёСЃС‚РѕС‡РЅРёРє РёСЃС‚РёРЅС‹ РїСЂРё РІРєР»СЋС‡РµРЅРёРё
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
      lastWriteTarget = preview; // РїСЂРѕРіСЂР°РјРјРЅС‹Р№ scrollIntoView РёРґС‘С‚ РІ РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂ
      if (clearRaf) cancelAnimationFrame(clearRaf);
      clearRaf = requestAnimationFrame(() => {
        syncing = false;
        clearRaf = 0;
      });
    },
  };
}
