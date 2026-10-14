// IPC-команды AI-ассистента.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentId } from "../ids";
import type { Json } from "../../viewRegistry";
import type { PluginInfo } from "./plugins";
import type { UndoResult } from "./documents";

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
