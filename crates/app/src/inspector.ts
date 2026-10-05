// inspector.ts — режим инспектора: двусторонняя подсветка «блок предпросмотра
// ↔ фрагмент исходника в редакторе». Блоки размечаются на Rust-стороне
// (md-core::to_html_mapped) атрибутом data-md="start,end", где start/end —
// байтовые смещения в исходном markdown.
//
// Главная сложность — единицы измерения: textarea оперирует UTF-16 code units
// (selectionStart/End), а pulldown-cmark — байтами UTF-8. Поэтому здесь строятся
// две префиксные карты byte↔UTF-16 по editor.value (который WebView может
// нормализовать по CRLF — именно он и есть источник истины).

interface Block {
  start: number; // байтовый offset в editor.value (inclusive)
  end: number;   // байтовый offset в editor.value (exclusive)
  el: HTMLElement;
}

interface UnitMaps {
  b2u: Int32Array; // байтовый индекс → позиция в UTF-16 code units
  u2b: Int32Array; // позиция в UTF-16 code units → байтовый индекс
}

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

export function createInspector(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  statusEl: HTMLElement;
}): Inspector {
  const { editor, preview, statusEl } = opts;

  let active = false;
  let blocks: Block[] | null = null;
  let maps: UnitMaps | null = null;
  let activeBlockEl: HTMLElement | null = null;

  // Состояние textarea и фокуса до включения режима (восстанавливаем на выходе).
  let savedSelection = { start: 0, end: 0 };
  let savedScrollTop = 0;
  let savedFocus: HTMLElement | null = null;

  let lastScrollAt = 0;

  // ---------- byte ↔ UTF-16 ----------

  function utf8ByteLength(code: number): number {
    if (code < 0x80) return 1;
    if (code < 0x800) return 2;
    if (code < 0x10000) return 3;
    return 4;
  }

  function buildMaps(text: string): UnitMaps {
    let byteLen = 0;
    // Итерация по code points: суррогатные пары не разрываются.
    for (const ch of text) byteLen += utf8ByteLength(ch.codePointAt(0)!);

    const b2u = new Int32Array(byteLen + 1);
    const u2b = new Int32Array(text.length + 1);
    let byte = 0;
    let unit = 0;
    for (const ch of text) {
      const code = ch.codePointAt(0)!;
      const bl = utf8ByteLength(code);
      const ul = code > 0xffff ? 2 : 1;
      for (let i = 0; i < bl; i++) b2u[byte + i] = unit;
      for (let i = 0; i < ul; i++) u2b[unit + i] = byte;
      byte += bl;
      unit += ul;
    }
    b2u[byte] = unit;
    u2b[unit] = byte;
    return { b2u, u2b };
  }

  function bytesToUnits(b: number): number {
    if (!maps) return 0;
    // Зажимаем индекс: выход за границы Int32Array даёт undefined (→ 0, начало
    // документа) и подсветил бы не тот блок. Лучше крайняя корректная позиция.
    const i = Math.min(Math.max(b, 0), maps.b2u.length - 1);
    return maps.b2u[i]!;
  }

  function unitsToBytes(u: number): number {
    if (!maps) return 0;
    const i = Math.min(Math.max(u, 0), maps.u2b.length - 1);
    return maps.u2b[i]!;
  }

  // ---------- блоки ----------

  function parseRange(el: HTMLElement): { start: number; end: number } | null {
    const raw = el.getAttribute("data-md");
    if (!raw) return null;
    const [s, e] = raw.split(",");
    const start = Number(s);
    const end = Number(e);
    if (!Number.isFinite(start) || !Number.isFinite(end)) return null;
    return { start, end };
  }

  function collectBlocks(): Block[] {
    const list: Block[] = [];
    for (const el of preview.querySelectorAll<HTMLElement>("[data-md]")) {
      const range = parseRange(el);
      if (range) list.push({ start: range.start, end: range.end, el });
    }
    list.sort((a, b) => a.start - b.start);
    return list;
  }

  function findBlock(bytePos: number): Block | null {
    if (!blocks || blocks.length === 0) return null;
    for (const block of blocks) {
      if (bytePos >= block.start && bytePos < block.end) return block;
    }
    // Каретка точно в конце документа относится к последнему блоку.
    const last = blocks[blocks.length - 1];
    return bytePos === last.end ? last : null;
  }

  // ---------- подсветка ----------

  function clearBlockHighlight() {
    if (activeBlockEl) {
      activeBlockEl.classList.remove("inspect-active");
      activeBlockEl = null;
    }
  }

  function scrollIntoViewIfNeeded(el: HTMLElement) {
    const pr = preview.getBoundingClientRect();
    const er = el.getBoundingClientRect();
    if (er.top >= pr.top && er.bottom <= pr.bottom) return; // уже виден
    const now = performance.now();
    if (now - lastScrollAt < 100) return; // throttle, чтобы не дёргать скролл
    lastScrollAt = now;
    el.scrollIntoView({ block: "nearest" });
  }

  function applyBlock(el: HTMLElement, focus: boolean, force = false) {
    if (!force && activeBlockEl === el) return; // гистерезис — не дёргать на том же блоке
    clearBlockHighlight();
    const range = parseRange(el);
    if (!range) return;
    const start = bytesToUnits(range.start);
    const end = bytesToUnits(range.end);
    el.classList.add("inspect-active");
    activeBlockEl = el;
    // Сначала фокус, потом диапазон: focus() может восстановить сохранённое
    // выделение textarea и затереть только что выставленный диапазон.
    if (focus) editor.focus({ preventScroll: true });
    editor.setSelectionRange(start, end);
    scrollIntoViewIfNeeded(el);
  }

  function restoreSavedFocus() {
    if (savedFocus && savedFocus.isConnected && typeof savedFocus.focus === "function") {
      savedFocus.focus({ preventScroll: true });
    }
  }

  // ---------- слушатели ----------

  function onMouseOver(e: MouseEvent) {
    if (!active) return;
    const target = e.target as Element | null;
    const el = target?.closest?.("[data-md]") as HTMLElement | null;
    if (!el) return;
    applyBlock(el, true);
  }

  function onMouseOut(e: MouseEvent) {
    if (!active) return;
    const rel = e.relatedTarget as Node | null;
    if (rel && preview.contains(rel)) return; // всё ещё внутри preview
    clearBlockHighlight();
    restoreSavedFocus();
  }

  function onEditorSelection() {
    if (!active || !blocks || !maps) return;
    if (document.activeElement !== editor) return;
    // «Каретка» = активный край выделения. При протяжке мышью/Shift+стрелках
    // selectionStart фиксирован (якорь), а двигается противоположный край,
    // поэтому ориентируемся на selectionDirection (AC-5).
    const backward = editor.selectionDirection === "backward";
    const caretPos = backward
      ? editor.selectionStart ?? 0
      : editor.selectionEnd ?? 0;
    const block = findBlock(unitsToBytes(caretPos));
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
    const target = e.target as Element | null;
    if (target?.closest?.(INTERACTIVE_SELECTOR)) return; // дать tables.ts/ссылкам работать
    const el = target?.closest?.("[data-md]") as HTMLElement | null;
    if (!el) return;
    e.preventDefault();
    e.stopPropagation();
    // force: клик должен вернуть выделение и фокус в редактор, даже если блок
    // уже подсвечен (иначе focus уходит в предпросмотр, а выделение «гаснет»).
    applyBlock(el, true, true);
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
  }

  function disable(): void {
    if (!active) return;
    active = false;
    clearBlockHighlight();
    blocks = null;
    maps = null;
    statusEl.hidden = true;
    preview.removeEventListener("mouseover", onMouseOver);
    preview.removeEventListener("mouseout", onMouseOut);
    preview.removeEventListener("click", onClickCapture, true);
    document.removeEventListener("selectionchange", onEditorSelection);
    // OQ-3: восстанавливаем состояние редактора и фокус.
    editor.setSelectionRange(savedSelection.start, savedSelection.end);
    editor.scrollTop = savedScrollTop;
    restoreSavedFocus();
  }

  function onRendered(markdown?: string): void {
    clearBlockHighlight();
    blocks = null;
    maps = null;
    // Без аргумента — только сброс подсветки (вызывается из input-хендлера:
    // DOM ещё старый, переиндексация будет после установки нового innerHTML).
    if (!active || markdown === undefined) return;
    blocks = collectBlocks();
    // Смещения в data-md посчитаны по тексту рендера; строим карту по нему же.
    maps = buildMaps(markdown);
  }

  return { isActive, enable, disable, onRendered, onEditorActivity: onEditorSelection };
}