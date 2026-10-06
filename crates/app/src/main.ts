// mdedit — лёгкий редактор/вьюер Markdown в духе Notepad++.
// Композиционный корень (Фаза 4): собирает docStore + реестр представлений.
// Состояние документа ведёт docStore (проекция Rust-стора, D5); редактор —
// тир-2 view (textarea), предпросмотр — тир-1 view (HtmlView, встроен на хосте,
// D2). `preview.innerHTML` и общий RenderIndex живут только в previewView;
// inspector и scrollsync читают один и тот же снимок индекса.

import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { errorMessage } from "./tauri";
import { createDocStore } from "./docStore";
import { createRenderIndex } from "./renderIndex";
import { createViewRegistry, type ViewContext } from "./viewRegistry";
import { newPaneId, newViewId } from "./ids";
import { createEditorView } from "./editorView";
import { previewViewProvider, type PreviewView, type PreviewViewOptions } from "./previewView";
import { createInspector } from "./inspector";
import { createScrollSync } from "./scrollsync";
import "./style.css";

const MD_FILTER = {
  name: "Markdown",
  extensions: ["md", "markdown", "mdown", "mkd", "txt"],
};

const editor = document.getElementById("editor") as HTMLTextAreaElement;
const preview = document.getElementById("preview") as HTMLElement;
const fileLabel = document.getElementById("file-label") as HTMLElement;
const statPos = document.getElementById("stat-pos") as HTMLElement;
const statSize = document.getElementById("stat-size") as HTMLElement;
const statMsg = document.getElementById("stat-msg") as HTMLElement;
const statInspect = document.getElementById("stat-inspect") as HTMLElement;
const btnInspect = document.getElementById("btn-inspect") as HTMLButtonElement;
const chkPreview = document.getElementById("chk-preview") as HTMLInputElement;
const chkSync = document.getElementById("chk-sync") as HTMLInputElement;

// ---------- общий индекс рендера и связанные потребители ----------

const renderIndex = createRenderIndex();

const scrollSync = createScrollSync({ editor, preview, index: renderIndex });

const inspector = createInspector({
  editor,
  preview,
  statusEl: statInspect,
  index: renderIndex,
  // Программный scrollIntoView инспектора не должен тянуть редактор.
  beforeScrollIntoView: () => scrollSync.suspend(),
});

// ---------- статусная строка / заголовок (читают проекцию стора) ----------

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
  void getCurrentWindow().setTitle(`${s.dirty ? "● " : ""}${name} — mdedit`);
  fileLabel.textContent = name + (s.dirty ? " ●" : "");
}

function flash(msg: string): void {
  statMsg.textContent = msg;
  window.setTimeout(() => {
    if (statMsg.textContent === msg) statMsg.textContent = "";
  }, 4000);
}

// ---------- стор, реестр представлений, ViewContext ----------

const store = createDocStore({
  renderMapped: () => inspector.isActive(),
  // Единственная точка применения HTML — тир-1 previewView (Фаза 4).
  onRender: (res, source) => previewView.applyRender(res, source),
  onStatus: flash,
});

const registry = createViewRegistry();
registry.registerHtml(previewViewProvider);

// Фасад для view: только он, никакого произвольного доступа к хосту (§5.4).
function makeContext(): ViewContext {
  return {
    paneId: newPaneId(),
    viewId: newViewId("preview"),
    document: () => store.snapshot(),
    edit: (text) => store.setText(text),
    render: (text, mapped) => store.renderText(text, mapped),
    status: flash,
    openExternal: (url) => void openUrl(url).catch((e) => flash(`Ссылка: ${String(e)}`)),
    onDocument: (cb) => store.onDocument(cb),
  };
}

const previewView = registry.createHtmlView<PreviewView, PreviewViewOptions>("preview", makeContext(), {
  previewEl: preview,
  index: renderIndex,
  // Индекс перестроен/сброшен — оба потребителя перечитывают один снимок.
  onIndexed: () => {
    inspector.onIndexChanged();
    scrollSync.onIndexChanged();
  },
});

function onSelection(): void {
  updateStatus();
  inspector.onEditorActivity(); // переиспользуем единый хук (T-11)
}

const editorView = createEditorView(editor, store, onSelection);

// Реакция на проекцию: заголовок/статус всегда; на смену текста — индекс
// невалиден до нового рендера (подсветка и анкоровые привязки сбрасываются).
let prevText = "";
store.subscribe((s) => {
  if (s.text !== prevText) {
    prevText = s.text;
    previewView.invalidate();
  }
  updateTitle();
  updateStatus();
});

// ---------- команды файла ----------

async function confirmDiscard(): Promise<boolean> {
  if (!store.state().dirty) return true;
  return confirm("Несохранённые изменения будут потеряны. Продолжить?", {
    title: "mdedit",
    kind: "warning",
  });
}

async function newFile(): Promise<void> {
  if (!(await confirmDiscard())) return;
  try {
    await store.newDocument("");
    editorView.focus();
  } catch (e) {
    flash(`Новый документ: ${errorMessage(e)}`);
  }
}

