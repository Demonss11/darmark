// РўРёРїРёР·РёСЂРѕРІР°РЅРЅС‹Рµ РѕР±С‘СЂС‚РєРё РЅР°Рґ Tauri IPC. Р›РѕРіРёРєР° (РїР°СЂСЃРёРЅРі md, РІР»Р°РґРµРЅРёРµ
// РґРѕРєСѓРјРµРЅС‚Р°РјРё) вЂ” РЅР° Rust-СЃС‚РѕСЂРѕРЅРµ; Р·РґРµСЃСЊ С‚РѕР»СЊРєРѕ СЃРёРіРЅР°С‚СѓСЂС‹ Рё РєРѕРЅС‚СЂР°РєС‚ РѕС€РёР±РѕРє.
import { invoke } from "@tauri-apps/api/core";
import type { DocumentId } from "./ids";
import type { Json } from "../viewRegistry";

/** РџСЂРѕРµРєС†РёСЏ РґРѕРєСѓРјРµРЅС‚Р° РёР· Rust-СЃС‚РѕСЂР°; РЅР°РґРјРЅРѕР¶РµСЃС‚РІРѕ `DocMeta`. */
export interface DocumentSnapshot {
  id: DocumentId;
  path: string | null;
  rev: number;
  text: string;
  dirty_hint: boolean;
}

/** РњРµС‚Р°РґР°РЅРЅС‹Рµ Р±РµР· С‚РµРєСЃС‚Р° вЂ” РѕС‚РІРµС‚ `save_document`. */
export interface DocMeta {
  id: DocumentId;
  path: string | null;
  rev: number;
  dirty_hint: boolean;
}

/** Р РµР·СѓР»СЊС‚Р°С‚ СЂРµРЅРґРµСЂР°: HTML, СЂРµРІРёР·РёСЏ Рё В«HTML РёР·РјРµРЅРёР»СЃСЏ СЃ РїСЂРѕС€Р»РѕРіРѕ СЂР°Р·Р°В». */
export interface RenderResult {
  html: string;
  rev: number;
  changed: boolean;
}

/** Р‘РµР·С‹РјСЏРЅРЅС‹Р№ РґРѕРєСѓРјРµРЅС‚; `text` вЂ” СЃС‚Р°СЂС‚РѕРІРѕРµ СЃРѕРґРµСЂР¶РёРјРѕРµ. */
export function newDocument(text?: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("new_document", { text: text ?? null });
}

/** РћС‚РєСЂС‹РІР°РµС‚ С„Р°Р№Р» Рё СЂРµРіРёСЃС‚СЂРёСЂСѓРµС‚ РµРіРѕ РІ Rust-СЃС‚РѕСЂРµ. */
export function openDocument(path: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("open_document", { path });
}

/** РџСЂРёРјРµРЅСЏРµС‚ РїСЂР°РІРєСѓ С‚РµРєСЃС‚Р°: СЃС‚РѕСЂ + СЂРµРЅРґРµСЂ (`rev` СЂР°СЃС‚С‘С‚ С‚РѕР»СЊРєРѕ РїСЂРё СЃРјРµРЅРµ С‚РµРєСЃС‚Р°). */
export function updateDocument(
  id: DocumentId,
  text: string,
  mapped = false
): Promise<RenderResult> {
  return invoke<RenderResult>("update_document", { id, text, mapped });
}

/** Р РµРЅРґРµСЂ Р±РµР· РїСЂР°РІРєРё С‚РµРєСЃС‚Р°: СЃРјРµРЅР° `mapped` РёР»Рё РїРµСЂРІС‹Р№ СЂРµРЅРґРµСЂ. */
export function renderDocument(id: DocumentId, mapped = false): Promise<RenderResult> {
  return invoke<RenderResult>("render_document", { id, mapped });
}



export interface UndoResult {
  text: string;
  rev: number;
  /** РќР°С‡Р°Р»Рѕ СЂРµР·СѓР»СЊС‚Р°С‚Р° (Р±Р°Р№С‚С‹ UTF-8, inclusive). */
  start: number;
  /** РљРѕРЅРµС† СЂРµР·СѓР»СЊС‚Р°С‚Р° (Р±Р°Р№С‚С‹ UTF-8, exclusive). */
  end: number;
}



export function undoDocument(id: DocumentId): Promise<UndoResult> {
  return invoke<UndoResult>("undo_document", { id });
}

