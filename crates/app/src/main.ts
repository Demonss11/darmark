// mdedit — лёгкий редактор/вьюер Markdown в духе Notepad++.
// Состояние документа + рендер предпросмотра через Rust (crate md-core).

import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  newDocument,
  openDocument,
  saveDocument,
  renderMarkdown,
  errorMessage,
} from "./tauri";
import type { DocumentId } from "./ids";
import type { DocumentSnapshot } from "./tauri";
import { enhanceTables, attachMenuAutoClose } from "./tables";
import { createInspector } from "./inspector";
import { resolveLocalImages } from "./images";
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

const scrollSync = createScrollSync({ editor, preview });

const inspector = createInspector({
  editor,
  preview,
  statusEl: statInspect,
  // Программный scrollIntoView инспектора не должен тянуть редактор.
  beforeScrollIntoView: () => scrollSync.suspend(),
});

let currentId: DocumentId | null = null;
let currentPath: string | null = null;
let dirty = false;
let renderSeq = 0; // защита от «гонки» асинхронных рендеров
let lastRenderedHtml = ""; // чтобы не перетирать DOM (и состояние таблиц) без изменений

// ---------- предпросмотр (debounce, чтобы не спамить IPC на каждое нажатие) ----------

let debounceTimer = 0;
function scheduleRender() {
  clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(doRender, 120);
}

// Сброс состояния рендера при смене документа (P1.1): гасим отложенный рендер
// и делаем неактуальными уже запущенные, чтобы старый ввод не «эхнул» в новый файл.
function resetRenderState() {
  clearTimeout(debounceTimer);
  renderSeq++;
  lastRenderedHtml = "";
  scrollSync.onRendered(); // анкоры предыдущего документа устарели — fallback на пропорцию
}

async function doRender() {
  const seq = ++renderSeq;
  // Фиксируем текст, ушедший в рендер: по нему посчитаны data-md-смещения,
  // поэтому карту инспектора нужно строить именно по нему (см. T-9/T-14).
  const source = editor.value;
  try {
    const html = await renderMarkdown(source, inspector.isActive());
    if (seq !== renderSeq) return; // более новый рендер уже в полёте
    // Если HTML не изменился — не трогаем DOM: иначе сбрасывались бы
    // сортировка, фильтры и фокус в предпросмотре на каждом дебаунсе.
    if (html !== lastRenderedHtml) {
      lastRenderedHtml = html;
      preview.innerHTML = html;
      try {
        enhanceTables(preview); // Excel-подобные сортировка/фильтры для всех <table>
      } catch (e) {
        // Украшение таблиц упало — оставляем читаемый HTML без улучшений (P1.2).
        flash(`Таблицы: ${String(e)}`);
      }
    }
    // Локальные картинки: относительные src → asset-URL (идемпотентно, DOM
    // мог не пересоздаваться, если HTML совпал).
    resolveLocalImages(preview, currentPath);
    // Переиндексация нужна всегда: после изменения текста карта устарела, даже
    // если разметка визуально не поменялась (совпадающий HTML — не повод).
    inspector.onRendered(source);
    // Высоты предпросмотра могли измениться — выравниваем прокрутку (если включена).
    // Аргумент — текст рендера: по нему строятся анкорные карты sync.
    scrollSync.onRendered(source);
  } catch (e) {
    if (seq === renderSeq) {
      lastRenderedHtml = "";
      preview.textContent = `Ошибка рендера: ${String(e)}`;
    }
  }
}

function toggleInspector() {
  if (inspector.isActive()) inspector.disable();
  else inspector.enable();
  btnInspect.classList.toggle("active", inspector.isActive());
  // HTML с обёртками .md-block отличается от обычного — принудительно перерисовываем.
  lastRenderedHtml = "";
  void doRender();
}

attachMenuAutoClose(preview); // закрытие меню фильтров при прокрутке предпросмотра

// Внешние ссылки открываем системным браузером через плагин opener (P0.2).
// window.open в WebView не гарантирует внешнее открытие и обходит CSP,
// поэтому используем отдельный нативный плагин.
preview.addEventListener("click", (e) => {
  const anchor = (e.target as HTMLElement).closest("a");
  if (!anchor) return;
  const href = anchor.getAttribute("href") ?? "";
  if (/^https?:/i.test(href)) {
    e.preventDefault();
    void openUrl(href).catch((err) => flash(`Ссылка: ${String(err)}`));
  }
});

// ---------- статусная строка / заголовок ----------

function updateStatus() {
  const pos = editor.selectionStart ?? 0;
  const before = editor.value.slice(0, pos);
  const line = before.split("\n").length;
  const col = pos - (before.lastIndexOf("\n") + 1) + 1;
  statPos.textContent = `Стр ${line}, Кол ${col}`;
  statSize.textContent = `${[...editor.value].length} симв.`;
}

function baseName(p: string): string {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i >= 0 ? p.slice(i + 1) : p;
}

