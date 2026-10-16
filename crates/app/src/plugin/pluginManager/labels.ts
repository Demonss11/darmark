

import type { PluginInfo, PluginStatus } from "../../core/tauri";

export const STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "активен",
  failed: "ошибка",
  quarantined: "карантин",
  stopped: "остановлен",
};

const QUARANTINE_THRESHOLD = 3;

const PERMISSION_LABELS: Record<string, string> = {
  "document:read": "Чтение документа",
  "document:write": "Правка документа",
  "view:create": "Своя вкладка в предпросмотре",
  "view:modify": "Запись в свою вкладку",
  "ui:statusbar": "Сообщения в статусбар",
};

export function permissionLabel(permission: string): string {
  return PERMISSION_LABELS[permission] ?? permission;
}

export function statusMessage(status: PluginStatus): string | null {
  return status.state === "failed" ? status.message : null;
}

export function stateHint(status: PluginStatus): string | null {
  if (status.state === "failed") return "Исправьте код плагина и нажмите «Перезагрузить».";
  if (status.state === "quarantined") {
    return `Автоотключён после ${QUARANTINE_THRESHOLD} падений подряд. Исправьте и включите тумблером — карантин сбросится.`;
  }
  return null;
}

export function documentBoundaryNotice(info: PluginInfo): string | null {
  return info.notices.find((notice) => notice.toLowerCase().includes("документ")) ?? null;
}
