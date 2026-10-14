// darmark — лёгкий редактор/вьюер Markdown в духе Notepad++.
//
// Композиционный корень: собирает домен (docStore, RenderIndex, реестр view,
// панели) и передаёт его оболочке. Поток данных: редактор (тир-2 view) → docStore
// (проекция Rust-стора, D5) → RenderIndex → previewView (тир-1 view, единственное
// `preview.innerHTML`). Панели — дерево layout (MAX_PANES = 2), ядро Markdown —
// `md-core` (Rust).

import { openUrl } from "@tauri-apps/plugin-opener";
import { confirm } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { createDocStore, type Tab } from "./doc/docStore";
import { createRenderIndex } from "./doc/renderIndex";
import { createViewRegistry, type ViewContext } from "./viewRegistry";
import { asDocumentId, asPaneId, asViewId, newPaneId, newViewId, type DocumentId } from "./core/ids";
import { createEditorView } from "./doc/editorView";
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

function fileIcon(name: string): string {
  const dot = name.lastIndexOf(".");
  const ext = dot >= 0 ? name.slice(dot + 1).toLowerCase() : "";
  switch (ext) {
    case "md":
    case "markdown":
    case "mdown":
    case "mkd":
      return "M↓";
    case "txt":
      return "T";
    default:
      return "•";
  }
}

async function closeTabWithConfirm(id: DocumentId): Promise<void> {
  const tab = store.tabs().find((t) => t.id === id);
  if (!tab) return;
  if (tab.dirty) {
    const ok = await confirm(`«${tab.name}» — несохранённые изменения будут потеряны. Закрыть?`, {
      title: "ПЕНАЛ",
      kind: "warning",
    });
    if (!ok) return;
  }
  await store.closeTab(id);
}

function createTabElement(tab: Tab): HTMLElement {
  const el = document.createElement("div");
  el.className = "tab";
  el.dataset.id = tab.id;
  el.id = `tab-${tab.id}`;
  el.setAttribute("role", "tab");
  el.setAttribute("aria-controls", "panes");
  el.setAttribute("aria-label", tab.name);
  el.tabIndex = -1;

  const ficon = document.createElement("span");
  ficon.className = "ficon";
  ficon.setAttribute("aria-hidden", "true");
  el.appendChild(ficon);

  const name = document.createElement("span");
  name.className = "name";
  el.appendChild(name);

  const close = document.createElement("button");
  close.className = "close";
  close.type = "button";
  close.title = "Закрыть";
  close.tabIndex = -1;
  close.textContent = "×";
  el.appendChild(close);

  el.addEventListener("click", (e) => {
    if (e.target === close) return;
    store.switchTab(tab.id);
    el.focus();
  });
  close.addEventListener("click", (e) => {
    e.stopPropagation();
    void closeTabWithConfirm(tab.id);
  });

  return el;
}

function updateTabElement(el: HTMLElement, tab: Tab, activeId: DocumentId | null): void {
  const isActive = tab.id === activeId;
  el.setAttribute("aria-selected", String(isActive));
  el.tabIndex = isActive ? 0 : -1;

  const ficon = el.querySelector(".ficon");
  if (ficon) ficon.textContent = fileIcon(tab.name);

  const name = el.querySelector(".name");
  if (name) name.textContent = tab.name;

  let dirty = el.querySelector(".dirty");
  if (tab.dirty && !dirty) {
    dirty = document.createElement("span");
    dirty.className = "dirty";
    dirty.setAttribute("aria-hidden", "true");
    const close = el.querySelector(".close");
    if (close) el.insertBefore(dirty, close);
    else el.appendChild(dirty);
  } else if (!tab.dirty && dirty) {
    dirty.remove();
  }

  const close = el.querySelector(".close") as HTMLElement | null;
  if (close) close.setAttribute("aria-label", `Закрыть ${tab.name}`);
}

