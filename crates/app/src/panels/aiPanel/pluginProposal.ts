// Разбор предложенного плагина из ответа ассистента и подтверждение удаления.

import type { PluginStatus } from "../../core/tauri";
import type { Json } from "../../views/viewRegistry";
import { openDialog, type Dialog } from "../../ui/dialog";
import { actionButton, el } from "./dom";

export const PLUGIN_STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "активен",
  failed: "ошибка",
  quarantined: "карантин",
  stopped: "остановлен",
};

const PLUGIN_FENCE = /```json[^\n]*\n([\s\S]*?)```/gi;

export interface AiPluginProposal {
  id: string;
  name: string;
  version: string;
  source: string;
  manifest: Json;
}

export interface PluginCardState {
  id: string;
  isInstalled(): boolean;
  installed(label: string, stateClass: string): void;
  deleted(): void;
}

export function extractPluginProposal(content: string): AiPluginProposal | null {
  PLUGIN_FENCE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = PLUGIN_FENCE.exec(content)) !== null) {
    const proposal = parsePluginBlock(match[1]);
    if (proposal) return proposal;
  }
  return null;
}

function parsePluginBlock(raw: string): AiPluginProposal | null {
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!data || typeof data !== "object" || Array.isArray(data)) return null;
  const obj = data as Record<string, unknown>;
  const id = typeof obj.id === "string" ? obj.id.trim() : "";
  const source = typeof obj.source === "string" ? obj.source : "";
  if (!id || !source) return null;
  const name = typeof obj.name === "string" && obj.name.trim() ? obj.name.trim() : id;
  const version =
    typeof obj.version === "string" && obj.version.trim() ? obj.version.trim() : "0.1.0";
  const rawPermissions: unknown[] = Array.isArray(obj.permissions) ? obj.permissions : [];
  const permissions = rawPermissions.filter((p): p is string => typeof p === "string");
  const manifest: Json = { id, name, version, api_version: 1, entry: "main.lua", permissions };
  const contributes = obj.contributes;
  if (contributes && typeof contributes === "object" && !Array.isArray(contributes)) {
    manifest.contributes = contributes as Json;
  }
  return { id, name, version, source, manifest };
}

export function confirmDeletePlugin(pluginId: string): Promise<boolean> {
  return new Promise((resolve) => {
    const content = el("div", "ai-confirm");
    const text = el("p", "ai-confirm-text");
    text.textContent = `Плагин «${pluginId}» будет удалён безвозвратно: файлы и настройки.`;
    content.appendChild(text);

    const buttons = el("div", "dialog-actions");
    const cancel = actionButton("Отмена", "btn ghost");
    const confirm = actionButton("Удалить", "btn primary");
    buttons.append(cancel, confirm);
    content.appendChild(buttons);

    let dialog: Dialog | null = null;
    const finish = (value: boolean): void => {
      resolve(value);
      dialog?.close();
    };
    cancel.addEventListener("click", () => finish(false));
    confirm.addEventListener("click", () => finish(true));

    dialog = openDialog({
      title: "Удаление плагина",
      content,
      initialFocus: cancel,
      onClose: () => resolve(false),
    });
  });
}
