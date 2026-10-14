// editorView — тир-2 представление редактора (textarea). `editor.value` — лишь
// отображение проекции стора, не источник истины (D5).
//
// Нативный undo/redo браузера отключён: владелец undo-стека — Rust
// (D5/ADR-0028), иначе два стека разъедутся.

import type { DocStore } from "./docStore";
import { byteToUtf16 } from "../core/mapping";

export interface EditorView {
  focus(): void;
  dispose(): void;
}

export function createEditorView(
  host: HTMLTextAreaElement,
  store: DocStore,
  onSelection: () => void
): EditorView {
  const onInput = (): void => store.setText(host.value);
  const onSelect = (): void => onSelection();

  // Rust отдаёт выделение в байтах UTF-8, textarea оперирует UTF-16 code units —
  // конвертируем по тексту результата. Однопроходного перевода достаточно:
  // полные карты на весь документ ради одного выделения — лишняя аллокация O(n).
  async function applyHistory(
    run: () => Promise<{ start: number; end: number } | null>
  ): Promise<void> {
    const range = await run();
    if (!range) return;
    const start = byteToUtf16(host.value, range.start);
    const end = byteToUtf16(host.value, range.end);
    host.setSelectionRange(start, end);
    onSelection();
  }

  // Ctrl+Z — undo; Ctrl+Y и Ctrl+Shift+Z — redo. `e.code` вместо `e.key`:
  // независимо от раскладки.
  const onKeyDown = (e: KeyboardEvent): void => {
    if (!e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.code === "KeyZ" && !e.shiftKey) {
      e.preventDefault();
      void applyHistory(() => store.undo());
    } else if (e.code === "KeyY" || (e.code === "KeyZ" && e.shiftKey)) {
      e.preventDefault();
      void applyHistory(() => store.redo());
    }
  };

  // Меню браузера/контекстное шлют `beforeinput` вместо keydown. Ctrl+Z сюда не
  // попадает: keydown уже отменил событие.
  const onBeforeInput = (e: InputEvent): void => {
    if (e.inputType === "historyUndo") {
      e.preventDefault();
      void applyHistory(() => store.undo());
    } else if (e.inputType === "historyRedo") {
      e.preventDefault();
      void applyHistory(() => store.redo());
    }
  };

  host.addEventListener("input", onInput);
  host.addEventListener("keydown", onKeyDown);
  host.addEventListener("beforeinput", onBeforeInput);
  host.addEventListener("keyup", onSelect);
  host.addEventListener("click", onSelect);
  host.addEventListener("select", onSelect);

  // Внешняя смена документа (open/new/switchTab): подтягиваем буфер без
  // генерации `input`. Сравнение значений исключает цикл store → view → store.
  const unsubscribe = store.subscribe((tabs, activeId) => {
    const active = tabs.find((t) => t.id === activeId) ?? null;
    if (active && active.text !== host.value) {
      host.value = active.text;
      host.setSelectionRange(0, 0);
      onSelection();
    }
  });

  return {
    focus() {
      host.focus();
    },
    dispose() {
      host.removeEventListener("input", onInput);
      host.removeEventListener("keydown", onKeyDown);
      host.removeEventListener("beforeinput", onBeforeInput);
      host.removeEventListener("keyup", onSelect);
      host.removeEventListener("click", onSelect);
      host.removeEventListener("select", onSelect);
      unsubscribe();
    },
  };
}
