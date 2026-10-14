// Типизированные обёртки над Tauri IPC. Логика (парсинг md, владение
// документами) — на Rust-стороне; здесь только сигнатуры и контракт ошибок.
import { invoke } from "@tauri-apps/api/core";
import type { DocumentId } from "./ids";
import type { Json } from "./viewRegistry";

/** Проекция документа из Rust-стора; надмножество `DocMeta`. */
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

/** Безымянный документ; `text` — стартовое содержимое. */
export function newDocument(text?: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("new_document", { text: text ?? null });
}

/** Открывает файл и регистрирует его в Rust-сторе. */
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

/** Рендер без правки текста: смена `mapped` или первый рендер. */
export function renderDocument(id: DocumentId, mapped = false): Promise<RenderResult> {
  return invoke<RenderResult>("render_document", { id, mapped });
}

/**
 * Результат undo/redo (D5, ADR-0028): текст после операции и байтовые (UTF-8)
 * смещения результата — по ним редактор восстанавливает выделение.
 */
export interface UndoResult {
  text: string;
  rev: number;
  /** Начало результата (байты UTF-8, inclusive). */
  start: number;
  /** Конец результата (байты UTF-8, exclusive). */
  end: number;
}

/**
 * Отменяет последнюю правку в Rust-undo-стеке. Ошибка IPC при пустом стеке
 * (код `nothing_to_undo`) или неизвестном документе (`not_found`).
 */
export function undoDocument(id: DocumentId): Promise<UndoResult> {
  return invoke<UndoResult>("undo_document", { id });
}

/** Повторяет отменённую правку. Ошибка IPC при пустом redo-стеке (`nothing_to_redo`). */
export function redoDocument(id: DocumentId): Promise<UndoResult> {
  return invoke<UndoResult>("redo_document", { id });
}

/** Текст берётся из стора; `path` задаётся для «Сохранить как». */
export function saveDocument(id: DocumentId, path?: string): Promise<DocMeta> {
  return invoke<DocMeta>("save_document", { id, path: path ?? null });
}

export function closeDocument(id: DocumentId): Promise<void> {
  return invoke<void>("close_document", { id });
}

/** Pull после события `document-updated`; null, если документ неизвестен. */
export function documentSnapshot(id: DocumentId): Promise<DocumentSnapshot | null> {
  return invoke<DocumentSnapshot | null>("document_snapshot", { id });
}

/** Описание плагинного тир-1 представления (IPC-команда `plugin_views`, §4.6 плагинной системы). */
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

/** Статус плагина — tagged union из Rust (§4.7 плагинной системы). */
export type PluginStatus =
  | { state: "stopped" }
  | { state: "active" }
  | { state: "quarantined" }
  | { state: "failed"; message: string };

export interface PluginCommandInfo {
  id: string;
  title: string;
  keybinding: string | null;
}

/**
 * Свойство UI-схемы настроек плагина — подмножество JSON Schema из
 * `contributes.settings.properties` (`docs/PLUGIN_API.md` §1). Данные из
 * манифеста недоверенные, поэтому `type` — свободная строка, а не union:
 * неизвестный тип контрола просто не рендерится.
 */
export interface PluginSettingProperty {
  /** `"boolean" | "integer" | "number" | "string"`; иначе контрол не рендерится. */
  type?: string;
  /** Фолбэк подписи — ключ свойства. */
  title?: string;
  /** Мелкая подсказка под контролом. */
  description?: string;
  /** Значение, если в сторе для ключа ничего нет. */
  default?: unknown;
  /** Допустимые значения — контрол становится `<select>`. */
  enum?: Json[];
}

/** UI-схема настроек `contributes.settings`: объект с картой свойств. */
export interface PluginSettingsSchema {
  type?: string;
  properties?: Record<string, PluginSettingProperty>;
}

/** Проекция плагина для менеджера (IPC-команда `list_plugins`, Фаза 5). */
export interface PluginInfo {
  id: string;
  /** Имя и версия из манифеста (голова карточки, S12a). */
  name: string;
  version: string;
  status: PluginStatus;
  /** Права из манифеста (декларация). */
  permissions: string[];
  /** Согласованные пользователем права (подмножество `permissions`, deny-by-default). */
  granted_permissions: string[];
  enabled: boolean;
  commands: PluginCommandInfo[];
  notices: string[];
  /** UI-схема из манифеста (`contributes.settings`); null — секцию не рендерим. */
  settings_schema: PluginSettingsSchema | null;
  /** Сохранённые значения настроек (per-plugin JSON-объект; может быть пустым). */
  settings_values: Record<string, unknown>;
}

