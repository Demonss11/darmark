// shell.ts — привязка оболочки к DOM: кнопки тулбара, горячие клавиши,
// тумблеры предпросмотра/синхронизации, фокус активной панели, закрытие окна.
// Логики домена здесь нет — только вызовы команд, переданных композиционным
// корнем (main.ts). Вынесено из main.ts ради тонкого корня.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirm } from "@tauri-apps/plugin-dialog";
import type { PaneId } from "./ids";

export interface ShellCommands {
  newFile(): void;
  openFile(): void;
  saveFile(): void;
  saveAs(): void;
  toggleInspector(): void;
  /** Показать/скрыть вторичную панель (предпросмотр). */
  setPreviewVisible(on: boolean): void;
  setSyncEnabled(on: boolean): void;
  setActivePane(paneId: PaneId): void;
  isDirty(): boolean;
}

export interface ShellOptions {
  commands: ShellCommands;
  inspectorActive(): boolean;
  editorPane: PaneId;
  previewPane: PaneId;
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
}

function byId<T extends HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

export function createShell(opts: ShellOptions): void {
  const { commands, editor, preview } = opts;
  const chkPreview = byId<HTMLInputElement>("chk-preview");
  const chkSync = byId<HTMLInputElement>("chk-sync");

  byId<HTMLButtonElement>("btn-new").addEventListener("click", commands.newFile);
  byId<HTMLButtonElement>("btn-open").addEventListener("click", commands.openFile);
  byId<HTMLButtonElement>("btn-save").addEventListener("click", commands.saveFile);
  byId<HTMLButtonElement>("btn-save-as").addEventListener("click", commands.saveAs);
  byId<HTMLButtonElement>("btn-inspect").addEventListener("click", commands.toggleInspector);

  // Активная панель (фокус) — для визуализации Pane/View.
  editor.addEventListener("focus", () => commands.setActivePane(opts.editorPane));
  preview.addEventListener("mousedown", () => commands.setActivePane(opts.previewPane));

  chkPreview.addEventListener("change", () => commands.setPreviewVisible(chkPreview.checked));
  chkSync.addEventListener("change", () => commands.setSyncEnabled(chkSync.checked));

  window.addEventListener("keydown", (e: KeyboardEvent) => {
    // Esc выходит из режима инспектора (кроме случая открытого меню фильтра).
    if (e.key === "Escape" && opts.inspectorActive()) {
      if (!document.querySelector(".col-filter-menu")) {
        e.preventDefault();
        commands.toggleInspector();
      }
      return;
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    const k = e.key.toLowerCase();
    if (k === "n" && !e.shiftKey) { e.preventDefault(); commands.newFile(); }
    else if (k === "o") { e.preventDefault(); commands.openFile(); }
    else if (k === "s" && e.shiftKey) { e.preventDefault(); commands.saveAs(); }
    else if (k === "s") { e.preventDefault(); commands.saveFile(); }
    else if (k === "p") { e.preventDefault(); chkPreview.click(); }
    else if (k === "i") { e.preventDefault(); commands.toggleInspector(); }
  });

  // Закрытие окна: при несохранённых изменениях спрашиваем подтверждение;
  // если согласен — закрываем через destroy() (preventDefault иначе отменит закрытие).
  const appWindow = getCurrentWindow();
  appWindow
    .onCloseRequested(async (event) => {
      if (!commands.isDirty()) return;
      event.preventDefault();
      const ok = await confirm("Закрыть приложение с несохранёнными изменениями?", {
        title: "darmark",
        kind: "warning",
      });
      if (ok) await appWindow.destroy();
    })
    .catch((e) => console.error("onCloseRequested:", e));
}