/** РџРѕРІС‚РѕСЂСЏРµС‚ РѕС‚РјРµРЅС‘РЅРЅСѓСЋ РїСЂР°РІРєСѓ. РћС€РёР±РєР° IPC РїСЂРё РїСѓСЃС‚РѕРј redo-СЃС‚РµРєРµ (`nothing_to_redo`). */
export function redoDocument(id: DocumentId): Promise<UndoResult> {
  return invoke<UndoResult>("redo_document", { id });
}

/** РўРµРєСЃС‚ Р±РµСЂС‘С‚СЃСЏ РёР· СЃС‚РѕСЂР°; `path` Р·Р°РґР°С‘С‚СЃСЏ РґР»СЏ В«РЎРѕС…СЂР°РЅРёС‚СЊ РєР°РєВ». */
export function saveDocument(id: DocumentId, path?: string): Promise<DocMeta> {
  return invoke<DocMeta>("save_document", { id, path: path ?? null });
}

export function closeDocument(id: DocumentId): Promise<void> {
  return invoke<void>("close_document", { id });
}

/** Pull РїРѕСЃР»Рµ СЃРѕР±С‹С‚РёСЏ `document-updated`; null, РµСЃР»Рё РґРѕРєСѓРјРµРЅС‚ РЅРµРёР·РІРµСЃС‚РµРЅ. */
export function documentSnapshot(id: DocumentId): Promise<DocumentSnapshot | null> {
  return invoke<DocumentSnapshot | null>("document_snapshot", { id });
}

/** РћРїРёСЃР°РЅРёРµ РїР»Р°РіРёРЅРЅРѕРіРѕ С‚РёСЂ-1 РїСЂРµРґСЃС‚Р°РІР»РµРЅРёСЏ (IPC-РєРѕРјР°РЅРґР° `plugin_views`, В§4.6 РїР»Р°РіРёРЅРЅРѕР№ СЃРёСЃС‚РµРјС‹). */
export interface PluginViewInfo {
  view_id: string;
  plugin_id: string;
  kind: string;
  title: string;
  html: string;
}

/** РЎРїРёСЃРѕРє РїР»Р°РіРёРЅРЅС‹С… С‚РёСЂ-1 РїСЂРµРґСЃС‚Р°РІР»РµРЅРёР№ (РїСѓСЃС‚Рѕ, РµСЃР»Рё РїР»Р°РіРёРЅРѕРІ РЅРµС‚). */
export function pluginViews(): Promise<PluginViewInfo[]> {
  return invoke<PluginViewInfo[]>("plugin_views");
}

/** РћР±СЂР°С‚РЅР°СЏ РјР°СЂС€СЂСѓС‚РёР·Р°С†РёСЏ РєР»РёРєР° РёР· РїР»Р°РіРёРЅРЅРѕРіРѕ view РІ Lua-С…РµРЅРґР»РµСЂ (ADR-0022). */
export function pluginViewAction(
  viewId: string,
  action: string,
  payload?: Json
): Promise<void> {

  return invoke<void>("plugin_view_action", { viewId, action, payload: payload ?? null });
}

/** РЎС‚Р°С‚СѓСЃ РїР»Р°РіРёРЅР° вЂ” tagged union РёР· Rust (В§4.7 РїР»Р°РіРёРЅРЅРѕР№ СЃРёСЃС‚РµРјС‹). */
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



export interface PluginSettingProperty {
  /** `"boolean" | "integer" | "number" | "string"`; РёРЅР°С‡Рµ РєРѕРЅС‚СЂРѕР» РЅРµ СЂРµРЅРґРµСЂРёС‚СЃСЏ. */
  type?: string;
  /** Р¤РѕР»Р±СЌРє РїРѕРґРїРёСЃРё вЂ” РєР»СЋС‡ СЃРІРѕР№СЃС‚РІР°. */
  title?: string;
  /** РњРµР»РєР°СЏ РїРѕРґСЃРєР°Р·РєР° РїРѕРґ РєРѕРЅС‚СЂРѕР»РѕРј. */
  description?: string;
  /** Р—РЅР°С‡РµРЅРёРµ, РµСЃР»Рё РІ СЃС‚РѕСЂРµ РґР»СЏ РєР»СЋС‡Р° РЅРёС‡РµРіРѕ РЅРµС‚. */
  default?: unknown;
  /** Р”РѕРїСѓСЃС‚РёРјС‹Рµ Р·РЅР°С‡РµРЅРёСЏ вЂ” РєРѕРЅС‚СЂРѕР» СЃС‚Р°РЅРѕРІРёС‚СЃСЏ `<select>`. */
  enum?: Json[];
}

