// statusBar.ts — статусная строка и заголовок окна. Читают проекцию docStore
// (§4.2: заголовок/dirty/размер — из стора, позиция курсора — из редактора).
// Вынесено из main.ts, чтобы тот остался композиционным корнем.

import { getCurrentWindow } from "@tauri-apps/api/window";
import type { DocStore } from "./docStore";

export interface StatusBar {
  /** Заголовок окна + имя файла + маркер несохранённых правок. */
  updateTitle(): void;
  /** Позиция курсора (Стр/Кол) и размер документа. */
  updateStatus(): void;
  /** Кратковременное сообщение в статусбаре. */
  flash(msg: string): void;
}

export function createStatusBar(opts: {
  store: DocStore;
  editor: HTMLTextAreaElement;
  appName: string;
}): StatusBar {
  const { store, editor, appName } = opts;
  const fileLabel = document.getElementById("file-label") as HTMLElement;
  const statPos = document.getElementById("stat-pos") as HTMLElement;
  const statSize = document.getElementById("stat-size") as HTMLElement;
  const statMsg = document.getElementById("stat-msg") as HTMLElement;

  function baseName(p: string): string {
    const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return i >= 0 ? p.slice(i + 1) : p;
  }

  function updateStatus(): void {
    const s = store.state();
    const pos = editor.selectionStart ?? 0;
    const before = s.text.slice(0, pos);
    const line = before.split("\n").length;
    const col = pos - (before.lastIndexOf("\n") + 1) + 1;
    statPos.textContent = `Стр ${line}, Кол ${col}`;
    statSize.textContent = `${[...s.text].length} симв.`;
  }

  function updateTitle(): void {
    const s = store.state();
    const name = s.path ? baseName(s.path) : "безымянный";
    // Заголовок окна — не критично: если команда недоступна/упала, не плодим
    // unhandled rejection (разрешение core:window:allow-set-title — в capabilities).
    getCurrentWindow()
      .setTitle(`${s.dirty ? "● " : ""}${name} — ${appName}`)
      .catch((e) => console.warn("set_title:", e));
    fileLabel.textContent = name + (s.dirty ? " ●" : "");
  }

  function flash(msg: string): void {
    statMsg.textContent = msg;
    window.setTimeout(() => {
      if (statMsg.textContent === msg) statMsg.textContent = "";
    }, 4000);
  }

  return { updateTitle, updateStatus, flash };
}
