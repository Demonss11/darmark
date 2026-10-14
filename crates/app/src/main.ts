// darmark — лёгкий редактор/вьюер Markdown в духе Notepad++.
//
// Композиционный корень: собирает домен (docStore, RenderIndex, реестр view,
// панели) и передаёт его оболочке. Поток данных: редактор (тир-2 view) → docStore
// (проекция Rust-стора, D5) → RenderIndex → previewView (тир-1 view, единственное
// `preview.innerHTML`). Панели — дерево layout (MAX_PANES = 2), ядро Markdown —
// `md-core` (Rust).

import { openUrl } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import { createDocStore } from "./doc/docStore";
import { createRenderIndex } from "./doc/renderIndex";
import { createViewRegistry, type ViewContext } from "./viewRegistry";
import { asPaneId, asViewId, newPaneId, newViewId } from "./core/ids";
import { createEditorView } from "./doc/editorView";
import { createTabStrip } from "./editor/tabStrip";
import { previewViewProvider, type PreviewView, type PreviewViewOptions } from "./previewView";
import { createLinkController } from "./linkController";
import { createPaneHost } from "./layout/paneHost";
import { applyLayout, LayoutError, type LayoutNode, type Pane } from "./layout/layout";
import { createStatusBar } from "./statusBar";
import { createPluginViews } from "./plugin/pluginViews";
import { createPluginManager } from "./plugin/pluginManager";
import { createPluginStatusBar } from "./plugin/pluginStatusBar";
import { createFileActions } from "./fileActions";
import { createShell } from "./layout/shell";
import { createDrawer } from "./ui/drawer";
import { initTheme } from "./ui/theme";
import { createGutter } from "./ui/gutter";
import { createFormatActions } from "./formatActions";
import { createLogPanel } from "./panels/logPanel";
import { createResourceMonitor, type ResourceMonitor } from "./panels/resourceMonitor";
import { createAiPanel, type AiPanel } from "./panels/aiPanel";
import { START_TEXT } from "./sampleDocument";
import { createPalette, type CommandProvider, type PaletteCommand } from "./ui/palette";
import { toast } from "./ui/toast";
import { errorMessage, listPlugins, runPluginCommand } from "./core/tauri";
import type { PluginInfo, RenderResult } from "./core/tauri";
import "./style.css";

const APP_NAME = "ПЕНАЛ";

const editor = document.getElementById("editor") as HTMLTextAreaElement;
const preview = document.getElementById("preview") as HTMLElement;
const panesEl = document.getElementById("panes") as HTMLElement;
const statInspect = document.getElementById("stat-inspect") as HTMLElement;
const btnInspect = document.getElementById("btn-inspect") as HTMLButtonElement;
const gutterEl = document.getElementById("gutter") as HTMLElement;
const tabStrip = document.getElementById("tab-strip") as HTMLElement;

const drawer = createDrawer({
  root: document.getElementById("drawer") as HTMLElement,
  trigger: document.getElementById("tb-plugins") as HTMLElement,
  closeButton: document.getElementById("drawer-close") as HTMLButtonElement,
  backdrop: document.getElementById("backdrop") as HTMLElement,
  title: "Плагины",
});
const gutter = createGutter(editor, gutterEl);
const format = createFormatActions(editor);

let monitor: ResourceMonitor | null = null;
let ai: AiPanel | null = null;
const logPanel = createLogPanel({
  trigger: document.getElementById("sb-logs") as HTMLButtonElement,
  monTrigger: document.getElementById("sb-mon") as HTMLButtonElement,
  aiTrigger: document.getElementById("sb-ai") as HTMLButtonElement,
  aiToolbarTrigger: document.getElementById("tb-ai") as HTMLButtonElement,
  badge: document.getElementById("log-badge") as HTMLElement,
  onLayoutChange: () => monitor?.refresh(),
  onModeChange: (mode, state) => ai?.onDockState(mode, state),
});

monitor = createResourceMonitor({
  root: document.getElementById("mon-rows") as HTMLElement,
  log: (level, category, message, detail) => logPanel.log(level, category, message, detail),
});

const renderIndex = createRenderIndex();
const link = createLinkController({ editor, preview, index: renderIndex, statusEl: statInspect });
const inspector = link.inspector;

const store = createDocStore({
  renderMapped: () => inspector.isActive(),
  onRender: (res: RenderResult, source: string) => previewView.applyRender(res, source),
  onStatus: (msg) => status.flash(msg),
});

const status = createStatusBar({ store, editor, appName: APP_NAME });

ai = createAiPanel({
  root: document.getElementById("ai-mode") as HTMLElement,
  log: (level, category, message, detail) => logPanel.log(level, category, message, detail),
  getDocumentText: () => store.active()?.text ?? "",
  getActiveDocument: () => {
    const tab = store.active();
    return tab ? { id: tab.id, rev: tab.rev } : null;
  },
});

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