/** UI-СЃС…РµРјР° РЅР°СЃС‚СЂРѕРµРє `contributes.settings`: РѕР±СЉРµРєС‚ СЃ РєР°СЂС‚РѕР№ СЃРІРѕР№СЃС‚РІ. */
export interface PluginSettingsSchema {
  type?: string;
  properties?: Record<string, PluginSettingProperty>;
}

/** РџСЂРѕРµРєС†РёСЏ РїР»Р°РіРёРЅР° РґР»СЏ РјРµРЅРµРґР¶РµСЂР° (IPC-РєРѕРјР°РЅРґР° `list_plugins`, Р¤Р°Р·Р° 5). */
export interface PluginInfo {
  id: string;
  /** РРјСЏ Рё РІРµСЂСЃРёСЏ РёР· РјР°РЅРёС„РµСЃС‚Р° (РіРѕР»РѕРІР° РєР°СЂС‚РѕС‡РєРё, S12a). */
  name: string;
  version: string;
  status: PluginStatus;
  /** РџСЂР°РІР° РёР· РјР°РЅРёС„РµСЃС‚Р° (РґРµРєР»Р°СЂР°С†РёСЏ). */
  permissions: string[];
  /** РЎРѕРіР»Р°СЃРѕРІР°РЅРЅС‹Рµ РїРѕР»СЊР·РѕРІР°С‚РµР»РµРј РїСЂР°РІР° (РїРѕРґРјРЅРѕР¶РµСЃС‚РІРѕ `permissions`, deny-by-default). */
  granted_permissions: string[];
  enabled: boolean;
  commands: PluginCommandInfo[];
  notices: string[];
  /** UI-СЃС…РµРјР° РёР· РјР°РЅРёС„РµСЃС‚Р° (`contributes.settings`); null вЂ” СЃРµРєС†РёСЋ РЅРµ СЂРµРЅРґРµСЂРёРј. */
  settings_schema: PluginSettingsSchema | null;
  /** РЎРѕС…СЂР°РЅС‘РЅРЅС‹Рµ Р·РЅР°С‡РµРЅРёСЏ РЅР°СЃС‚СЂРѕРµРє (per-plugin JSON-РѕР±СЉРµРєС‚; РјРѕР¶РµС‚ Р±С‹С‚СЊ РїСѓСЃС‚С‹Рј). */
  settings_values: Record<string, unknown>;
}

/** РЎРїРёСЃРѕРє РїР»Р°РіРёРЅРѕРІ СЃРѕ СЃС‚Р°С‚СѓСЃР°РјРё/СЂР°Р·СЂРµС€РµРЅРёСЏРјРё (РїСѓСЃС‚Рѕ, РµСЃР»Рё РїР»Р°РіРёРЅРѕРІ РЅРµС‚). */
export function listPlugins(): Promise<PluginInfo[]> {
  return invoke<PluginInfo[]>("list_plugins");
}

/** Р’РєР»СЋС‡Р°РµС‚/РІС‹РєР»СЋС‡Р°РµС‚ РїР»Р°РіРёРЅ; РІРєР»СЋС‡РµРЅРёРµ СЃРЅРёРјР°РµС‚ РєР°СЂР°РЅС‚РёРЅ (В§4.7). */
export function setPluginEnabled(id: string, enabled: boolean): Promise<void> {
  return invoke<void>("set_plugin_enabled", { id, enabled });
}

/** Р¤РёРєСЃРёСЂСѓРµС‚ СЃРѕРіР»Р°СЃРёРµ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ РЅР° РїСЂР°РІР° РїР»Р°РіРёРЅР°; Р°РєС‚РёРІРЅС‹Р№ РїРµСЂРµР·Р°РїСѓСЃРєР°РµС‚СЃСЏ (В§11.1 Рї.4). */
export function setPluginPermissions(id: string, granted: string[]): Promise<void> {
  return invoke<void>("set_plugin_permissions", { id, granted });
}



export function setPluginSetting(id: string, key: string, value: Json): Promise<void> {
  return invoke<void>("set_plugin_setting", { id, key, value });
}

/** РџРµСЂРµР·Р°РіСЂСѓР¶Р°РµС‚ РїР»Р°РіРёРЅ СЃ РґРёСЃРєР° Р±РµР· СЂРµСЃС‚Р°СЂС‚Р° РїСЂРёР»РѕР¶РµРЅРёСЏ (В§4.7). */
export function reloadPlugin(id: string): Promise<void> {
  return invoke<void>("reload_plugin", { id });
}

