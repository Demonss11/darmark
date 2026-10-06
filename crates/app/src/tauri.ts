// Тонкие типизированные обёртки над Tauri IPC.
// Вся тяжёлая логика (парсинг md, владение документами) — на Rust-стороне.
import { invoke } from "@tauri-apps/api/core";
import type { DocumentId } from "./ids";

/** Проекция документа из Rust-стора: текст + ревизия + путь. Надмножество `DocMeta`. */
export interface DocumentSnapshot {
  id: DocumentId;
  path: string | null;
  rev: number;
  text: string;
  dirty_hint: boolean;
}

/** Метаданные без текста — ответ `save_document`. */
export interface DocMeta {
  id: DocumentId;
  path: string | null;
  rev: number;
  dirty_hint: boolean;
}

/** Создаёт безымянный документ; `text` — стартовый текст. */
export function newDocument(text?: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("new_document", { text: text ?? null });
}

/** Открывает файл с диска и регистрирует его в Rust-сторе. */
export function openDocument(path: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("open_document", { path });
}

/** Сохраняет документ; `path` задаётся для «Сохранить как». */
export function saveDocument(
  id: DocumentId,
  text: string,
  path?: string
): Promise<DocMeta> {
  return invoke<DocMeta>("save_document", { id, text, path: path ?? null });
}

/** Закрывает документ в сторе (используется вместе с панелями). */
export function closeDocument(id: DocumentId): Promise<void> {
  return invoke<void>("close_document", { id });
}

/** Рендер markdown → HTML (в Фазе 2 переедет в `update_document`/`render_document`). */
export function renderMarkdown(markdown: string, mapped = false): Promise<string> {
  return invoke<string>("render_markdown", { markdown, mapped });
}

/** Достаёт человекочитаемое сообщение из ошибки IPC `{ code, message }` или `Error`. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    const message = (e as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(e);
}