let lastPlugins: PluginInfo[] = [];
const pluginViews = createPluginViews({
  registry,
  ctx: makeContext(),
  switchEl: document.getElementById("view-switch") as HTMLElement,
  containerEl: document.getElementById("plugin-view") as HTMLElement,
  previewEl: preview,
  backButtonEl: document.getElementById("btn-back-preview"),
  status: (msg: string) => status.flash(msg),
  onViewsChanged: () => pluginStatus.render(lastPlugins),
});

const pluginStatus = createPluginStatusBar({
  root: document.getElementById("plugin-status") as HTMLElement,
  hasView: (id: string) => pluginViews.hasView(id),
  onActivate: (id: string) => {
    const chk = document.getElementById("chk-preview") as HTMLInputElement | null;
    if (chk && !chk.checked) chk.click();
    pluginViews.openPlugin(id);
  },
  onShowPanel: () => drawer.open(),
});

function problemSuffix(n: number): string {
  const mod100 = n % 100;
  const unit = n % 10;
  return unit >= 1 && unit <= 4 && (mod100 < 11 || mod100 > 14)
    ? "с проблемой"
    : "с проблемами";
}

function updatePluginCount(list: PluginInfo[]): void {
  const problems = list.filter(
    (info) => info.status.state === "failed" || info.status.state === "quarantined"
  ).length;

  const countEl = document.getElementById("side-count");
  if (countEl) {
    if (problems > 0) {
      countEl.textContent = `${problems} ${problemSuffix(problems)}`;
      countEl.classList.add("warn");
    } else {
      countEl.textContent = String(list.length);
      countEl.classList.remove("warn");
    }
  }

  const triggerEl = document.getElementById("tb-plugins");
  const triggerCnt = document.getElementById("tb-plugins-cnt");
  if (triggerEl) triggerEl.classList.toggle("has-problem", problems > 0);
  if (triggerCnt) {
    triggerCnt.textContent = String(
      list.filter((info) => info.status.state === "active").length
    );
  }
}

const pluginManager = createPluginManager({
  root: drawer.content,
  status: (msg: string) => status.flash(msg),
  onPlugins: (list: PluginInfo[]) => {
    lastPlugins = list;
    pluginStatus.render(list);
    updatePluginCount(list);
  },
});

void listen<{ plugin_id: string; text: string }>("plugin-message", (event) => {
  const { plugin_id, text } = event.payload;
  if (!pluginStatus.setMessage(plugin_id, text)) status.flash(`${plugin_id}: ${text}`);
}).catch(() => null);

void listen<{ doc_id: string; rev: number }>("document-updated", (event) => {
  store.applyHostUpdate(event.payload.doc_id, event.payload.rev);
}).catch(() => null);

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

function setPreviewVisible(on: boolean): void {
  const next = on
    ? applyLayout(layout, { t: "splitPane", target: EDITOR_PANE, dir: "row", pane: previewPane })
    : applyLayout(layout, { t: "closePane", pane: PREVIEW_PANE });
  if (!(next instanceof LayoutError)) layout = next;
  paneHost.mount(layout);
}

const editorView = createEditorView(editor, store, () => {
  status.updateStatus();
  inspector.onEditorActivity();
});

const tabStripView = createTabStrip({ strip: tabStrip, panes: panesEl, store });

let prevText = "";
store.subscribe((tabs, activeId) => {
  const active = tabs.find((t) => t.id === activeId) ?? null;
  if (active && active.text !== prevText) {
    prevText = active.text;
    previewView.invalidate();
  }
  gutter.update();
  tabStripView.render(tabs, activeId);
  status.updateTitle();
  status.updateStatus();
});

function toggleInspector(): void {
  if (inspector.isActive()) inspector.disable();
  else inspector.enable();
  btnInspect.classList.toggle("active", inspector.isActive());
  previewView.setMapped(inspector.isActive());
  store.reload();
}

const files = createFileActions({
  store,
  status: (m) => status.flash(m),
  onPathChanged: () => previewView.refreshImages(),
  focusEditor: () => editorView.focus(),
});

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
    isDirty: () => store.active()?.dirty ?? false,
    palette: () => {
      drawer.close();
      palette.toggle();
      document
        .getElementById("palette-trigger")
        ?.setAttribute("aria-expanded", String(palette.isOpen()));
    },
    reloadPlugins: () =>
      void pluginManager
        .reloadForDocument(store.active()?.path ?? null)
        .catch((e: unknown) => status.flash(`Плагины: ${errorMessage(e)}`)),
    flash: (m) => status.flash(m),
  },
  format,
  drawer,
  logPanel,
  inspectorActive: () => inspector.isActive(),
  editorPane: EDITOR_PANE,
  previewPane: PREVIEW_PANE,
  editor,
  preview,
});

initTheme();

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
  await pluginViews.refresh();
  await pluginManager.refresh();
  editorView.focus();
}

void bootstrap();
