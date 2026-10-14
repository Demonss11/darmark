// shell.ts — привязка оболочки к DOM: кнопки тулбара, горячие клавиши, тумблеры
// предпросмотра/синхронизации, формат-группа, палитра, триггер дровера плагинов,
// журнал-панель, фокус активной панели, закрытие окна. Доменной логики нет — только
// вызовы команд из main.ts.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirm } from "@tauri-apps/plugin-dialog";
import type { PaneId } from "../core/ids";
import type { FormatActions } from "../editor/formatActions";
import type { Drawer } from "../ui/drawer";
import type { LogPanel } from "../panels/logPanel";
import { isModalOpen } from "../ui/dialog";
import { openDevtools } from "../core/tauri";

interface ShellCommands {
  newFile(): void;
  openFile(): void;
  saveFile(): void;
  saveAs(): void;
  toggleInspector(): void;
  setPreviewVisible(on: boolean): void;
  setSyncEnabled(on: boolean): void;
  setActivePane(paneId: PaneId): void;
  isDirty(): boolean;
  palette(): void;
  reloadPlugins(): void;
  flash(msg: string): void;
}

interface ShellOptions {
  commands: ShellCommands;
  format: FormatActions;
  drawer: Drawer;
  inspectorActive(): boolean;
  editorPane: PaneId;
  previewPane: PaneId;
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  logPanel: LogPanel;
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

  const formatById: Record<string, () => void> = {
    bold: opts.format.bold,
    italic: opts.format.italic,
    code: opts.format.code,
    heading: opts.format.heading,
    link: opts.format.link,
  };
  for (const btn of document.querySelectorAll<HTMLButtonElement>("#format-group button[data-action]")) {
    const action = btn.dataset.action ?? "";
    const fn = formatById[action];
    if (fn) btn.addEventListener("click", fn);
  }

  byId<HTMLButtonElement>("palette-trigger").addEventListener("click", commands.palette);
  byId<HTMLButtonElement>("tb-plugins").addEventListener("click", () => opts.drawer.toggle());

  editor.addEventListener("focus", () => commands.setActivePane(opts.editorPane));
  preview.addEventListener("mousedown", () => commands.setActivePane(opts.previewPane));
  document.addEventListener("focusin", (e) => {
    const pane = (e.target as Element | null)?.closest?.(".pane") as HTMLElement | null;
    const id = pane?.dataset.pane;
    if (id === opts.editorPane) commands.setActivePane(opts.editorPane);
    else if (id === opts.previewPane) commands.setActivePane(opts.previewPane);
  });

  chkPreview.addEventListener("change", () => commands.setPreviewVisible(chkPreview.checked));
  chkSync.addEventListener("change", () => commands.setSyncEnabled(chkSync.checked));

  window.addEventListener("keydown", (e: KeyboardEvent) => {
    if (isModalOpen()) return;
    if (e.key === "Escape") {
      const panel = document.getElementById("logs-panel");
      if (panel && !panel.hidden && panel.contains(document.activeElement)) {
        e.preventDefault();
        const st = opts.logPanel.state();
        if (st === "expanded") opts.logPanel.collapse();
        else if (st === "collapsed") opts.logPanel.hide();
        return;
      }
    }
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
    else if (k === "a" && e.shiftKey) { e.preventDefault(); opts.logPanel.toggleMode("ai"); }
    else if (import.meta.env.DEV && k === "i" && e.shiftKey) {
      e.preventDefault();
      void openDevtools().catch((err) => console.error("open_devtools:", err));
    }
    else if (k === "i") { e.preventDefault(); commands.toggleInspector(); }
    else if (k === "b") { e.preventDefault(); opts.format.bold(); }
    else if (k === "k" && e.shiftKey) { e.preventDefault(); opts.format.link(); }
    else if (k === "k") { e.preventDefault(); commands.palette(); }
    else if (k === "r" && (e.shiftKey || e.altKey)) { e.preventDefault(); }
    else if (k === "r") { e.preventDefault(); commands.reloadPlugins(); }
  });

  const appWindow = getCurrentWindow();
  appWindow
    .onCloseRequested(async (event) => {
      if (!commands.isDirty()) return;
      event.preventDefault();
      const ok = await confirm("Закрыть приложение с несохранёнными изменениями?", {
        title: "ПЕНАЛ",
        kind: "warning",
      });
      if (ok) await appWindow.destroy();
    })
    .catch((e) => console.error("onCloseRequested:", e));
}
