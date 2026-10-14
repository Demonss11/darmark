// Типы нижнего дока.

export type LogLevel = "info" | "warn" | "error";

export type LogCategory = "plugin" | "document" | "system";

export interface LogEntry {
  id: number;
  timestamp: number;
  level: LogLevel;
  category: LogCategory;
  message: string;
  detail?: string;
}

export type DockMode = "logs" | "mon" | "ai";

export type DockState = "hidden" | "collapsed" | "expanded";

export interface LogPanel {
  state(): DockState;
  mode(): DockMode;
  collapse(moveFocus?: boolean): void;
  hide(): void;
  toggleMode(next: DockMode): void;
  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
  dispose(): void;
}

export interface LogPanelOptions {
  trigger: HTMLElement;
  monTrigger: HTMLElement;
  aiTrigger: HTMLElement;
  aiToolbarTrigger: HTMLElement;
  badge: HTMLElement;
  onLayoutChange?: () => void;
  onModeChange?: (mode: DockMode, state: DockState) => void;
}

export type LogFilter = "all" | "error";