/** Список плагинов со статусами/разрешениями (пусто, если плагинов нет). */
export function listPlugins(): Promise<PluginInfo[]> {
  return invoke<PluginInfo[]>("list_plugins");
}

/** Включает/выключает плагин; включение снимает карантин (§4.7). */
export function setPluginEnabled(id: string, enabled: boolean): Promise<void> {
  return invoke<void>("set_plugin_enabled", { id, enabled });
}

/** Фиксирует согласие пользователя на права плагина; активный перезапускается (§11.1 п.4). */
export function setPluginPermissions(id: string, granted: string[]): Promise<void> {
  return invoke<void>("set_plugin_permissions", { id, granted });
}

/**
 * Записывает значение настройки плагина (`host.set_setting` в сторе Rust).
 * `value` — примитив из UI-контрола; тип приводится на фронте по схеме,
 * валидация и хранение — на Rust-стороне.
 */
export function setPluginSetting(id: string, key: string, value: Json): Promise<void> {
  return invoke<void>("set_plugin_setting", { id, key, value });
}

/** Перезагружает плагин с диска без рестарта приложения (§4.7). */
export function reloadPlugin(id: string): Promise<void> {
  return invoke<void>("reload_plugin", { id });
}

/** Полностью удаляет плагин: останавливает child, снимает views, удаляет каталог и настройки. */
export function removePlugin(id: string): Promise<void> {
  return invoke<void>("remove_plugin", { id });
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

// --- AI-ассистент (ADR-0030/0032/0033/0034) ---
// Сеть — только на Rust-стороне (CSP `default-src 'self'`). API-ключ наружу
// не отдаётся: фронт видит лишь флаг `has_key`.

/** Настройки AI без секрета. */
export interface AiSettingsView {
  enabled: boolean;
  provider: string;
  base_url: string;
  model: string;
  max_tokens: number;
  temperature: number;
  consent_document_send: boolean;
  has_key: boolean;
}

/** Полная секция настроек AI без секрета — вход `ai_set_settings`. */
export interface AiSettingsInput {
  enabled: boolean;
  provider: string;
  base_url: string;
  model: string;
  max_tokens: number;
  temperature: number;
  consent_document_send: boolean;
}

/** Сообщение диалога (формат OpenAI-совместимого API). */
export interface AiMessage {
  // `tool` — результат MCP-инструмента (Epic 6); Rust сохраняет эту роль в историю,
  // поэтому она входит в контракт, даже если сейчас фронт такие сообщения не создаёт.
  role: "system" | "user" | "assistant" | "tool";
  content: string;
}

/** Расход токенов ответа (prompt + completion). */
export interface AiUsage {
  prompt_tokens: number;
  completion_tokens: number;
}

export interface AiResponse {
  message: AiMessage;
  usage: AiUsage | null;
  /** Причина завершения от провайдера: `"stop"`, `"length"` и т.д.; `null` — неизвестно. */
  finish_reason: string | null;
}

/** Готовый скилл (промт) из библиотеки Rust. */
export interface AiSkill {
  id: string;
  title: string;
  description: string;
  prompt_template: string;
  category: string;
}

export interface AiTestResult {
  ok: boolean;
  model: string | null;
  error: string | null;
}

/** Секрет читается только маской через `has_key`. */
export function aiGetSettings(): Promise<AiSettingsView> {
  return invoke<AiSettingsView>("ai_get_settings");
}

/** Настройки пишутся в config.json; секретный ключ наружу не возвращается (без ключа). */
export function aiSetSettings(settings: AiSettingsInput): Promise<void> {
  return invoke<void>("ai_set_settings", { settings });
}

/** Ключ пишется в config.json и наружу не возвращается. */
export function aiSetApiKey(key: string): Promise<void> {
  return invoke<void>("ai_set_api_key", { key });
}

export function aiListSkills(): Promise<AiSkill[]> {
  return invoke<AiSkill[]>("ai_list_skills");
}

/** Стриминг приходит событиями `ai:*`. */
export function aiChat(messages: AiMessage[]): Promise<AiResponse> {
  return invoke<AiResponse>("ai_chat", { messages });
}

/** Проверка соединения с провайдером — без отправки документа. */
export function aiTestConnection(): Promise<AiTestResult> {
  return invoke<AiTestResult>("ai_test_connection");
}

/** Рендер текста ответа AI как Markdown через `md-core` (ADR-0038 §2.2). */
export function aiRenderMarkdown(text: string): Promise<string> {
  return invoke<string>("ai_render_markdown", { text });
}

// --- История чата (IPC-команды ai_history_load/save/clear) ---

/** Проекция истории чата из Rust (IPC-команда `ai_history_load`). */
export interface HistoryView {
  /** Идентификатор сессии (генерируется Rust). */
  session_id: string;
  /** Unix-секунды последнего обновления файла истории. */
  updated_at: number;
  /** Сообщения диалога (role/content). */
  messages: AiMessage[];
  /** Предупреждение (битый файл, превышение лимита) или null. */
  warning: string | null;
}

/** Загружает историю чата с диска (IPC-команда `ai_history_load`). */
export function aiHistoryLoad(): Promise<HistoryView> {
  return invoke<HistoryView>("ai_history_load");
}

/** Сохраняет историю чата на диск (полная замена файла). */
export function aiHistorySave(messages: AiMessage[]): Promise<void> {
  return invoke<void>("ai_history_save", { messages });
}

/** Очищает историю чата на диске (перезапись пустой сессией). */
export function aiHistoryClear(): Promise<void> {
  return invoke<void>("ai_history_clear");
}

// --- AI-ассистент: harness Фазы 2 (ADR-0034) ---
// Правка документа идёт через `DocumentStore::apply_edit` (D5); плагины — через
// тот же цикл валидации, что и пользовательские (песочница, D16).

/** Операция правки документа: байтовые (UTF-8) смещения + новый текст (ADR-0034). */
export interface AiEditOp {
  start: number;
  end: number;
  text: string;
}

/** Предложенная правка: исходный/новый текст и минимальный дифф (ADR-0034). */
export interface AiDiff {
  doc_id: string;
  base_rev: number;
  original: string;
  modified: string;
  ops: AiEditOp[];
}

/** Результат валидации плагина (манифест + entry + песочница, ADR-0034/ADR-0026). */
export interface AiValidationResult {
  ok: boolean;
  errors: string[];
  /** Версионированный отчёт CLI/fallback (`schema_version`); null — отчёта нет. */
  report: unknown | null;
}

/** Предлагает правку документа: минимальный дифф текущего текста и `proposed` (ADR-0034). */
export function aiDiff(docId: DocumentId, proposed: string): Promise<AiDiff> {
  return invoke<AiDiff>("ai_diff", { docId, proposed });
}

/** Применяет правку через `DocumentStore::apply_edit`; `baseRev` защищает от гонок (ADR-0034). */
export function aiApplyEdit(
  docId: DocumentId,
  baseRev: number,
  ops: AiEditOp[]
): Promise<UndoResult> {
  return invoke<UndoResult>("ai_apply_edit", { docId, baseRev, ops });
}

/** Валидирует плагин (манифест + entry + прогон в песочнице, ADR-0034/ADR-0026). */
export function aiValidatePlugin(
  id: string,
  manifest: Json,
  source: string
): Promise<AiValidationResult> {
  return invoke<AiValidationResult>("ai_validate_plugin", { id, manifest, source });
}

/** Создаёт плагин: запись → валидация → регистрация; только при `ok=true` (ADR-0034/ADR-0026). */
export function aiCreatePlugin(id: string, manifest: Json, source: string): Promise<PluginInfo> {
  return invoke<PluginInfo>("ai_create_plugin", { id, manifest, source });
}

/** Обновляет плагин: перезапись → валидация → reload без рестарта приложения (ADR-0034/ADR-0026). */
export function aiUpdatePlugin(id: string, manifest: Json, source: string): Promise<PluginInfo> {
  return invoke<PluginInfo>("ai_update_plugin", { id, manifest, source });
}

/** Удаляет плагин (файлы + настройки), останавливая child-процесс. */
export function aiDeletePlugin(id: string): Promise<void> {
  return invoke<void>("ai_delete_plugin", { id });
}