/** РџРѕР»РЅРѕСЃС‚СЊСЋ СѓРґР°Р»СЏРµС‚ РїР»Р°РіРёРЅ: РѕСЃС‚Р°РЅР°РІР»РёРІР°РµС‚ child, СЃРЅРёРјР°РµС‚ views, СѓРґР°Р»СЏРµС‚ РєР°С‚Р°Р»РѕРі Рё РЅР°СЃС‚СЂРѕР№РєРё. */
export function removePlugin(id: string): Promise<void> {
  return invoke<void>("remove_plugin", { id });
}

/** РСЃРїРѕР»РЅСЏРµС‚ РєРѕРјР°РЅРґСѓ РїР»Р°РіРёРЅР° (`command:invoked` РІ Lua-С…РµРЅРґР»РµСЂ). */
export function runPluginCommand(commandId: string): Promise<void> {

  return invoke<void>("run_plugin_command", { commandId });
}

/** Dev-only: РѕС‚РєСЂС‹С‚СЊ DevTools WebView2 (РІ release-СЃР±РѕСЂРєРµ вЂ” no-op). */
export function openDevtools(): Promise<void> {
  return invoke<void>("open_devtools");
}

/** Р”РѕСЃС‚Р°С‘С‚ С‡РµР»РѕРІРµРєРѕС‡РёС‚Р°РµРјРѕРµ СЃРѕРѕР±С‰РµРЅРёРµ РёР· РѕС€РёР±РєРё IPC `{ code, message }` РёР»Рё `Error`. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    const message = (e as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(e);
}





/** РќР°СЃС‚СЂРѕР№РєРё AI Р±РµР· СЃРµРєСЂРµС‚Р°. */
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

/** РџРѕР»РЅР°СЏ СЃРµРєС†РёСЏ РЅР°СЃС‚СЂРѕРµРє AI Р±РµР· СЃРµРєСЂРµС‚Р° вЂ” РІС…РѕРґ `ai_set_settings`. */
export interface AiSettingsInput {
  enabled: boolean;
  provider: string;
  base_url: string;
  model: string;
  max_tokens: number;
  temperature: number;
  consent_document_send: boolean;
}

/** РЎРѕРѕР±С‰РµРЅРёРµ РґРёР°Р»РѕРіР° (С„РѕСЂРјР°С‚ OpenAI-СЃРѕРІРјРµСЃС‚РёРјРѕРіРѕ API). */
export interface AiMessage {


  role: "system" | "user" | "assistant" | "tool";
  content: string;
}

/** Р Р°СЃС…РѕРґ С‚РѕРєРµРЅРѕРІ РѕС‚РІРµС‚Р° (prompt + completion). */
export interface AiUsage {
  prompt_tokens: number;
  completion_tokens: number;
}

export interface AiResponse {
  message: AiMessage;
  usage: AiUsage | null;
  /** РџСЂРёС‡РёРЅР° Р·Р°РІРµСЂС€РµРЅРёСЏ РѕС‚ РїСЂРѕРІР°Р№РґРµСЂР°: `"stop"`, `"length"` Рё С‚.Рґ.; `null` вЂ” РЅРµРёР·РІРµСЃС‚РЅРѕ. */
  finish_reason: string | null;
}

/** Р“РѕС‚РѕРІС‹Р№ СЃРєРёР»Р» (РїСЂРѕРјС‚) РёР· Р±РёР±Р»РёРѕС‚РµРєРё Rust. */
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

/** РЎРµРєСЂРµС‚ С‡РёС‚Р°РµС‚СЃСЏ С‚РѕР»СЊРєРѕ РјР°СЃРєРѕР№ С‡РµСЂРµР· `has_key`. */
export function aiGetSettings(): Promise<AiSettingsView> {
  return invoke<AiSettingsView>("ai_get_settings");
}

/** РќР°СЃС‚СЂРѕР№РєРё РїРёС€СѓС‚СЃСЏ РІ config.json; СЃРµРєСЂРµС‚РЅС‹Р№ РєР»СЋС‡ РЅР°СЂСѓР¶Сѓ РЅРµ РІРѕР·РІСЂР°С‰Р°РµС‚СЃСЏ (Р±РµР· РєР»СЋС‡Р°). */
export function aiSetSettings(settings: AiSettingsInput): Promise<void> {
  return invoke<void>("ai_set_settings", { settings });
}

