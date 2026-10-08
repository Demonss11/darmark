// darmark — лёгкий редактор/вьюер Markdown в духе Notepad++.
//
// Композиционный корень (Фаза 6): собирает домен (docStore, RenderIndex,
// реестр view, панели) и передаёт его оболочке. Сам корень логики не содержит.
//
// Поток данных: редактор (тир-2 view) → docStore (проекция Rust-стора, D5) →
// RenderIndex → previewView (тир-1 view, единственное `preview.innerHTML`).
// Панели — дерево layout (MAX_PANES = 2). Ядро Markdown — `md-core` (Rust).

import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { createDocStore } from "./docStore";
import { createRenderIndex } from "./renderIndex";
import { createViewRegistry, type ViewContext } from "./viewRegistry";
import { asPaneId, asViewId, newPaneId, newViewId } from "./ids";
import { createEditorView } from "./editorView";
import { previewViewProvider, type PreviewView, type PreviewViewOptions } from "./previewView";
import { createLinkController } from "./linkController";
import { createPaneHost } from "./paneHost";
import { applyLayout, LayoutError, type LayoutNode, type Pane } from "./layout";
import { createStatusBar } from "./statusBar";
import { createPluginViews } from "./pluginViews";
import { createPluginManager } from "./pluginManager";
import { createFileActions } from "./fileActions";
import { createShell } from "./shell";
import { createSidebar } from "./sidebar";
import { createGutter } from "./gutter";
import { createFormatActions } from "./formatActions";
import { START_TEXT } from "./sampleDocument";
import { createPalette, type CommandProvider, type PaletteCommand } from "./palette";
import { toast } from "./toast";
import { errorMessage, listPlugins, runPluginCommand } from "./tauri";
import type { RenderResult } from "./tauri";
import "./style.css";

const APP_NAME = "darmark";

const editor = document.getElementById("editor") as HTMLTextAreaElement;
const preview = document.getElementById("preview") as HTMLElement;
const panesEl = document.getElementById("panes") as HTMLElement;
const statInspect = document.getElementById("stat-inspect") as HTMLElement;
const btnInspect = document.getElementById("btn-inspect") as HTMLButtonElement;
const gutterEl = document.getElementById("gutter") as HTMLElement;
const sidebarEl = document.getElementById("sidebar") as HTMLElement;
const tabName = document.getElementById("tab-name") as HTMLElement;
const tabDirty = document.getElementById("tab-dirty") as HTMLElement;

/** Имя файла из пути (для таба-заглушки). */
function baseName(p: string): string {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i >= 0 ? p.slice(i + 1) : p;
}

// Rail + сворачиваемый sidebar (explorer по умолчанию, plugins — заглушка H2).
const sidebar = createSidebar({
  sidebar: sidebarEl,
  panels: {
    explorer: document.getElementById("panel-explorer") as HTMLElement,
    plugins: document.getElementById("panel-plugins") as HTMLElement,
  },
  rail: {
    explorer: document.getElementById("rail-explorer") as HTMLElement,
    plugins: document.getElementById("rail-plugins") as HTMLElement,
  },
});
const gutter = createGutter(editor, gutterEl);
const format = createFormatActions(editor);

// 1. Общий индекс рендера + связка inspector/scrollsync.
const renderIndex = createRenderIndex();
const link = createLinkController({ editor, preview, index: renderIndex, statusEl: statInspect });
const inspector = link.inspector;

// 2. Стор документов. Колбэки ссылаются на status/previewView, объявленные ниже:
//    они вызываются позже, поэтому такой порядок безопасен.
const store = createDocStore({
  renderMapped: () => inspector.isActive(),
  onRender: (res: RenderResult, source: string) => previewView.applyRender(res, source),
  onStatus: (msg) => status.flash(msg),
});

const status = createStatusBar({ store, editor, appName: APP_NAME });

// 3. Реестр представлений: тир-1 preview встроен на хосте (D2).
const registry = createViewRegistry();
registry.registerHtml(previewViewProvider);

function makeContext(): ViewContext {
  return {
    paneId: newPaneId(),
    viewId: newViewId("preview"),
    document: () => store.snapshot(),
    edit: (text) => store.setText(text),
    render: (text, mapped) => store.renderText(text, mapped),
    status: (msg) => status.flash(msg),
    openExternal: (url) => void openUrl(url).catch((e) => status.flash(`Ссылка: ${String(e)}`)),
    onDocument: (cb) => store.onDocument(cb),
  };
}

