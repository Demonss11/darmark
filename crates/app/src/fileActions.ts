// fileActions.ts — файловые команды (Новый/Открыть/Сохранить/Сохранить как).
// Нативные диалоги — на фронтенде (модель Notepad++); текстом и путём владеет
// docStore (проекция Rust-стора, D5). Вынесено из main.ts.

import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import type { DocStore } from "./docStore";
import { errorMessage } from "./tauri";

/** Фильтр нативных диалогов (замороженный набор расширений). */
const MD_FILTER = {
  name: "Markdown",
  extensions: ["md", "markdown", "mdown", "mkd", "txt"],
};

export interface FileActions {
  newFile(): Promise<void>;
  openFile(): Promise<void>;
  saveFile(): Promise<void>;
  saveAs(): Promise<void>;
}

export function createFileActions(opts: {
  store: DocStore;
  /** Сообщение в статусбар (успех/ошибка). */
  status(msg: string): void;
  /** Путь документа изменился (Save As) — перерезолвить локальные картинки. */
  onPathChanged(): void;
  /** Вернуть фокус редактору после команды. */
  focusEditor(): void;
}): FileActions {
  const { store, status, onPathChanged, focusEditor } = opts;

  async function confirmDiscard(): Promise<boolean> {
    if (!store.active()?.dirty) return true;
    return confirm("Несохранённые изменения будут потеряны. Продолжить?", {
      title: "darmark",
      kind: "warning",
    });
  }

  async function newFile(): Promise<void> {
    if (!(await confirmDiscard())) return;
    try {
      await store.newDocument("");
      focusEditor();
    } catch (e) {
      status(`Новый документ: ${errorMessage(e)}`);
    }
  }

  async function openFile(): Promise<void> {
    if (!(await confirmDiscard())) return;
    const selected = await open({
      multiple: false,
      filters: [MD_FILTER],
      defaultPath: store.active()?.path ?? undefined,
    });
    if (typeof selected !== "string") return; // отмена
    try {
      await store.open(selected);
    } catch (e) {
      status(`Не открылось: ${errorMessage(e)}`);
    }
  }

  async function saveFile(): Promise<void> {
    const s = store.active();
    if (!s?.id) {
      status("Документ не создан — сохранение недоступно");
      return;
    }
    if (!s.path) return saveAs();
    try {
      await store.save();
      status("Сохранено");
    } catch (e) {
      status(`Не сохранилось: ${errorMessage(e)}`);
    }
  }

  async function saveAs(): Promise<void> {
    const s = store.active();
    if (!s?.id) {
      status("Документ не создан — сохранение недоступно");
      return;
    }
    const selected = await save({
      defaultPath: s.path ?? "untitled.md",
      filters: [MD_FILTER],
    });
    if (typeof selected !== "string") return;
    try {
      await store.save(selected);
      onPathChanged();
      status("Сохранено");
    } catch (e) {
      status(`Не сохранилось: ${errorMessage(e)}`);
    }
  }

  return { newFile, openFile, saveFile, saveAs };
}