async function openFile(): Promise<void> {
  if (!(await confirmDiscard())) return;
  const selected = await open({
    multiple: false,
    filters: [MD_FILTER],
    defaultPath: store.state().path ?? undefined,
  });
  if (typeof selected !== "string") return; // отмена
  try {
    await store.open(selected);
  } catch (e) {
    flash(`Не открылось: ${errorMessage(e)}`);
  }
}

async function saveFile(): Promise<void> {
  const s = store.state();
  if (!s.id) {
    flash("Документ не создан — сохранение недоступно");
    return;
  }
  if (!s.path) return saveAs();
  try {
    await store.save();
    flash("Сохранено");
  } catch (e) {
    flash(`Не сохранилось: ${errorMessage(e)}`);
  }
}

async function saveAs(): Promise<void> {
  if (!store.state().id) {
    flash("Документ не создан — сохранение недоступно");
    return;
  }
  const selected = await save({
    defaultPath: store.state().path ?? "untitled.md",
    filters: [MD_FILTER],
  });
  if (typeof selected !== "string") return;
  try {
    await store.save(selected);
    // Новый каталог — относительные картинки нужно перерезолвить от него.
    previewView.refreshImages();
    flash("Сохранено");
  } catch (e) {
    flash(`Не сохранилось: ${errorMessage(e)}`);
  }
}

// ---------- тулбар ----------

function toggleInspector(): void {
  if (inspector.isActive()) inspector.disable();
  else inspector.enable();
  btnInspect.classList.toggle("active", inspector.isActive());
  // Смена `mapped` меняет HTML — `render_document` вернёт changed: true.
  previewView.setMapped(inspector.isActive());
  store.reload();
}

document.getElementById("btn-new")!.addEventListener("click", () => void newFile());
document.getElementById("btn-open")!.addEventListener("click", () => void openFile());
document.getElementById("btn-save")!.addEventListener("click", () => void saveFile());
document.getElementById("btn-save-as")!.addEventListener("click", () => void saveAs());
btnInspect.addEventListener("click", toggleInspector);

chkPreview.addEventListener("change", () => {
  preview.style.display = chkPreview.checked ? "" : "none";
});

chkSync.addEventListener("change", () => {
  scrollSync.setEnabled(chkSync.checked);
});

window.addEventListener("keydown", (e: KeyboardEvent) => {
  // Esc выходит из режима инспектора (кроме случая открытого меню фильтра).
  if (e.key === "Escape" && inspector.isActive()) {
    if (!document.querySelector(".col-filter-menu")) {
      e.preventDefault();
      toggleInspector();
    }
    return;
  }
  if (!(e.ctrlKey || e.metaKey)) return;
  const k = e.key.toLowerCase();
  if (k === "n" && !e.shiftKey) { e.preventDefault(); void newFile(); }
  else if (k === "o") { e.preventDefault(); void openFile(); }
  else if (k === "s" && e.shiftKey) { e.preventDefault(); void saveAs(); }
  else if (k === "s") { e.preventDefault(); void saveFile(); }
  else if (k === "p") { e.preventDefault(); chkPreview.click(); }
  else if (k === "i") { e.preventDefault(); toggleInspector(); }
});

// Закрытие окна. При несохранённых изменениях спрашиваем подтверждение;
// если пользователь согласен — закрываем окошко через destroy().
const appWindow = getCurrentWindow();

appWindow
  .onCloseRequested(async (event) => {
    if (!store.state().dirty) return; // подтверждать нечего — окно закроется само
    event.preventDefault();
    const ok = await confirm("Закрыть приложение с несохранёнными изменениями?", {
      title: "mdedit",
      kind: "warning",
    });
    if (ok) await appWindow.destroy();
  })
  .catch((e) => console.error("onCloseRequested:", e));

// ---------- стартовый документ ----------

// Создаётся в Rust-сторе: с этого момента id/путь/текст ведёт хост (D5).
const START_TEXT = [
  "# Добро пожаловать в mdedit",
  "",
  "Лёгкий редактор Markdown. Слева — исходник, справа — HTML-предпросмотр,",
  "который собирает `md-core` (CommonMark + GFM).",
  "",
  "## Таблицы (GFM)",
  "",
  "Кликните по заголовку столбца — сортировка ↑/↓; воронка ▾ — фильтр по значениям,",
  "поле над таблицей — поиск. Типы данных определяются автоматически.",
  "",
  "| Файл | Размер | Строк | Изменён |",
  "|------|-------:|------:|-----------|",
  "| README.md | 2 КБ | 48 | 01.10.2026 |",
  "| Cargo.toml | 1 КБ | 21 | 28.09.2026 |",
  "| main.rs | 3,5 КБ | 102 | 03.10.2026 |",
  "| style.css | 4 КБ | 150 | 30.09.2026 |",
  "",
  "## Прочее",
  "- [x] открыть файл",
  "- [x] сохранить файл",
  "- [ ] вкладки",
  "",
  "~~Зачёркнутый текст~~ и **жирный**, `код`, ссылки: [Tauri](https://tauri.app).",
  "",
  "> Цитаты тоже работают.",
].join("\n");

async function bootstrap(): Promise<void> {
  try {
    await store.newDocument(START_TEXT);
  } catch (e) {
    flash(`Запуск: ${errorMessage(e)}`);
  }
  updateStatus();
  updateTitle();
  editorView.focus();
}

void bootstrap();