/** РљР»СЋС‡ РїРёС€РµС‚СЃСЏ РІ config.json Рё РЅР°СЂСѓР¶Сѓ РЅРµ РІРѕР·РІСЂР°С‰Р°РµС‚СЃСЏ. */
export function aiSetApiKey(key: string): Promise<void> {
  return invoke<void>("ai_set_api_key", { key });
}

export function aiListSkills(): Promise<AiSkill[]> {
  return invoke<AiSkill[]>("ai_list_skills");
}

/** РЎС‚СЂРёРјРёРЅРі РїСЂРёС…РѕРґРёС‚ СЃРѕР±С‹С‚РёСЏРјРё `ai:*`. */
export function aiChat(messages: AiMessage[]): Promise<AiResponse> {
  return invoke<AiResponse>("ai_chat", { messages });
}

/** РџСЂРѕРІРµСЂРєР° СЃРѕРµРґРёРЅРµРЅРёСЏ СЃ РїСЂРѕРІР°Р№РґРµСЂРѕРј вЂ” Р±РµР· РѕС‚РїСЂР°РІРєРё РґРѕРєСѓРјРµРЅС‚Р°. */
export function aiTestConnection(): Promise<AiTestResult> {
  return invoke<AiTestResult>("ai_test_connection");
}

/** Р РµРЅРґРµСЂ С‚РµРєСЃС‚Р° РѕС‚РІРµС‚Р° AI РєР°Рє Markdown С‡РµСЂРµР· `md-core` (ADR-0038 В§2.2). */
export function aiRenderMarkdown(text: string): Promise<string> {
  return invoke<string>("ai_render_markdown", { text });
}



/** РџСЂРѕРµРєС†РёСЏ РёСЃС‚РѕСЂРёРё С‡Р°С‚Р° РёР· Rust (IPC-РєРѕРјР°РЅРґР° `ai_history_load`). */
export interface HistoryView {
  /** РРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ СЃРµСЃСЃРёРё (РіРµРЅРµСЂРёСЂСѓРµС‚СЃСЏ Rust). */
  session_id: string;
  /** Unix-СЃРµРєСѓРЅРґС‹ РїРѕСЃР»РµРґРЅРµРіРѕ РѕР±РЅРѕРІР»РµРЅРёСЏ С„Р°Р№Р»Р° РёСЃС‚РѕСЂРёРё. */
  updated_at: number;
  /** РЎРѕРѕР±С‰РµРЅРёСЏ РґРёР°Р»РѕРіР° (role/content). */
  messages: AiMessage[];
  /** РџСЂРµРґСѓРїСЂРµР¶РґРµРЅРёРµ (Р±РёС‚С‹Р№ С„Р°Р№Р», РїСЂРµРІС‹С€РµРЅРёРµ Р»РёРјРёС‚Р°) РёР»Рё null. */
  warning: string | null;
}

/** Р—Р°РіСЂСѓР¶Р°РµС‚ РёСЃС‚РѕСЂРёСЋ С‡Р°С‚Р° СЃ РґРёСЃРєР° (IPC-РєРѕРјР°РЅРґР° `ai_history_load`). */
export function aiHistoryLoad(): Promise<HistoryView> {
  return invoke<HistoryView>("ai_history_load");
}

/** РЎРѕС…СЂР°РЅСЏРµС‚ РёСЃС‚РѕСЂРёСЋ С‡Р°С‚Р° РЅР° РґРёСЃРє (РїРѕР»РЅР°СЏ Р·Р°РјРµРЅР° С„Р°Р№Р»Р°). */
export function aiHistorySave(messages: AiMessage[]): Promise<void> {
  return invoke<void>("ai_history_save", { messages });
}

/** РћС‡РёС‰Р°РµС‚ РёСЃС‚РѕСЂРёСЋ С‡Р°С‚Р° РЅР° РґРёСЃРєРµ (РїРµСЂРµР·Р°РїРёСЃСЊ РїСѓСЃС‚РѕР№ СЃРµСЃСЃРёРµР№). */
export function aiHistoryClear(): Promise<void> {
  return invoke<void>("ai_history_clear");
}





/** РћРїРµСЂР°С†РёСЏ РїСЂР°РІРєРё РґРѕРєСѓРјРµРЅС‚Р°: Р±Р°Р№С‚РѕРІС‹Рµ (UTF-8) СЃРјРµС‰РµРЅРёСЏ + РЅРѕРІС‹Р№ С‚РµРєСЃС‚ (ADR-0034). */
export interface AiEditOp {
  start: number;
  end: number;
  text: string;
}

