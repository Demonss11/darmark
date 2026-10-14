// Типизированные обёртки над Tauri IPC. Логика (парсинг md, владение
// документами) — на Rust-стороне; здесь только сигнатуры и контракт ошибок.
import { invoke } from "@tauri-apps/api/core";
import type { DocumentId } from "./ids";
import type { Json } from "../viewRegistry";

export interface DocumentSnapshot {
  id: DocumentId;
  path: string | null;
  rev: number;
  text: string;
  dirty_hint: boolean;
}

interface DocMeta {
  id: DocumentId;
  path: string | null;
  rev: number;
  dirty_hint: boolean;
}

export interface RenderResult {
  html: string;
  rev: number;
  changed: boolean;
}

export function newDocument(text?: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("new_document", { text: text ?? null });
}

export function openDocument(path: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("open_document", { path });
}

export function updateDocument(
  id: DocumentId,
  text: string,
  mapped = false
): Promise<RenderResult> {
  return invoke<RenderResult>("update_document", { id, text, mapped });
}

export function renderDocument(id: DocumentId, mapped = false): Promise<RenderResult> {
  return invoke<RenderResult>("render_document", { id, mapped });
}

interface UndoResult {
  text: string;
  rev: number;
  start: number;
  end: number;
}

export function undoDocument(id: DocumentId): Promise<UndoResult> {
  return invoke<UndoResult>("undo_document", { id });
}

export function redoDocument(id: DocumentId): Promise<UndoResult> {
  return invoke<UndoResult>("redo_document", { id });
}

export function saveDocument(id: DocumentId, path?: string): Promise<DocMeta> {
  return invoke<DocMeta>("save_document", { id, path: path ?? null });
}

export function closeDocument(id: DocumentId): Promise<void> {
  return invoke<void>("close_document", { id });
}

export function documentSnapshot(id: DocumentId): Promise<DocumentSnapshot | null> {
  return invoke<DocumentSnapshot | null>("document_snapshot", { id });
}

export interface PluginViewInfo {
  view_id: string;
  plugin_id: string;
  kind: string;
  title: string;
  html: string;
}

export function pluginViews(): Promise<PluginViewInfo[]> {
  return invoke<PluginViewInfo[]>("plugin_views");
}

export function pluginViewAction(
  viewId: string,
  action: string,
  payload?: Json
): Promise<void> {
  return invoke<void>("plugin_view_action", { viewId, action, payload: payload ?? null });
}

export type PluginStatus =
  | { state: "stopped" }
  | { state: "active" }
  | { state: "quarantined" }
  | { state: "failed"; message: string };

interface PluginCommandInfo {
  id: string;
  title: string;
  keybinding: string | null;
}

export interface PluginSettingProperty {
  type?: string;
  title?: string;
  description?: string;
  default?: unknown;
  enum?: Json[];
}

interface PluginSettingsSchema {
  type?: string;
  properties?: Record<string, PluginSettingProperty>;
}

export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  status: PluginStatus;
  permissions: string[];
  granted_permissions: string[];
  enabled: boolean;
  commands: PluginCommandInfo[];
  notices: string[];
  settings_schema: PluginSettingsSchema | null;
  settings_values: Record<string, unknown>;
}

export function listPlugins(): Promise<PluginInfo[]> {
  return invoke<PluginInfo[]>("list_plugins");
}

export function setPluginEnabled(id: string, enabled: boolean): Promise<void> {
  return invoke<void>("set_plugin_enabled", { id, enabled });
}

export function setPluginPermissions(id: string, granted: string[]): Promise<void> {
  return invoke<void>("set_plugin_permissions", { id, granted });
}

export function setPluginSetting(id: string, key: string, value: Json): Promise<void> {
  return invoke<void>("set_plugin_setting", { id, key, value });
}

export function reloadPlugin(id: string): Promise<void> {
  return invoke<void>("reload_plugin", { id });
}