const previewView = registry.createHtmlView<PreviewView, PreviewViewOptions>(
  "preview",
  makeContext(),
  {
    previewEl: preview,
    index: renderIndex,
    onIndexed: () => link.onIndexChanged(),
  }
);

// 4. Плагинные тир-1 view (Фаза 4, IDEA-003): каждый плагин-вью — `HtmlViewProvider`
//    в общем реестре; нативный preview остаётся дефолтным. Контейнер `#plugin-view`
//    показывается при выборе плагинной вкладки в `.view-switch`.
const pluginViews = createPluginViews({
  registry,
  ctx: makeContext(),
  switchEl: document.getElementById("view-switch") as HTMLElement,
  containerEl: document.getElementById("plugin-view") as HTMLElement,
  previewEl: preview,
  status: (msg) => status.flash(msg),
});

// Менеджер плагинов (Фаза 5): список/статусы/вкл-выкл/перезагрузка в панели
//    `#panel-plugins`. Данные — из Rust-хоста (`list_plugins`), изменения приходят
//    событием `plugins-changed`. Ошибка IPC глушится в контроллере.
const pluginManager = createPluginManager({
  root: document.getElementById("plugin-manager") as HTMLElement,
  status: (msg) => status.flash(msg),
});

// Сообщения плагинов (`host.show_message`) — в статусбар с указанием источника.
void listen<{ plugin_id: string; text: string }>("plugin-message", (event) => {
  status.flash(`${event.payload.plugin_id}: ${event.payload.text}`);
}).catch(() => null);

// Внешняя правка документа плагином: событие несёт лишь id/rev, текст тянем
// снапшотом (pull), чтобы не гонять большой документ через событие.
void listen<{ doc_id: string; rev: number }>("document-updated", (event) => {
  store.applyHostUpdate(event.payload.doc_id, event.payload.rev);
}).catch(() => null);

// 4. Панели: фиксированные две (редактор + предпросмотр), MAX_PANES = 2.
const EDITOR_PANE = asPaneId("pane-editor");
const PREVIEW_PANE = asPaneId("pane-preview");
const EDITOR_VIEW = asViewId("editor");
const PREVIEW_VIEW = asViewId("preview");
const editorPane: Pane = { kind: "pane", id: EDITOR_PANE, views: [EDITOR_VIEW], active: EDITOR_VIEW };
const previewPane: Pane = { kind: "pane", id: PREVIEW_PANE, views: [PREVIEW_VIEW], active: PREVIEW_VIEW };

let layout: LayoutNode = {
  kind: "split",
  id: "root",
  dir: "row",
  children: [editorPane, previewPane],
};
const paneHost = createPaneHost(panesEl);
paneHost.mount(layout);

// chk-preview переключает ВИДИМОСТЬ вторичной панели (idea5): `#preview` остаётся
// в DOM, поэтому скрытие/показ не требует пересоздания вида (AC-9).
function setPreviewVisible(on: boolean): void {
  const next = on
    ? applyLayout(layout, { t: "splitPane", target: EDITOR_PANE, dir: "row", pane: previewPane })
    : applyLayout(layout, { t: "closePane", pane: PREVIEW_PANE });
  if (!(next instanceof LayoutError)) layout = next;
  paneHost.mount(layout);
}

// 5. Редактор (тир-2 view) + реакция на проекцию стора.
const editorView = createEditorView(editor, store, () => {
  status.updateStatus();
  inspector.onEditorActivity();
});

let prevText = "";
store.subscribe((s) => {
  if (s.text !== prevText) {
    prevText = s.text;
    previewView.invalidate(); // индекс устарел до нового рендера
  }
  gutter.update(); // номера строк следуют за текстом редактора
  tabName.textContent = s.path ? baseName(s.path) : "безымянный";
  tabDirty.hidden = !s.dirty;
  status.updateTitle();
  status.updateStatus();
});

// 6. Инспектор и файловые команды.
function toggleInspector(): void {
  if (inspector.isActive()) inspector.disable();
  else inspector.enable();
  btnInspect.classList.toggle("active", inspector.isActive());
  previewView.setMapped(inspector.isActive()); // смена mapped меняет HTML
  store.reload();
}

const files = createFileActions({
  store,
  status: (m) => status.flash(m),
  onPathChanged: () => previewView.refreshImages(),
  focusEditor: () => editorView.focus(),
});

