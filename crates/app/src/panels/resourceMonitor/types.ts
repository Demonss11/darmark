// Типы мониторинга ресурсов.

import type { LogCategory, LogLevel } from "../logPanel";

interface ProcessMetrics {
  mem_bytes: number;
  cpu_percent?: number;
  handles?: number;
}

export type MainMetrics = ProcessMetrics;

export interface ChildMetrics extends ProcessMetrics {
  plugin_id: string;
  pid: number;
  mem_limit_bytes: number;
}

export interface ResourceSnapshot {
  ts: number;
  main: MainMetrics;
  children: ChildMetrics[];
}

export type MonState = "ok" | "warn" | "crit" | "orphan";

export interface OrphanEntry {
  pid: number;
  until: number;
}

export interface ResourceMonitorOptions {
  root: HTMLElement;
  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
}

export interface ResourceMonitor {
  refresh(): void;
  dispose(): void;
}
