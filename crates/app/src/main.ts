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
import { createPluginViews } from "../plugin/pluginViews";
import { createPluginManager } from "../plugin/pluginManager";
import { createPluginStatusBar } from "../plugin/pluginStatusBar";
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

// Дровер плагинов (вариант C): постоянный DOM, модальность — в drawer.ts.
// Создаётся до менеджера/статусбара: они монтируются в drawer.content.
const drawer = createDrawer({
  root: document.getElementById("drawer") as HTMLElement,
  trigger: document.getElementById("tb-plugins") as HTMLElement,
  closeButton: document.getElementById("drawer-close") as HTMLButtonElement,
  backdrop: document.getElementById("backdrop") as HTMLElement,
  title: "Плагины",
});
const gutter = createGutter(editor, gutterEl);
const format = createFormatActions(editor);

// Нижний док (S11, ADR-0029/0030): режимы «Журнал | Мониторинг | AI-ассистент».
// Создаётся до оболочки: shell.ts читает state() и вызывает collapse()/hide() (Esc).
// `monitor` объявлен заранее: onLayoutChange дёргает его догон при открытии
// вкладки «Мониторинг» (сам монитор создаётся ниже, после логгера).
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

// Вкладка «Мониторинг» (ADR-0031): подписка на
// `resource-metrics`/`plugin-process-exited`, рендер строк `#mon-rows` и тихой
// сводки `#mon-summary`. Живёт весь сеанс; запись в журнал — через шов ленты.
monitor = createResourceMonitor({
  root: document.getElementById("mon-rows") as HTMLElement,
  log: (level, category, message, detail) => logPanel.log(level, category, message, detail),
});

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

// AI-ассистент (ADR-0030/0032/0033/0034): тело режима AI дока. Сеть — только
// на Rust-стороне; ответ модели рендерится исключительно textContent. Создаётся
// после стора: текст документа берётся из активной вкладки (шов getDocumentText).
ai = createAiPanel({
  root: document.getElementById("ai-mode") as HTMLElement,
  log: (level, category, message, detail) => logPanel.log(level, category, message, detail),
  getDocumentText: () => store.active()?.text ?? "",
  getActiveDocument: () => {
    const tab = store.active();
    return tab ? { id: tab.id, rev: tab.rev } : null;
  },
});

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

// 4. Плагинные тир-1 view (Фаза 4, IDEA-003): каждый плагин-вью —
//    `HtmlViewProvider` в общем реестре; нативный preview остаётся дефолтным.
//    #plugin-view показывается при выборе плагинной вкладки в `.view-switch`.
let lastPlugins: PluginInfo[] = [];
const pluginViews = createPluginViews({
  registry,
  ctx: makeContext(),
  switchEl: document.getElementById("view-switch") as HTMLElement,
  containerEl: document.getElementById("plugin-view") as HTMLElement,
  previewEl: preview,
  backButtonEl: document.getElementById("btn-back-preview"),
  status: (msg) => status.flash(msg),
  // Появление/пропадание view меняет интерактивность статусбара, а события
  // `plugins-changed`/`plugin-views-changed` приходят независимо.
  onViewsChanged: () => pluginStatus.render(lastPlugins),
});

// Per-plugin элементы статусбара (§11.1 п.5): включённые плагины с правом
// `ui:statusbar`. Клик — открыть представление, «+N» — панель.
const pluginStatus = createPluginStatusBar({
  root: document.getElementById("plugin-status") as HTMLElement,
  hasView: (id) => pluginViews.hasView(id),
  onActivate: (id) => {
    // Панель предпросмотра могла быть скрыта (Ctrl+P): показываем её через чекбокс,
    // чтобы состояние было согласовано, и уже затем открываем представление.
    const chk = document.getElementById("chk-preview") as HTMLInputElement | null;
    if (chk && !chk.checked) chk.click();
    pluginViews.openPlugin(id);
  },
  onShowPanel: () => drawer.open(),
});

// Склонение «с проблемой/с проблемами» в счётчике панели: 1-4 (кроме 11-14) —
// «с проблемой», иначе «с проблемами». UI-строка — русский.
function problemSuffix(n: number): string {
  const mod100 = n % 100;
  const unit = n % 10;
  return unit >= 1 && unit <= 4 && (mod100 < 11 || mod100 > 14)
    ? "с проблемой"
    : "с проблемами";
}