export function removePlugin(id: string): Promise<void> {
  return invoke<void>("remove_plugin", { id });
}

export function runPluginCommand(commandId: string): Promise<void> {
  return invoke<void>("run_plugin_command", { commandId });
}

export function openDevtools(): Promise<void> {
  return invoke<void>("open_devtools");
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    const message = (e as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(e);
}


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

export interface AiSettingsInput {
  enabled: boolean;
  provider: string;
  base_url: string;
  model: string;
  max_tokens: number;
  temperature: number;
  consent_document_send: boolean;
}

export interface AiMessage {
  role: "system" | "user" | "assistant" | "tool";
  content: string;
}

export interface AiUsage {
  prompt_tokens: number;
  completion_tokens: number;
}

interface AiResponse {
  message: AiMessage;
  usage: AiUsage | null;
  finish_reason: string | null;
}

export interface AiSkill {
  id: string;
  title: string;
  description: string;
  prompt_template: string;
  category: string;
}

interface AiTestResult {
  ok: boolean;
  model: string | null;
  error: string | null;
}

export function aiGetSettings(): Promise<AiSettingsView> {
  return invoke<AiSettingsView>("ai_get_settings");
}

export function aiSetSettings(settings: AiSettingsInput): Promise<void> {
  return invoke<void>("ai_set_settings", { settings });
}

export function aiSetApiKey(key: string): Promise<void> {
  return invoke<void>("ai_set_api_key", { key });
}

export function aiListSkills(): Promise<AiSkill[]> {
  return invoke<AiSkill[]>("ai_list_skills");
}

export function aiChat(messages: AiMessage[]): Promise<AiResponse> {
  return invoke<AiResponse>("ai_chat", { messages });
}

export function aiTestConnection(): Promise<AiTestResult> {
  return invoke<AiTestResult>("ai_test_connection");
}

export function aiRenderMarkdown(text: string): Promise<string> {
  return invoke<string>("ai_render_markdown", { text });
}


interface HistoryView {
  session_id: string;
  updated_at: number;
  messages: AiMessage[];
  warning: string | null;
}

export function aiHistoryLoad(): Promise<HistoryView> {
  return invoke<HistoryView>("ai_history_load");
}

export function aiHistorySave(messages: AiMessage[]): Promise<void> {
  return invoke<void>("ai_history_save", { messages });
}

export function aiHistoryClear(): Promise<void> {
  return invoke<void>("ai_history_clear");
}


interface AiEditOp {
  start: number;
  end: number;
  text: string;
}

export interface AiDiff {
  doc_id: string;
  base_rev: number;
  original: string;
  modified: string;
  ops: AiEditOp[];
}

interface AiValidationResult {
  ok: boolean;
  errors: string[];
  report: unknown | null;
}

export function aiDiff(docId: DocumentId, proposed: string): Promise<AiDiff> {
  return invoke<AiDiff>("ai_diff", { docId, proposed });
}

export function aiApplyEdit(
  docId: DocumentId,
  baseRev: number,
  ops: AiEditOp[]
): Promise<UndoResult> {
  return invoke<UndoResult>("ai_apply_edit", { docId, baseRev, ops });
}

export function aiValidatePlugin(
  id: string,
  manifest: Json,
  source: string
): Promise<AiValidationResult> {
  return invoke<AiValidationResult>("ai_validate_plugin", { id, manifest, source });
}

export function aiCreatePlugin(id: string, manifest: Json, source: string): Promise<PluginInfo> {
  return invoke<PluginInfo>("ai_create_plugin", { id, manifest, source });
}

export function aiUpdatePlugin(id: string, manifest: Json, source: string): Promise<PluginInfo> {
  return invoke<PluginInfo>("ai_update_plugin", { id, manifest, source });
}

export function aiDeletePlugin(id: string): Promise<void> {
  return invoke<void>("ai_delete_plugin", { id });
}
