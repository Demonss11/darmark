// Тонкие типизированные обёртки над Tauri IPC.
// Вся тяжёлая логика (парсинг md, владение документами) — на Rust-стороне.
import { invoke } from "@tauri-apps/api/core";
import type { DocumentId } from "./ids";
import type { Json } from "./viewRegistry";

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

/** Результат рендера: HTML, ревизия и «HTML изменился с прошлого раза». */
export interface RenderResult {
  html: string;
  rev: number;
  changed: boolean;
}

/** Создаёт безымянный документ; `text` — стартовый текст. */
export function newDocument(text?: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("new_document", { text: text ?? null });
}

/** Открывает файл с диска и регистрирует его в Rust-сторе. */
export function openDocument(path: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("open_document", { path });
}

/** Применяет правку текста: стор + рендер (`rev` растёт только при смене текста). */
export function updateDocument(
  id: DocumentId,
  text: string,
  mapped = false
): Promise<RenderResult> {
  return invoke<RenderResult>("update_document", { id, text, mapped });
}

/** Рендерит документ без правки текста (смена `mapped`, первый рендер). */
export function renderDocument(id: DocumentId, mapped = false): Promise<RenderResult> {
  return invoke<RenderResult>("render_document", { id, mapped });
}

/** Сохраняет документ; текст берётся из стора, `path` задаётся для «Сохранить как». */
export function saveDocument(id: DocumentId, path?: string): Promise<DocMeta> {
  return invoke<DocMeta>("save_document", { id, path: path ?? null });
}

/** Закрывает документ в сторе (используется вместе с панелями). */
export function closeDocument(id: DocumentId): Promise<void> {
  return invoke<void>("close_document", { id });
}

/** Описание плагинного тир-1 представления (IPC-команда `plugin_views`, §4.6 TZ-H2). */
export interface PluginViewInfo {
  view_id: string;
  plugin_id: string;
  kind: string;
  title: string;
  html: string;
}

/** Список плагинных тир-1 представлений (пусто, если плагинов нет). */
export function pluginViews(): Promise<PluginViewInfo[]> {
  return invoke<PluginViewInfo[]>("plugin_views");
}

/** Обратная маршрутизация клика из плагинного view в Lua-хендлер (ADR-0022). */
export function pluginViewAction(
  viewId: string,
  action: string,
  payload?: Json
): Promise<void> {
  // Tauri сопоставляет camelCase-ключ `viewId` с Rust-параметром `view_id`.
  return invoke<void>("plugin_view_action", { viewId, action, payload: payload ?? null });
}

/** Достаёт человекочитаемое сообщение из ошибки IPC `{ code, message }` или `Error`. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    const message = (e as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(e);
}
