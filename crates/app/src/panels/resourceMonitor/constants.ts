// Пороговые константы мониторинга ресурсов.

import type { MonState } from "./types";

export const MB = 1024 * 1024;
export const CHILD_WARN_RATIO = 0.75;
export const CHILD_CRIT_RATIO = 0.9;
export const MAIN_WARN_BYTES = 800 * MB;
export const MAIN_CRIT_BYTES = 1.5 * 1024 * MB;
export const CHILD_CPU_WARN = 50;
export const CHILD_CPU_CRIT = 80;
export const MAIN_CPU_WARN = 70;
export const MAIN_CPU_CRIT = 90;
export const ORPHAN_TTL_MS = 5000;
export const STALE_MS = 6000;
export const TICK_MS = 1000;

export const RANK: Record<MonState, number> = { ok: 0, warn: 1, crit: 2, orphan: 0 };