function renderTabStrip(tabs: Tab[], activeId: DocumentId | null): void {
  const existing = new Map<string, HTMLElement>();
  for (const el of Array.from(tabStrip.children) as HTMLElement[]) {
    const id = el.dataset.id;
    if (id) existing.set(id, el);
  }

  const seen = new Set<string>();
  for (const tab of tabs) {
    seen.add(tab.id);
    let el = existing.get(tab.id);
    if (!el) {
      el = createTabElement(tab);
      tabStrip.appendChild(el);
    }
    updateTabElement(el, tab, activeId);
  }

  for (const [id, el] of existing) {
    if (!seen.has(id)) el.remove();
  }

  syncTabPanel(activeId);
}

function syncTabPanel(activeId: DocumentId | null): void {
  if (activeId) {
    panesEl.setAttribute("aria-labelledby", `tab-${activeId}`);
  } else {
    panesEl.removeAttribute("aria-labelledby");
  }
}

function onTabStripKeyDown(e: KeyboardEvent): void {
  const tabs = Array.from(tabStrip.querySelectorAll<HTMLElement>(".tab"));
  if (tabs.length === 0) return;
  const current = document.activeElement as HTMLElement | null;
  const currentTab = (current?.closest(".tab") ?? null) as HTMLElement | null;
  if (!currentTab) return;
  const currentIndex = tabs.indexOf(currentTab);
  if (currentIndex < 0) return;

  const wantsClose =
    e.key === "Delete" ||
    ((e.ctrlKey || e.metaKey) && e.code === "KeyW");
  if (wantsClose) {
    e.preventDefault();
    const id = currentTab.dataset.id;
    if (!id) return;
    void closeTabWithConfirm(asDocumentId(id)).then(() => {
      const activeId = store.activeId();
      if (activeId) {
        const next = tabStrip.querySelector<HTMLElement>(`.tab[data-id="${activeId}"]`);
        next?.focus();
      }
    });
    return;
  }

  let targetIndex = currentIndex;
  if (e.key === "ArrowLeft") targetIndex = (currentIndex - 1 + tabs.length) % tabs.length;
  else if (e.key === "ArrowRight") targetIndex = (currentIndex + 1) % tabs.length;
  else if (e.key === "Home") targetIndex = 0;
  else if (e.key === "End") targetIndex = tabs.length - 1;
  else return;

  e.preventDefault();
  const target = tabs[targetIndex];
  const id = target.dataset.id;
  if (id) store.switchTab(asDocumentId(id));
  target.focus();
}

tabStrip.addEventListener("keydown", onTabStripKeyDown);

let prevText = "";
store.subscribe((tabs, activeId) => {
  const active = tabs.find((t) => t.id === activeId) ?? null;
  if (active && active.text !== prevText) {
    prevText = active.text;
    previewView.invalidate();
  }
  gutter.update();
  renderTabStrip(tabs, activeId);
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

const THEME_STORAGE_KEY = "darmark.theme";
type Theme = "dark" | "light";

function currentTheme(): Theme {
  return document.documentElement.getAttribute("data-theme") === "light" ? "light" : "dark";
}

function applyTheme(theme: Theme): void {
  document.documentElement.setAttribute("data-theme", theme);
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
  }

  const btn = document.getElementById("sb-theme");
  if (btn) {
    const isLight = theme === "light";
    const label = document.getElementById("sb-theme-label");
    const sun = document.getElementById("sb-theme-icon-sun");
    const moon = document.getElementById("sb-theme-icon-moon");
    if (label) label.textContent = isLight ? "Светлая" : "Тёмная";
    if (sun) sun.hidden = !isLight;
    if (moon) moon.hidden = isLight;
    btn.setAttribute("aria-label", isLight ? "Тема: светлая" : "Тема: тёмная");
    btn.setAttribute("aria-pressed", String(isLight));
  }
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", theme === "light" ? "#fafbfc" : "#0d0e12");
}

applyTheme(currentTheme());
const themeBtn = document.getElementById("sb-theme");
themeBtn?.addEventListener("click", () => {
  applyTheme(currentTheme() === "light" ? "dark" : "light");
});

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