// 7. Палитра команд (Ctrl+K): core-команды + провайдер плагинных команд.
//    Палитра построена на dialog.ts; ошибка провайдера уходит в тост.
const coreCommands: PaletteCommand[] = [
  { id: "file.new", title: "Новый файл", group: "Файл", origin: "core", hotkey: "Ctrl+N", run: () => void files.newFile() },
  { id: "file.open", title: "Открыть файл", group: "Файл", origin: "core", hotkey: "Ctrl+O", run: () => void files.openFile() },
  { id: "file.save", title: "Сохранить", group: "Файл", origin: "core", hotkey: "Ctrl+S", run: () => void files.saveFile() },
  { id: "file.save-as", title: "Сохранить как", group: "Файл", origin: "core", hotkey: "Ctrl+Shift+S", run: () => void files.saveAs() },
  { id: "view.toggle-preview", title: "Показать/скрыть предпросмотр", group: "Вид", origin: "core", hotkey: "Ctrl+P", run: () => (document.getElementById("chk-preview") as HTMLInputElement).click() },
  { id: "view.toggle-sync", title: "Синхронная прокрутка", group: "Вид", origin: "core", run: () => (document.getElementById("chk-sync") as HTMLInputElement).click() },
  { id: "view.toggle-inspector", title: "Инспектор", group: "Вид", origin: "core", hotkey: "Ctrl+I", run: () => toggleInspector() },
  { id: "format.bold", title: "Жирный", group: "Формат", origin: "core", hotkey: "Ctrl+B", run: () => format.bold() },
  { id: "format.italic", title: "Курсив", group: "Формат", origin: "core", run: () => format.italic() },
  { id: "format.code", title: "Код", group: "Формат", origin: "core", run: () => format.code() },
  { id: "format.heading", title: "Заголовок", group: "Формат", origin: "core", run: () => format.heading() },
  { id: "format.link", title: "Ссылка", group: "Формат", origin: "core", hotkey: "Ctrl+Shift+K", run: () => format.link() },
];

// Плагинные команды: только у включённых плагинов; origin = `plugin:<id>`.
const pluginCommandProvider: CommandProvider = {
  async list(): Promise<PaletteCommand[]> {
    const plugins = await listPlugins();
    const commands: PaletteCommand[] = [];
    for (const plugin of plugins) {
      if (!plugin.enabled) continue;
      for (const command of plugin.commands ?? []) {
        commands.push({
          id: `plugin:${plugin.id}:${command.id}`,
          title: command.title,
          group: "Плагины",
          origin: `plugin:${plugin.id}`,
          ...(command.keybinding ? { hotkey: command.keybinding } : {}),
          run: () => runPluginCommand(command.id),
        });
      }
    }
    return commands;
  },
};

const palette = createPalette({
  providers: [{ list: () => coreCommands }, pluginCommandProvider],
  onError: (message) => toast(message, { kind: "error" }),
});

// 8. Оболочка: тулбар, хоткеи, тумблеры, фокус панелей, закрытие окна.
createShell({
  commands: {
    newFile: () => void files.newFile(),
    openFile: () => void files.openFile(),
    saveFile: () => void files.saveFile(),
    saveAs: () => void files.saveAs(),
    toggleInspector,
    setPreviewVisible,
    setSyncEnabled: (on) => link.setSyncEnabled(on),
    setActivePane: (p) => paneHost.setActive(p),
    isDirty: () => store.state().dirty,
    palette: () => {
      palette.toggle();
      // a11y: отражаем состояние палитры на её триггере.
      document
        .getElementById("palette-trigger")
        ?.setAttribute("aria-expanded", String(palette.isOpen()));
    },
    flash: (m) => status.flash(m),
  },
  format,
  sidebar,
  inspectorActive: () => inspector.isActive(),
  editorPane: EDITOR_PANE,
  previewPane: PREVIEW_PANE,
  editor,
  preview,
});

// 9. Стартовый документ (создаётся в Rust-сторе: id/путь/текст ведёт хост).
async function bootstrap(): Promise<void> {
  try {
    await store.newDocument(START_TEXT);
  } catch (e) {
    status.flash(`Запуск: ${errorMessage(e)}`);
  }
  const chkPreview = document.getElementById("chk-preview") as HTMLInputElement;
  setPreviewVisible(chkPreview.checked);
  status.updateStatus();
  status.updateTitle();
  // Плагинные view — после документа и тумблеров, чтобы их вкладки не влияли
  // на стартовое состояние предпросмотра (ошибка IPC глушится в контроллере).
  await pluginViews.refresh();
  // Менеджер плагинов — после view: панель может быть скрыта, DOM всё равно готов.
  await pluginManager.refresh();
  editorView.focus();
}

void bootstrap();
