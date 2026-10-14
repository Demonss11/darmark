// shell.ts — привязка оболочки к DOM: кнопки тулбара, горячие клавиши, тумблеры
// предпросмотра/синхронизации, формат-группа, палитра, триггер дровера плагинов,
// журнал-панель, фокус активной панели, закрытие окна. Доменной логики нет — только
// вызовы команд из main.ts.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirm } from "@tauri-apps/plugin-dialog";
import type { PaneId } from "./core/ids";
import type { FormatActions } from "./formatActions";
import type { Drawer } from "./drawer";
import type { LogPanel } from "./logPanel";
import { isModalOpen } from "./dialog";
import { openDevtools } from "./core/tauri";

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
  palette(): void;
  /** Ctrl+R: перезагрузить плагин-владелец открытого `.lua` или все включённые. */
  reloadPlugins(): void;
  flash(msg: string): void;
}

export interface ShellOptions {
  commands: ShellCommands;
  format: FormatActions;
  /** Правый модальный дровер плагинов. */
  drawer: Drawer;
  inspectorActive(): boolean;
  editorPane: PaneId;
  previewPane: PaneId;
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  /** Панель «Журнал» (S11, ADR-0029): сворачивающаяся консоль над статусбаром. */
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
  // Триггеры дока (#sb-logs/#sb-mon) вешает сам logPanel — режимами владеет он;
  // shell отвечает только за прогрессивный Esc (ниже).

  editor.addEventListener("focus", () => commands.setActivePane(opts.editorPane));
  preview.addEventListener("mousedown", () => commands.setActivePane(opts.previewPane));
  // Активная панель следует и за клавиатурным фокусом (Tab): любой сфокусированный
  // элемент внутри панели подсвечивает её, а не только клик/фокус редактора.
  document.addEventListener("focusin", (e) => {
    const pane = (e.target as Element | null)?.closest?.(".pane") as HTMLElement | null;
    const id = pane?.dataset.pane;
    if (id === opts.editorPane) commands.setActivePane(opts.editorPane);
    else if (id === opts.previewPane) commands.setActivePane(opts.previewPane);
  });

  chkPreview.addEventListener("change", () => commands.setPreviewVisible(chkPreview.checked));
  chkSync.addEventListener("change", () => commands.setSyncEnabled(chkSync.checked));

  window.addEventListener("keydown", (e: KeyboardEvent) => {
    // Пока открыт модальный диалог (палитра), хоткеи оболочки не срабатывают:
    // управление (в т.ч. Escape) остаётся за dialog.ts.
    if (isModalOpen()) return;
    // Esc в доке — прогрессивный (ADR-0030 §2.2 №10): развёрнут → свернуть,
    // свёрнут → скрыть. Только при фокусе внутри панели (§4.5 ТЗ), иначе Esc
    // остаётся инспектору/редактору.
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
    // Ctrl+Shift+A — открыть/свернуть режим «AI-ассистент» дока (тот же путь, что
    // триггеры #sb-ai/#tb-ai: режимами владеет logPanel, не shell).
    else if (k === "a" && e.shiftKey) { e.preventDefault(); opts.logPanel.toggleMode("ai"); }
    // Dev-only: WebView2 по настройке глушит свои акселераторы (F12 /
    // Ctrl+Shift+C — см. disable_browser_accelerator_keys в Rust), поэтому
    // даём явный вход на Ctrl+Shift+I. В prod ветка вырезается.
    else if (import.meta.env.DEV && k === "i" && e.shiftKey) {
      e.preventDefault();
      void openDevtools().catch((err) => console.error("open_devtools:", err));
    }
    else if (k === "i") { e.preventDefault(); commands.toggleInspector(); }
    else if (k === "b") { e.preventDefault(); opts.format.bold(); }
    // Ctrl+Shift+K — вставка ссылки; должен проверяться ДО Ctrl+K (палитра).
    else if (k === "k" && e.shiftKey) { e.preventDefault(); opts.format.link(); }
    else if (k === "k") { e.preventDefault(); commands.palette(); }
    // Ctrl+R — перезагрузка плагинов; страницу WebView перезагружать нельзя,
    // поэтому всегда гасим дефолт (в т.ч. для Ctrl+Shift+R — hard reload).
    else if (k === "r" && (e.shiftKey || e.altKey)) { e.preventDefault(); }
    else if (k === "r") { e.preventDefault(); commands.reloadPlugins(); }
  });

  // Закрытие окна: при несохранённых изменениях спрашиваем подтверждение;
  // если согласен — закрываем через destroy() (preventDefault иначе отменит закрытие).
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
