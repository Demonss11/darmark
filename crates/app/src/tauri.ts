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

/** Статус плагина — tagged union из Rust (§4.7 TZ-H2). */
export type PluginStatus =
  | { state: "stopped" }
  | { state: "active" }
  | { state: "quarantined" }
  | { state: "failed"; message: string };

/** Команда, объявленная плагином (`contributes.commands`). */
export interface PluginCommandInfo {
  id: string;
  title: string;
  keybinding: string | null;
}

/** Проекция плагина для менеджера (IPC-команда `list_plugins`, Фаза 5). */
export interface PluginInfo {
  id: string;
  status: PluginStatus;
  permissions: string[];
  enabled: boolean;
  commands: PluginCommandInfo[];
  notices: string[];
}

/** Список плагинов со статусами/разрешениями (пусто, если плагинов нет). */
export function listPlugins(): Promise<PluginInfo[]> {
  return invoke<PluginInfo[]>("list_plugins");
}

/** Включает/выключает плагин; включение снимает карантин (§4.7). */
export function setPluginEnabled(id: string, enabled: boolean): Promise<void> {
  return invoke<void>("set_plugin_enabled", { id, enabled });
}

/** Перезагружает плагин с диска без рестарта приложения (§4.7). */
export function reloadPlugin(id: string): Promise<void> {
  return invoke<void>("reload_plugin", { id });
}

/** Исполняет команду плагина (`command:invoked` в Lua-хендлер). */
export function runPluginCommand(commandId: string): Promise<void> {
  // Tauri сопоставляет camelCase-ключ `commandId` с Rust-параметром `command_id`.
  return invoke<void>("run_plugin_command", { commandId });
}

/** Dev-only: открыть DevTools WebView2 (в release-сборке — no-op). */
export function openDevtools(): Promise<void> {
  return invoke<void>("open_devtools");
}

/** Достаёт человекочитаемое сообщение из ошибки IPC `{ code, message }` или `Error`. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    const message = (e as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(e);
}
