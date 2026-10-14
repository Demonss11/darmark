// Константы и хелперы нижнего дока.

export const MAX_ENTRIES = 200;

export const MIN_HEIGHT = 28;
const MAX_HEIGHT = 520;

export const STICK_THRESHOLD = 4;

export const DAY_FORMATTER = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "long" });

export function byId<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`logs-panel: не найден элемент #${id}`);
  return el as T;
}

export function dayKey(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

export function clampHeight(value: number): number {
  return Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, Math.round(value)));
}
