// mdedit — лёгкий редактор/вьюер Markdown в духе Notepad++.
// Состояние документа + рендер предпросмотра через Rust (crate md-core).

import { open, save, confirm } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { renderMarkdown, readFile, writeFile } from "./tauri";
import { enhanceTables, attachMenuAutoClose } from "./tables";
import "./style.css";

const MD_FILTER = { name: "Markdown", extensions: ["md", "markdown", "mdown", "mkd", "txt"] };

const editor = document.getElementById("editor") as HTMLTextAreaElement;
const preview = document.getElementById("preview") as HTMLElement;
const fileLabel = document.getElementById("file-label") as HTMLElement;
const statPos = document.getElementById("stat-pos") as HTMLElement;
const statSize = document.getElementById("stat-size") as HTMLElement;
const statMsg = document.getElementById("stat-msg") as HTMLElement;
const chkPreview = document.getElementById("chk-preview") as HTMLInputElement;

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

async function doRender() {
  const seq = ++renderSeq;
  try {
    const html = await renderMarkdown(editor.value);
    if (seq !== renderSeq) return; // более новый рендер уже в полёте
    // Если HTML не изменился — не трогаем DOM: иначе сбрасывались бы
    // сортировка, фильтры и фокус в предпросмотре на каждом дебаунсе.
    if (html === lastRenderedHtml) return;
    lastRenderedHtml = html;
    preview.innerHTML = html;
    enhanceTables(preview); // Excel-подобные сортировка/фильтры для всех <table>
  } catch (e) {
    if (seq === renderSeq) {
      lastRenderedHtml = "";
      preview.textContent = `Ошибка рендера: ${String(e)}`;
    }
  }
}

attachMenuAutoClose(preview); // закрытие меню фильтров при прокрутке предпросмотра

// ---------- статусная строка / заголовок ----------

function updateStatus() {
  const pos = editor.selectionStart ?? 0;
  const before = editor.value.slice(0, pos);
  const line = before.split("\n").length;
  const col = pos - (before.lastIndexOf("\n") + 1) + 1;
  statPos.textContent = `Стр ${line}, Кол ${col}`;
  statSize.textContent = `${editor.value.length} симв.`;
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

async function newFile() {
  if (dirty && !(await confirm("Несохранённые изменения будут потеряны. Продолжить?", { title: "mdedit", kind: "warning" }))) {
    return;
  }
  currentPath = null;
  dirty = false;
  editor.value = "";
  preview.innerHTML = "";
  updateTitle();
  updateStatus();
  editor.focus();
}

async function openFile() {
  if (dirty && !(await confirm("Несохранённые изменения будут потеряны. Продолжить?", { title: "mdedit", kind: "warning" }))) {
    return;
  }
  const path = await open({ multiple: false, filters: [MD_FILTER] });
  if (typeof path !== "string") return; // отмена
  try {
    editor.value = await readFile(path);
    currentPath = path;
    dirty = false;
    updateTitle();
    void doRender();
    updateStatus();
  } catch (e) {
    flash(`Не открылось: ${String(e)}`);
  }
}

async function saveFile() {
  if (!currentPath) return saveAs();
  try {
    await writeFile(currentPath, editor.value);
    dirty = false;
    updateTitle();
    flash("Сохранено");
  } catch (e) {
    flash(`Не сохранилось: ${String(e)}`);
  }
}

async function saveAs() {
  const path = await save({
    defaultPath: currentPath ?? "untitled.md",
    filters: [MD_FILTER],
  });
  if (typeof path !== "string") return;
  try {
    await writeFile(path, editor.value);
    currentPath = path;
    dirty = false;
    updateTitle();
    flash("Сохранено");
  } catch (e) {
    flash(`Не сохранилось: ${String(e)}`);
  }
}

// ---------- события ----------

document.getElementById("btn-new")!.addEventListener("click", () => void newFile());
document.getElementById("btn-open")!.addEventListener("click", () => void openFile());
document.getElementById("btn-save")!.addEventListener("click", () => void saveFile());
document.getElementById("btn-save-as")!.addEventListener("click", () => void saveAs());

editor.addEventListener("input", () => {
  dirty = true;
  updateTitle();
  updateStatus();
  scheduleRender();
});
for (const ev of ["keyup", "click"]) editor.addEventListener(ev, updateStatus);

chkPreview.addEventListener("change", () => {
  preview.style.display = chkPreview.checked ? "" : "none";
});

window.addEventListener("keydown", (e: KeyboardEvent) => {
  if (!(e.ctrlKey || e.metaKey)) return;
  const k = e.key.toLowerCase();
  if (k === "n" && !e.shiftKey) { e.preventDefault(); void newFile(); }
  else if (k === "o") { e.preventDefault(); void openFile(); }
  else if (k === "s" && e.shiftKey) { e.preventDefault(); void saveAs(); }
  else if (k === "s") { e.preventDefault(); void saveFile(); }
  else if (k === "p") { e.preventDefault(); chkPreview.click(); }
});

// закрытие окна с несохранёнными изменениями — штатный вопрос Windows-диалога
void getCurrentWindow().onCloseRequested(async (event) => {
  if (dirty) {
    event.preventDefault();
    const ok = await confirm("Закрыть приложение с несохранёнными изменениями?", {
      title: "mdedit",
      kind: "warning",
    });
    if (ok) {
      dirty = false;
      await getCurrentWindow().destroy();
    }
  }
});

// стартовый документ — сразу видно, что таблицы рендерятся
editor.value = [
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
void doRender();
updateStatus();
updateTitle();
editor.focus();
