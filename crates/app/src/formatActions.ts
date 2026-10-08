// formatActions.ts — Markdown-обёртки над выделением textarea (icon-toolbar idea5).
//
// Модуль меняет только `host.value` вокруг выделения, восстанавливает выделение и
// диспатчит `input` — это единственный путь обновления стора (через editorView).
// Нативная история undo textarea при программных правках сбрасывается — известное
// ограничение (лечится Rust-undo, отдельная задача).

export interface FormatActions {
  bold(): void;
  italic(): void;
  code(): void;
  heading(): void;
  link(): void;
}

export function createFormatActions(host: HTMLTextAreaElement): FormatActions {
  /** Применить новое значение и выделение, уведомив стор событием `input`. */
  function commit(value: string, selStart: number, selEnd: number): void {
    host.value = value;
    host.focus();
    host.setSelectionRange(selStart, selEnd);
    host.dispatchEvent(new Event("input"));
  }

  /** Обернуть выделение маркером с обеих сторон (`**`, `_`, `` ` ``). */
  function wrap(marker: string): void {
    const { selectionStart: s, selectionEnd: e, value } = host;
    const selected = value.slice(s, e);
    const next = value.slice(0, s) + marker + selected + marker + value.slice(e);
    if (selected) {
      // Выделяем текст внутри маркеров, чтобы правку можно было продолжить.
      commit(next, s + marker.length, s + marker.length + selected.length);
    } else {
      // Пустое выделение — курсор между маркерами.
      commit(next, s + marker.length, s + marker.length);
    }
  }

  function bold(): void {
    wrap("**");
  }
  function italic(): void {
    wrap("_");
  }
  function code(): void {
    wrap("`");
  }

  /** Сделать выделенные строки заголовком (`# ` перед каждой). */
  function heading(): void {
    const { selectionStart: s, selectionEnd: e, value } = host;
    const lineStart = value.lastIndexOf("\n", s - 1) + 1;
    let lineEnd = value.indexOf("\n", e);
    if (lineEnd === -1) lineEnd = value.length;
    const block = value.slice(lineStart, lineEnd);
    const prefixed = block
      .split("\n")
      .map((ln) => (ln.startsWith("#") ? ln : "# " + ln))
      .join("\n");
    const next = value.slice(0, lineStart) + prefixed + value.slice(lineEnd);
    // Выделяем изменённый блок целиком.
    commit(next, lineStart, lineStart + prefixed.length);
  }

  /** Обернуть выделение ссылкой `[текст](url)`, выделив `url` для замены. */
  function link(): void {
    const { selectionStart: s, selectionEnd: e, value } = host;
    const selected = value.slice(s, e);
    const text = selected || "текст";
    const inserted = `[${text}](url)`;
    const next = value.slice(0, s) + inserted + value.slice(e);
    const urlStart = s + 1 + text.length + 2;
    commit(next, urlStart, urlStart + 3);
  }

  return { bold, italic, code, heading, link };
}