/** Счётчик плагинов в шапке дровера (`#side-count`). Узел может отсутствовать. */
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

  // Триггер дровера в тулбаре: число активных + флаг проблем (ADR-0024).
  // Точка `.pdot` краснеет через `.has-problem`.
  const triggerEl = document.getElementById("tb-plugins");
  const triggerCnt = document.getElementById("tb-plugins-cnt");
  if (triggerEl) triggerEl.classList.toggle("has-problem", problems > 0);
  if (triggerCnt) {
    triggerCnt.textContent = String(
      list.filter((info) => info.status.state === "active").length
    );
  }
}

// Менеджер плагинов (Фаза 5): список/статусы/вкл-выкл/перезагрузка в дровере.
//    Данные — из Rust-хоста (`list_plugins`), изменения приходят событием
//    `plugins-changed`; ошибки IPC глушит контроллер.
const pluginManager = createPluginManager({
  root: drawer.content,
  status: (msg) => status.flash(msg),
  onPlugins: (list) => {
    lastPlugins = list;
    pluginStatus.render(list);
    // Счётчик в шапке дровера владеет композиционный корень, не контроллер.
    updatePluginCount(list);
  },
});

// Сообщения плагинов (`host.show_message`) — в per-plugin элемент статусбара
// (§11.1 п.5). Если элемент скрыт (плагин свёрнут в «+N»), показываем текст в
// #stat-msg, чтобы сообщение не потерялось визуально.
void listen<{ plugin_id: string; text: string }>("plugin-message", (event) => {
  const { plugin_id, text } = event.payload;
  if (!pluginStatus.setMessage(plugin_id, text)) status.flash(`${plugin_id}: ${text}`);
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

// chk-preview переключает ВИДИМОСТЬ вторичной панели (idea5): #preview остаётся
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

/** Иконка типа файла по имени (расширение). */
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

/** Закрытие вкладки с подтверждением при несохранённых изменениях. */
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

/** Создаёт DOM-элемент вкладки с обработчиками. */
function createTabElement(tab: Tab): HTMLElement {
  const el = document.createElement("div");
  el.className = "tab";
  el.dataset.id = tab.id;
  // Стабильный id для aria-controls/aria-labelledby (паттерн tabs, ADR-0037).
  el.id = `tab-${tab.id}`;
  el.setAttribute("role", "tab");
  el.setAttribute("aria-controls", "panes");
  // Доступное имя — имя документа (содержимое .name дублирует его для скринридера).
  el.setAttribute("aria-label", tab.name);
  // Roving tabindex: активная вкладка получает 0 в updateTabElement, остальные -1.
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
  close.tabIndex = -1; // кнопка закрытия вне порядка табуляции (pattern tabs)
  // Крестик текстовым глифом × (U+00D7): CSS .tab .close стилизует содержимое
  // как текст (font-size: 14px, центрирование inline-flex), поэтому SVG не нужен.
  close.textContent = "×";
  el.appendChild(close);

  el.addEventListener("click", (e) => {
    if (e.target === close) return; // клик по кнопке закрытия обрабатывается отдельно
    store.switchTab(tab.id);
    el.focus(); // паттерн tabs требует фокус на активной вкладке
  });
  close.addEventListener("click", (e) => {
    e.stopPropagation();
    void closeTabWithConfirm(tab.id);
  });

  return el;
}

/** Обновляет содержимое существующего элемента вкладки (имя, dirty, активность). */
function updateTabElement(el: HTMLElement, tab: Tab, activeId: DocumentId | null): void {
  const isActive = tab.id === activeId;
  el.setAttribute("aria-selected", String(isActive));
  el.tabIndex = isActive ? 0 : -1; // roving tabindex: в таб-порядке только активная

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

/**
 * Рендерит полосу вкладок с диффингом: не пересоздаёт DOM целиком при каждом
 * изменении текста или dirty-флага.
 */
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

  // Удаляем элементы закрытых вкладок
  for (const [id, el] of existing) {
    if (!seen.has(id)) el.remove();
  }

  syncTabPanel(activeId);
}

/**
 * Связывает панель с активной вкладкой через aria-labelledby (паттерн tabs):
 * скринридер объявляет панель именем активной вкладки при её появлении.
 */
function syncTabPanel(activeId: DocumentId | null): void {
  if (activeId) {
    panesEl.setAttribute("aria-labelledby", `tab-${activeId}`);
  } else {
    panesEl.removeAttribute("aria-labelledby");
  }
}

/**
 * Клавиатурная навигация по вкладкам (WAI-ARIA tabs, автоматическая активация
 * при фокусе): стрелки/Home/End — перемещение фокуса с активацией;
 * Delete/Ctrl+W — закрытие активной вкладки. Слушатель на контейнере: вкладки
 * создаются/удаляются динамически, делегирование переживает их пересоздание.
 */
function onTabStripKeyDown(e: KeyboardEvent): void {
  const tabs = Array.from(tabStrip.querySelectorAll<HTMLElement>(".tab"));
  if (tabs.length === 0) return;
  const current = document.activeElement as HTMLElement | null;
  // Фокус может быть на кнопке закрытия внутри вкладки — поднимаемся до .tab.
  const currentTab = (current?.closest(".tab") ?? null) as HTMLElement | null;
  if (!currentTab) return;
  const currentIndex = tabs.indexOf(currentTab);
  if (currentIndex < 0) return;

  // Закрытие: Delete или Ctrl+W. Глобального обработчика Ctrl+W в shell.ts нет,
  // поэтому конфликта с другими хоткеями нет. e.code — физическая клавиша.
  const wantsClose =
    e.key === "Delete" ||
    ((e.ctrlKey || e.metaKey) && e.code === "KeyW");
  if (wantsClose) {
    e.preventDefault();
    const id = currentTab.dataset.id;
    if (!id) return;
    void closeTabWithConfirm(asDocumentId(id)).then(() => {
      // После закрытия возвращаем фокус на новую активную вкладку.
      const activeId = store.activeId();
      if (activeId) {
        const next = tabStrip.querySelector<HTMLElement>(`.tab[data-id="${activeId}"]`);
        next?.focus();
      }
    });
    return;
  }

  // Навигация: стрелки по кругу, Home/End — крайние вкладки.
  let targetIndex = currentIndex;
  if (e.key === "ArrowLeft") targetIndex = (currentIndex - 1 + tabs.length) % tabs.length;
  else if (e.key === "ArrowRight") targetIndex = (currentIndex + 1) % tabs.length;
  else if (e.key === "Home") targetIndex = 0;
  else if (e.key === "End") targetIndex = tabs.length - 1;
  else return; // прочие клавиши (Enter/Space) — активация уже произошла на фокусе

  e.preventDefault();
  const target = tabs[targetIndex];
  const id = target.dataset.id;
  if (id) store.switchTab(asDocumentId(id)); // активация при перемещении фокуса
  target.focus();
}

tabStrip.addEventListener("keydown", onTabStripKeyDown);

let prevText = "";
store.subscribe((tabs, activeId) => {
  const active = tabs.find((t) => t.id === activeId) ?? null;
  if (active && active.text !== prevText) {
    prevText = active.text;
    previewView.invalidate(); // индекс устарел до нового рендера
  }
  gutter.update();
  renderTabStrip(tabs, activeId);
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
//    Ошибка провайдера уходит в тост.
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
    isDirty: () => store.active()?.dirty ?? false,
    palette: () => {
      // Взаимоисключение модальных слоёв: открытие палитры закрывает дровер
      // (обратное не требуется — триггер палитры инертен, пока дровер открыт).
      drawer.close();
      palette.toggle();
      document
        .getElementById("palette-trigger")
        ?.setAttribute("aria-expanded", String(palette.isOpen()));
    },
    reloadPlugins: () =>
      void pluginManager
        .reloadForDocument(store.active()?.path ?? null)
        .catch((e) => status.flash(`Плагины: ${errorMessage(e)}`)),
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

// 8.5. Тема (ADR-0037): тумблер в статусбаре (#sb-theme), data-theme на <html>,
//      выбор сохраняется между сессиями. Атрибут выставляется до первого
//      рендера внешним скриптом theme-init.js (CSP запрещает inline).
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
    // localStorage недоступен (приватный режим) — тема живёт до перезагрузки.
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
  // theme-color: нативная панель WebView следует за активной темой.
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", theme === "light" ? "#fafbfc" : "#0d0e12");
}

applyTheme(currentTheme());
const themeBtn = document.getElementById("sb-theme");
themeBtn?.addEventListener("click", () => {
  applyTheme(currentTheme() === "light" ? "dark" : "light");
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
  // на стартовое состояние предпросмотра.
  await pluginViews.refresh();
  // Менеджер плагинов — после view: панель может быть скрыта, DOM всё равно готов.
  await pluginManager.refresh();
  editorView.focus();
}

void bootstrap();
