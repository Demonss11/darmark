// shell.ts — привязка оболочки к DOM: кнопки тулбара, горячие клавиши, тумблеры
// предпросмотра/синхронизации, формат-группа и палитра (заглушка), заглушка
// журнала, фокус активной панели, закрытие окна. Логики домена здесь нет — только
// вызовы команд из main.ts.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirm } from "@tauri-apps/plugin-dialog";
import type { PaneId } from "./ids";
import type { FormatActions } from "./formatActions";
import type { Sidebar } from "./sidebar";
import { openDevtools } from "./tauri";

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
  /** Палитра команд — заглушка (H2). */
  palette(): void;
  /** Кратковременное сообщение в статусбар. */
  flash(msg: string): void;
}

export interface ShellOptions {
  commands: ShellCommands;
  /** Действия форматирования над выделением редактора. */
  format: FormatActions;
  /** Rail + сворачиваемый sidebar. */
  sidebar: Sidebar;
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

  // Формат-группа: кнопки различаются data-action.
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

  // Палитра команд — заглушка (H2).
  byId<HTMLButtonElement>("palette-trigger").addEventListener("click", commands.palette);
  byId<HTMLButtonElement>("rail-logs").addEventListener("click", () => commands.flash("Журнал — скоро"));

  // Rail ↔ sidebar: повторный клик по активной панели сворачивает sidebar.
  byId<HTMLButtonElement>("rail-explorer").addEventListener("click", () => opts.sidebar.toggle("explorer"));
  byId<HTMLButtonElement>("rail-plugins").addEventListener("click", () => opts.sidebar.toggle("plugins"));

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
    // Dev-only: DevTools. WebView2 по настройке глушит свои акселераторы
    // DevTools (F12 / Ctrl+Shift+C) — см. disable_browser_accelerator_keys в Rust,
    // поэтому даём явный вход на стандартном Chromium-хоткее Ctrl+Shift+I.
    // В prod-сборке ветка вырезается (`import.meta.env.DEV`).
    else if (import.meta.env.DEV && k === "i" && e.shiftKey) {
      e.preventDefault();
      void openDevtools().catch((err) => console.error("open_devtools:", err));
    }
    else if (k === "i") { e.preventDefault(); commands.toggleInspector(); }
    else if (k === "b") { e.preventDefault(); opts.format.bold(); }
    else if (k === "k") { e.preventDefault(); commands.palette(); }
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
