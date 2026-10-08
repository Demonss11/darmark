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
    if (!selected) {
      // Пустое выделение — курсор между маркерами.
      commit(value.slice(0, s) + marker + marker + value.slice(e), s + marker.length, s + marker.length);
      return;
    }
    // Краевые whitespace-символы (пробелы, табы, `\n`/`\r`, NBSP) оставляем
    // СНАРУЖИ маркеров: мышью в выделение часто попадает замыкающий пробел,
    // а `**слово **` ломает форматирование.
    const lead = selected.length - selected.trimStart().length;
    const trail = selected.length - selected.trimEnd().length;
    const core = selected.slice(lead, selected.length - trail);
    if (!core) {
      // Выделение из одних whitespace — оборачиваем как есть (нет ядра).
      commit(
        value.slice(0, s) + marker + selected + marker + value.slice(e),
        s + marker.length,
        s + marker.length + selected.length
      );
      return;
    }
    // Пробелы по краям сохраняются вне маркеров; ядро выделяется для продолжения.
    const insert =
      selected.slice(0, lead) + marker + core + marker + selected.slice(selected.length - trail);
    const coreStart = s + lead + marker.length;
    commit(value.slice(0, s) + insert + value.slice(e), coreStart, coreStart + core.length);
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