/** РџСЂРµРґР»РѕР¶РµРЅРЅР°СЏ РїСЂР°РІРєР°: РёСЃС…РѕРґРЅС‹Р№/РЅРѕРІС‹Р№ С‚РµРєСЃС‚ Рё РјРёРЅРёРјР°Р»СЊРЅС‹Р№ РґРёС„С„ (ADR-0034). */
export interface AiDiff {
  doc_id: string;
  base_rev: number;
  original: string;
  modified: string;
  ops: AiEditOp[];
}

/** Р РµР·СѓР»СЊС‚Р°С‚ РІР°Р»РёРґР°С†РёРё РїР»Р°РіРёРЅР° (РјР°РЅРёС„РµСЃС‚ + entry + РїРµСЃРѕС‡РЅРёС†Р°, ADR-0034/ADR-0026). */
export interface AiValidationResult {
  ok: boolean;
  errors: string[];
  /** Р’РµСЂСЃРёРѕРЅРёСЂРѕРІР°РЅРЅС‹Р№ РѕС‚С‡С‘С‚ CLI/fallback (`schema_version`); null вЂ” РѕС‚С‡С‘С‚Р° РЅРµС‚. */
  report: unknown | null;
}

/** РџСЂРµРґР»Р°РіР°РµС‚ РїСЂР°РІРєСѓ РґРѕРєСѓРјРµРЅС‚Р°: РјРёРЅРёРјР°Р»СЊРЅС‹Р№ РґРёС„С„ С‚РµРєСѓС‰РµРіРѕ С‚РµРєСЃС‚Р° Рё `proposed` (ADR-0034). */
export function aiDiff(docId: DocumentId, proposed: string): Promise<AiDiff> {
  return invoke<AiDiff>("ai_diff", { docId, proposed });
}

/** РџСЂРёРјРµРЅСЏРµС‚ РїСЂР°РІРєСѓ С‡РµСЂРµР· `DocumentStore::apply_edit`; `baseRev` Р·Р°С‰РёС‰Р°РµС‚ РѕС‚ РіРѕРЅРѕРє (ADR-0034). */
export function aiApplyEdit(
  docId: DocumentId,
  baseRev: number,
  ops: AiEditOp[]
): Promise<UndoResult> {
  return invoke<UndoResult>("ai_apply_edit", { docId, baseRev, ops });
}

/** Р’Р°Р»РёРґРёСЂСѓРµС‚ РїР»Р°РіРёРЅ (РјР°РЅРёС„РµСЃС‚ + entry + РїСЂРѕРіРѕРЅ РІ РїРµСЃРѕС‡РЅРёС†Рµ, ADR-0034/ADR-0026). */
export function aiValidatePlugin(
  id: string,
  manifest: Json,
  source: string
): Promise<AiValidationResult> {
  return invoke<AiValidationResult>("ai_validate_plugin", { id, manifest, source });
}

/** РЎРѕР·РґР°С‘С‚ РїР»Р°РіРёРЅ: Р·Р°РїРёСЃСЊ в†’ РІР°Р»РёРґР°С†РёСЏ в†’ СЂРµРіРёСЃС‚СЂР°С†РёСЏ; С‚РѕР»СЊРєРѕ РїСЂРё `ok=true` (ADR-0034/ADR-0026). */
export function aiCreatePlugin(id: string, manifest: Json, source: string): Promise<PluginInfo> {
  return invoke<PluginInfo>("ai_create_plugin", { id, manifest, source });
}

/** РћР±РЅРѕРІР»СЏРµС‚ РїР»Р°РіРёРЅ: РїРµСЂРµР·Р°РїРёСЃСЊ в†’ РІР°Р»РёРґР°С†РёСЏ в†’ reload Р±РµР· СЂРµСЃС‚Р°СЂС‚Р° РїСЂРёР»РѕР¶РµРЅРёСЏ (ADR-0034/ADR-0026). */
export function aiUpdatePlugin(id: string, manifest: Json, source: string): Promise<PluginInfo> {
  return invoke<PluginInfo>("ai_update_plugin", { id, manifest, source });
}

/** РЈРґР°Р»СЏРµС‚ РїР»Р°РіРёРЅ (С„Р°Р№Р»С‹ + РЅР°СЃС‚СЂРѕР№РєРё), РѕСЃС‚Р°РЅР°РІР»РёРІР°СЏ child-РїСЂРѕС†РµСЃСЃ. */
export function aiDeletePlugin(id: string): Promise<void> {
  return invoke<void>("ai_delete_plugin", { id });
}
