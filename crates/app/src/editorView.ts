// editorView — тир-2 представление редактора (textarea). Держит DOM-буфер:
// ввод уходит в стор (`setText`), внешняя смена документа приходит из стора.
// `editor.value` — лишь отображение проекции, не источник истины (D5).

import type { DocStore } from "./docStore";

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

  host.addEventListener("input", onInput);
  host.addEventListener("keyup", onSelect);
  host.addEventListener("click", onSelect);
  host.addEventListener("select", onSelect);

  // Внешняя смена документа (open/new/switchTab): подтягиваем буфер без генерации `input`.
  // Сравнение значений исключает цикл store → view → store.
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
      host.removeEventListener("keyup", onSelect);
      host.removeEventListener("click", onSelect);
      host.removeEventListener("select", onSelect);
      unsubscribe();
    },
  };
}
