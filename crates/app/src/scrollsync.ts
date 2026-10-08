// scrollsync.ts — синхронизация вертикальной прокрутки редактора и предпросмотра.
//
// v2: точная анкорная привязка через `data-md`. Когда в DOM есть размеченные
// блоки (mapped-рендер, т.е. включён инспектор), позиция пересчитывается по
// содержанию: редактор → блок, содержащий верхнюю видимую логическую строку;
// предпросмотр → строка начала верхнего видимого блока. Иначе (и между вводом
// и перерендером) работает прежняя пропорциональная синхронизация как fallback.
//
// Карты byte↔UTF-16, начала строк и блоки берутся из общего RenderIndex
// (Фаза 4): их строит previewView по тексту рендера, здесь только читается
// снимок — тот же, что видит инспектор (без дубля построения).
//
// Почему логические строки тождественны визуальным: мягкий перенос у редактора
// отключён (`wrap="off"` + `white-space: pre` в style.css), поэтому
// `scrollTop/lineH` точно указывает на логическую строку, а `lineStarts`
// (по `\n`) и `lineH` согласованы. При включённом переносе формула теряет смысл.
//
// Защита от обратной связи двухуровневая:
// 1) флаг `syncing` поднимается ДО записи scrollTop и снимается в rAF — гасит
//    синхронное эхо «запись → scroll-событие»;
// 2) окно `ECHO_MS` после программной записи — гасит инерционное эхо WebView2,
//    которое приходит уже после снятия флага. Игнорируются события только той
//    панели, в которую писала синхронизация (`lastWriteTarget`), иначе быстрый
//    скролл источника сам себя глушил бы.

import {
  type Block,
  type UnitMaps,
  bytesToUnits,
  findBlockContaining,
  findLineAt,
  nextBlockAfter,
  unitsToBytes,
} from "./mapping";
import type { RenderIndex } from "./renderIndex";

export interface ScrollSync {
  isEnabled(): boolean;
  /** Включение/выключение; при включении панели сразу выравниваются. */
  setEnabled(enabled: boolean): void;
  /**
   * Индекс изменился (перестроен после рендера или сброшен при правке).
   * Данные берутся из RenderIndex; без снимка — fallback на пропорцию.
   */
  onIndexChanged(): void;
  /**
   * Гасит синхронизацию на короткое время: программная прокрутка инспектора
   * не должна тянуть вторую панель.
   */
  suspend(): void;
}

/** Окно игнорирования эха WebView2 после программной записи scrollTop, мс. */
const ECHO_MS = 100;

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