function updateTitle() {
  const name = currentPath ? baseName(currentPath) : "безымянный";
  void getCurrentWindow().setTitle(`${dirty ? "● " : ""}${name} — mdedit`);
  fileLabel.textContent = name + (dirty ? " ●" : "");
}

function flash(msg: string) {
  statMsg.textContent = msg;
  window.setTimeout(() => {
    if (statMsg.textContent === msg) statMsg.textContent = "";
  }, 4000);
}

// ---------- команды файла ----------

// Приводит UI к снимку документа из Rust-стора: id/путь/текст ведёт хост.
function applySnapshot(snap: DocumentSnapshot) {
  currentId = snap.id;
  currentPath = snap.path;
  dirty = false;
  editor.value = snap.text;
}

async function newFile() {
  if (dirty && !(await confirm("Несохранённые изменения будут потеряны. Продолжить?", { title: "mdedit", kind: "warning" }))) {
    return;
  }
  try {
    applySnapshot(await newDocument(""));
    resetRenderState();
    preview.innerHTML = "";
    inspector.onRendered(editor.value); // сбрасываем устаревшие диапазоны
    updateTitle();
    updateStatus();
    editor.focus();
  } catch (e) {
    flash(`Новый документ: ${errorMessage(e)}`);
  }
}

async function openFile() {
  if (dirty && !(await confirm("Несохранённые изменения будут потеряны. Продолжить?", { title: "mdedit", kind: "warning" }))) {
    return;
  }
  const selected = await open({
    multiple: false,
    filters: [MD_FILTER],
    defaultPath: currentPath ?? undefined,
  });
  if (typeof selected !== "string") return; // отмена
  resetRenderState();
  try {
    applySnapshot(await openDocument(selected));
    updateTitle();
    void doRender();
    updateStatus();
  } catch (e) {
    flash(`Не открылось: ${errorMessage(e)}`);
  }
}

async function saveFile() {
  if (!currentId) return;
  if (!currentPath) return saveAs();
  try {
    await saveDocument(currentId, editor.value);
    dirty = false;
    updateTitle();
    flash("Сохранено");
  } catch (e) {
    flash(`Не сохранилось: ${errorMessage(e)}`);
  }
}

async function saveAs() {
  if (!currentId) return;
  const selected = await save({
    defaultPath: currentPath ?? "untitled.md",
    filters: [MD_FILTER],
  });
  if (typeof selected !== "string") return;
  try {
    const meta = await saveDocument(currentId, editor.value, selected);
    currentPath = meta.path;
    dirty = false;
    updateTitle();
    // Новый каталог — относительные картинки нужно перерезолвить от него.
    resolveLocalImages(preview, currentPath);
    flash("Сохранено");
  } catch (e) {
    flash(`Не сохранилось: ${errorMessage(e)}`);
  }
}

// ---------- события ----------

document.getElementById("btn-new")!.addEventListener("click", () => void newFile());
document.getElementById("btn-open")!.addEventListener("click", () => void openFile());
document.getElementById("btn-save")!.addEventListener("click", () => void saveFile());
document.getElementById("btn-save-as")!.addEventListener("click", () => void saveAs());
btnInspect.addEventListener("click", toggleInspector);

editor.addEventListener("input", () => {
  dirty = true;
  updateTitle();
  updateStatus();
  inspector.onRendered(); // сбрасываем подсветку — диапазоны устарели
  scrollSync.onRendered(); // анкоры sync устарели до перерендера — fallback на пропорцию
  scheduleRender();
});
for (const ev of ["keyup", "click", "select"]) {
  editor.addEventListener(ev, () => {
    updateStatus();
    inspector.onEditorActivity(); // переиспользуем единый хук (T-11)
  });
}

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

// закрытие окна с несохранёнными изменениями — штатный вопрос Windows-диалога
// Закрытие окна. При несохранённых изменениях спрашиваем подтверждение;
// если пользователь согласен — закрываем окошко через destroy().
// Сам onCloseRequested из @tauri-apps/api/window автоматически вызывает
// destroy(), пока обработчик не выставил preventDefault.
const appWindow = getCurrentWindow();

appWindow
  .onCloseRequested(async (event) => {
    if (!dirty) return; // подтверждать нечего — окно закроется само
    event.preventDefault();
    const ok = await confirm("Закрыть приложение с несохранёнными изменениями?", {
      title: "mdedit",
      kind: "warning",
    });
    if (ok) {
      dirty = false;
      await appWindow.destroy();
    }
  })
  .catch((e) => console.error("onCloseRequested:", e));

// Стартовый документ — сразу видно, что таблицы рендерятся.
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

async function bootstrap() {
  try {
    applySnapshot(await newDocument(START_TEXT));
  } catch (e) {
    flash(`Запуск: ${errorMessage(e)}`);
  }
  void doRender();
  updateStatus();
  updateTitle();
  editor.focus();
}

void bootstrap();