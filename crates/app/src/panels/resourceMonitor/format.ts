// Форматирование объёма памяти.

import { MB } from "./constants";

export function mbValue(bytes: number): number {
  return Number.isFinite(bytes) && bytes > 0 ? Math.round(bytes / MB) : 0;
}

export function formatMb(bytes: number): string {
  return `${mbValue(bytes)} МБ`;
}