export function createScrollSync(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  /** Общий индекс рендера: карты, строки, блоки (Фаза 4). */
  index: RenderIndex;
}): ScrollSync {
  const { editor, preview, index } = opts;

  let enabled = true;
  let syncing = false;          // поднят на время программной записи scrollTop
  let rafId = 0;                // отложенная запись (throttle на кадр)
  let pending: (() => void) | null = null; // последняя запрошенная операция
  let clearRaf = 0;             // снятие флага syncing в следующем кадре
  let lastWriteAt = 0;          // timestamp последней программной записи
  let lastWriteTarget: HTMLElement | null = null; // в какую панель писали

  let blocks: Block[] | null = null;
  let maps: UnitMaps | null = null;
  let lineStarts: Int32Array | null = null;
  let lineH = 0;                // измеренная высота строки редактора, px
  let previewPadTop = 0;        // верхний padding предпросмотра, px

  /** Анкорная привязка доступна только когда в DOM есть размеченные блоки. */
  function hasAnchors(): boolean {
    return !!blocks && blocks.length > 0 && !!maps && !!lineStarts && lineH > 0;
  }

  function maxScroll(el: HTMLElement): number {
    return el.scrollHeight - el.clientHeight;
  }

  /**
   * Эхо от нашей программной записи приходит только от той панели, в которую
   * писали (`lastWriteTarget`). Флаг `syncing` гасит синхронное эхо, окно
   * `ECHO_MS` — инерционное. События панели-источника не глушим, иначе быстрый
   * скролл источника сам себя дропал бы.
   */
  function isEcho(target: EventTarget | null): boolean {
    if (target !== lastWriteTarget) return false;
    return syncing || performance.now() - lastWriteAt < ECHO_MS;
  }

  /** Запись scrollTop под флагом syncing; флаг снимается в следующем кадре. */
  function writeScrollTop(to: HTMLElement, target: number) {
    if (to.clientHeight === 0) return; // панель скрыта — нечего синхронизировать
    const hi = maxScroll(to);
    if (hi <= 0) return;
    const next = clamp(target, 0, hi);
    if (Math.abs(to.scrollTop - next) < 1) return; // уже на месте
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

  /** Ставит операцию на следующий кадр; при частых скроллах побеждает последняя. */
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

  /** Пропорция — fallback, когда анкоров нет. */
  function syncPair(from: HTMLElement, to: HTMLElement) {
    const fromMax = maxScroll(from);
    const toMax = maxScroll(to);
    if (fromMax <= 0 || toMax <= 0) return; // панель скрыта/без скролла
    schedule(() => {
      if (!enabled) return;
      writeScrollTop(to, (from.scrollTop / fromMax) * toMax);
    });
  }

  // ---------- редактор → предпросмотр ----------

  function alignEditorToPreview() {
    if (!hasAnchors()) {
      syncPair(editor, preview);
      return;
    }
    const ls = lineStarts!;
    const mp = maps!;
    const bl = blocks!;
    const topLine = clamp(Math.floor(editor.scrollTop / lineH), 0, ls.length - 1);
    // Верхняя логическая строка 0 (0 ≤ editor.scrollTop < lineH): предпросмотр
    // ставим в естественное начало (0), а не выравниваем блок по верхнему padding.
    // У первого блока нет соседа сверху, а его верхний margin (зазор для эвристики
    // «верхний блок») не нужно «съедать» прокруткой — иначе панели сдвигаются вниз
    // (BUG-005). Известное ограничение: для документов, где первый блок начинается
    // не с байта 0 (ведущие пустые строки/ссылочные определения), прямая и обратная
    // синхронизация тут взаимно не обратимы.
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
      // Разность координат надёжнее offsetTop-цепочки при вложенности/sticky.
      const pr = preview.getBoundingClientRect();
      const br = block.el.getBoundingClientRect();
      const delta = br.top - pr.top;
      writeScrollTop(preview, preview.scrollTop + delta - previewPadTop);
    });
  }

  // ---------- предпросмотр → редактор ----------

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
        block = b; // верхний видимый блок
        break;
      }
    }
    if (!block) block = bl[bl.length - 1] ?? null; // всё выше — последний
    if (!block) {
      syncPair(preview, editor);
      return;
    }
    const mp = maps!;
    const bytePos = block.start;
    const line = findLineAt(lineStarts!, bytesToUnits(mp, bytePos));
    schedule(() => {
      if (!enabled) return;
      // Инвариант: обе стороны должны быть взаимно обратны §5.1
      // (topLine = floor(scrollTop / lineH)). При scrollTop = line*lineH строка
      // `line` встаёт на тот же визуальный отступ, что первая строка при
      // scrollTop = 0 (padding-top), а floor(scrollTop/lineH) == line.
      writeScrollTop(editor, line * lineH);
    });
  }

  function measure() {
    const cs = getComputedStyle(editor);
    const lh = parseFloat(cs.lineHeight);
    const fs = parseFloat(cs.fontSize);
    // "normal"/нечисловое → эвристика fontSize * 1.2.
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
      alignEditorToPreview(); // редактор — источник истины при включении
    },
    onIndexChanged() {
      const snap = index.current();
      if (!snap) {
        // Между вводом и рендером анкоры устарели — только пропорция.
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
      lastWriteTarget = preview; // программный scrollIntoView идёт в предпросмотр
      if (clearRaf) cancelAnimationFrame(clearRaf);
      clearRaf = requestAnimationFrame(() => {
        syncing = false;
        clearRaf = 0;
      });
    },
  };
}
