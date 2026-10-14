// IPC-команды плагинов.

import { invoke } from "@tauri-apps/api/core";
import type { Json } from "../../viewRegistry";

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
