// logPanel.ts вЂ” РЅРёР¶РЅРёР№ РґРѕРє (S11, ADR-0029/0030): РєРѕРЅС‚СЂРѕР»Р»РµСЂ СЂРµР¶РёРјРѕРІ
// В«Р–СѓСЂРЅР°Р» | РњРѕРЅРёС‚РѕСЂРёРЅРі | AI-Р°СЃСЃРёСЃС‚РµРЅС‚В» РїР»СЋСЃ С‚РµР»Рѕ СЂРµР¶РёРјР° В«Р–СѓСЂРЅР°Р»В» вЂ” ring buffer
// СЃРѕР±С‹С‚РёР№ РЅР° 200 Р·Р°РїРёСЃРµР№, РїРѕРґРїРёСЃРєР° РЅР° Tauri-СЃРѕР±С‹С‚РёСЏ, Р±РµР№РґР¶ РЅРµРїСЂРѕС‡РёС‚Р°РЅРЅС‹С….
// Р—Р°РїРёСЃРё СЂРµРЅРґРµСЂСЏС‚СЃСЏ С‚РѕР»СЊРєРѕ С‡РµСЂРµР· textContent (CSP/XSS).
//
// РЎРѕСЃС‚РѕСЏРЅРёСЏ РґРѕРєР°: hidden (РЅРµС‚ РІ layout, РґРµС„РѕР»С‚) / collapsed (РїРѕР»РѕСЃР°-С€Р°РїРєР°) /
// expanded (С‚РµР»Рѕ СЂРµР¶РёРјР° РІРёРґРЅРѕ). РЎРєСЂС‹С‚РёРµ вЂ” РєРЅРѕРїРєР° Г— (#logs-close) Рё РїСЂРѕРіСЂРµСЃСЃРёРІРЅС‹Р№
// Esc (СЂР°Р·РІС‘СЂРЅСѓС‚ в†’ СЃРІРµСЂРЅСѓС‚СЊ, СЃРІС‘СЂРЅСѓС‚ в†’ СЃРєСЂС‹С‚СЊ; РѕР±СЂР°Р±РѕС‚С‡РёРє вЂ” shell.ts).
//
// Р¤Р°Р·Р° 3 (В§8 РўР—): С„РёР»СЊС‚СЂ В«Р’СЃРµ/РћС€РёР±РєРёВ», РѕС‡РёСЃС‚РєР° Р»РµРЅС‚С‹, РіСЂСѓРїРїРёСЂРѕРІРєР° РїРѕ РґР°С‚Рµ,
// drag-resize РІС‹СЃРѕС‚С‹ Рё Р°РІС‚РѕРїСЂРѕРєСЂСѓС‚РєР° СЃ Р·Р°РјРѕСЂРѕР·РєРѕР№.

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";

/** РўСЏР¶РµСЃС‚СЊ Р·Р°РїРёСЃРё Р¶СѓСЂРЅР°Р»Р° (В§4.2 РўР—). */
export type LogLevel = "info" | "warn" | "error";

/** РљР°С‚РµРіРѕСЂРёСЏ вЂ” С†РІРµС‚ С‚РѕС‡РєРё Рё РїСЂРёРѕСЂРёС‚РµС‚ Р±РµР№РґР¶Р°. */
export type LogCategory = "plugin" | "document" | "system";

/** Р—Р°РїРёСЃСЊ Р¶СѓСЂРЅР°Р»Р°. `message` вЂ” С‡РµР»РѕРІРµРєРѕС‡РёС‚Р°РµРјС‹Р№ С‚РµРєСЃС‚, РЅРµ HTML. */
export interface LogEntry {
  id: number;
  timestamp: number;
  level: LogLevel;
  category: LogCategory;
  message: string;
  detail?: string;
}

export interface LogPanel {
  /** РўРµРєСѓС‰РµРµ СЃРѕСЃС‚РѕСЏРЅРёРµ РІРёРґРёРјРѕСЃС‚Рё РґРѕРєР° (hidden/collapsed/expanded). */
  state(): DockState;
  /** РўРµРєСѓС‰РёР№ Р°РєС‚РёРІРЅС‹Р№ СЂРµР¶РёРј РґРѕРєР°. */
  mode(): DockMode;
  /** РЎРІРµСЂРЅСѓС‚СЊ СЂР°СЃРєСЂС‹С‚С‹Р№ РґРѕРє РґРѕ РїРѕР»РѕСЃС‹-С€Р°РїРєРё. */
  collapse(moveFocus?: boolean): void;
  /** РџРѕР»РЅРѕСЃС‚СЊСЋ СЃРєСЂС‹С‚СЊ РґРѕРє (СѓР±СЂР°С‚СЊ РёР· layout). */
  hide(): void;


  toggleMode(next: DockMode): void;


  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
  dispose(): void;
}

/** Р РµР¶РёРј РЅРёР¶РЅРµРіРѕ РґРѕРєР° (ADR-0030): `ai` Р·Р°СЂРµР·РµСЂРІРёСЂРѕРІР°РЅ Рё РЅРµ Р°РєС‚РёРІРёСЂСѓРµС‚СЃСЏ. */
export type DockMode = "logs" | "mon" | "ai";

/** Р’РёРґРёРјРѕСЃС‚СЊ РґРѕРєР°: РЅРµС‚ РІ layout / РїРѕР»РѕСЃР°-С€Р°РїРєР° / С‚РµР»Рѕ СЂРµР¶РёРјР° РІРёРґРЅРѕ. */
export type DockState = "hidden" | "collapsed" | "expanded";

export interface LogPanelOptions {
  /** РўСЂРёРіРіРµСЂ В«Р–СѓСЂРЅР°Р»В» РІ СЃС‚Р°С‚СѓСЃР±Р°СЂРµ (#sb-logs): РїРѕР»СѓС‡Р°РµС‚ aria-expanded/.active. */
  trigger: HTMLElement;
  /** РўСЂРёРіРіРµСЂ В«РњРѕРЅРёС‚РѕСЂРёРЅРіВ» РІ СЃС‚Р°С‚СѓСЃР±Р°СЂРµ (#sb-mon). */
  monTrigger: HTMLElement;
  /** РўСЂРёРіРіРµСЂ В«AI-Р°СЃСЃРёСЃС‚РµРЅС‚В» РІ СЃС‚Р°С‚СѓСЃР±Р°СЂРµ (#sb-ai). */
  aiTrigger: HTMLElement;
  /** РўСЂРёРіРіРµСЂ В«AI-Р°СЃСЃРёСЃС‚РµРЅС‚В» РІ С‚СѓР»Р±Р°СЂРµ (#tb-ai). */
  aiToolbarTrigger: HTMLElement;
  /** Р‘РµР№РґР¶ РЅРµРїСЂРѕС‡РёС‚Р°РЅРЅС‹С… (#log-badge). */
  badge: HTMLElement;


  onLayoutChange?: () => void;


  onModeChange?: (mode: DockMode, state: DockState) => void;
}

/** Р¤РёР»СЊС‚СЂ Р»РµРЅС‚С‹ (В§4.2 РўР—, AC-8): В«РћС€РёР±РєРёВ» вЂ” С‚РѕР»СЊРєРѕ warn/error. */
type LogFilter = "all" | "error";

/** Р›РёРјРёС‚ ring buffer (В§4.3): РїРµСЂРµРїРѕР»РЅРµРЅРёРµ вЂ” РјРѕР»С‡Р°Р»РёРІРѕРµ FIFO-СѓРґР°Р»РµРЅРёРµ СЃС‚Р°СЂС€РёС…. */
const MAX_ENTRIES = 200;

/** Р“СЂР°РЅРёС†С‹ РІС‹СЃРѕС‚С‹ РїР°РЅРµР»Рё РїСЂРё drag-resize (В§6.1, AC-3). */
const MIN_HEIGHT = 28;
const MAX_HEIGHT = 520;

/** Р”РѕРїСѓСЃРє В«Р»РµРЅС‚Р° РїСЂРёР¶Р°С‚Р° Рє РЅРёР·СѓВ», px: Р±РѕР»СЊС€Рµ вЂ” СЃС‡РёС‚Р°РµРј, С‡С‚Рѕ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊ СѓС€С‘Р» РІС‹С€Рµ. */
const STICK_THRESHOLD = 4;

/** В«11 РѕРєС‚СЏР±СЂСЏВ» вЂ” С„РѕСЂРјР°С‚С‚РµСЂ СЃРѕР·РґР°С‘С‚СЃСЏ РѕРґРёРЅ СЂР°Р·: СЂРµРЅРґРµСЂ РёРґС‘С‚ РЅР° РєР°Р¶РґРѕРµ СЃРѕР±С‹С‚РёРµ. */
const DAY_FORMATTER = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "long" });


function byId<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`logs-panel: РЅРµ РЅР°Р№РґРµРЅ СЌР»РµРјРµРЅС‚ #${id}`);
  return el as T;
}

/** РљР»СЋС‡ РєР°Р»РµРЅРґР°СЂРЅРѕРіРѕ РґРЅСЏ РІ Р»РѕРєР°Р»СЊРЅРѕР№ Р·РѕРЅРµ вЂ” РіСЂСѓРїРїРёСЂРѕРІРєР° Р·Р°РїРёСЃРµР№ РїРѕ РґР°С‚Рµ (В§6.7). */
function dayKey(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

/** РљР»Р°РјРї РІС‹СЃРѕС‚С‹ РїР°РЅРµР»Рё (В§6.1): 28вЂ“520px. */
function clampHeight(value: number): number {
  return Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, Math.round(value)));
}

export function createLogPanel(opts: LogPanelOptions): LogPanel {
  const { trigger, monTrigger, aiTrigger, aiToolbarTrigger, badge } = opts;

  const panel = byId<HTMLElement>("logs-panel");
  const body = byId<HTMLElement>("logs-body");
  const list = byId<HTMLElement>("logs-list");
  const monBody = byId<HTMLElement>("mon-body");
  const aiBody = byId<HTMLElement>("ai-mode");
  const title = byId<HTMLElement>("logs-title");
  const caret = byId<HTMLButtonElement>("logs-collapse");
  const closeButton = byId<HTMLButtonElement>("logs-close");
  const count = byId<HTMLElement>("logs-count");
  const clearButton = byId<HTMLButtonElement>("logs-clear");
  const resizeBar = byId<HTMLElement>("logs-resize");
  const modeLogsBtn = byId<HTMLButtonElement>("mode-logs-btn");
  const modeMonBtn = byId<HTMLButtonElement>("mode-mon-btn");
  const modeAiBtn = byId<HTMLButtonElement>("mode-ai-btn");
  const filterButtons = Array.from(panel.querySelectorAll<HTMLButtonElement>(".filter-btn"));

  const entries: LogEntry[] = [];
  let nextId = 0;
  let unread = 0;
  let unreadErrors = 0;
  let unreadWarnings = 0;


  let open = false;
  let mode: DockMode = "logs";
  let filter: LogFilter = "all";
  let renderQueued = false;


  let stick = true;

  let programmaticScroll = false;


  let disposed = false;
  const unlisteners: UnlistenFn[] = [];



  function push(level: LogLevel, category: LogCategory, message: string, detail?: string): void {
    entries.push({ id: nextId++, timestamp: Date.now(), level, category, message, detail });
    if (entries.length > MAX_ENTRIES) entries.splice(0, entries.length - MAX_ENTRIES);


    if (!open || mode !== "logs") {
      unread++;
      if (level === "error") unreadErrors++;
      else if (level === "warn") unreadWarnings++;
      updateBadge();
    }
    scheduleRender();
  }



  function updateBadge(): void {
    if (unread > 0) {
      badge.hidden = false;
      badge.textContent = String(unread);
      badge.setAttribute("aria-label", `РќРµРїСЂРѕС‡РёС‚Р°РЅРЅС‹С… СЃРѕР±С‹С‚РёР№: ${unread}`);
      badge.className = "badge" + (unreadErrors > 0 ? " error" : unreadWarnings > 0 ? " warning" : "");
    } else {
      badge.hidden = true;
      badge.className = "badge";
      badge.textContent = "0";
    }
  }

  /** Р›РµРЅС‚Р° Р¶СѓСЂРЅР°Р»Р° РІРёРґРёРјР° вЂ” СЃРѕР±С‹С‚РёСЏ РїСЂРѕС‡РёС‚Р°РЅС‹: СЃР±СЂР°СЃС‹РІР°РµРј СЃС‡С‘С‚С‡РёРєРё Рё Р±РµР№РґР¶ (В§6.5). */
  function markRead(): void {
    unread = 0;
    unreadErrors = 0;
    unreadWarnings = 0;
    updateBadge();
  }



  /** Р—Р°РіРѕР»РѕРІРѕРє С€Р°РїРєРё РїРѕ СЂРµР¶РёРјСѓ вЂ” РѕРЅ Р¶Рµ accessible-РёРјСЏ СЂРµРіРёРѕРЅР° (aria-labelledby). */
  const MODE_TITLE: Record<DockMode, string> = {
    logs: "Р–СѓСЂРЅР°Р»",
    mon: "РњРѕРЅРёС‚РѕСЂРёРЅРі",
    ai: "AI-Р°СЃСЃРёСЃС‚РµРЅС‚",
  };


  function toggleModeFields(selector: string, show: boolean): void {
    for (const el of panel.querySelectorAll<HTMLElement>(selector)) el.hidden = !show;
  }


  function applyMode(): void {
    panel.dataset.mode = mode;
    title.textContent = MODE_TITLE[mode];
    const isLogs = mode === "logs";
    const isMon = mode === "mon";
    const isAi = mode === "ai";
    modeLogsBtn.classList.toggle("active", isLogs);
    modeMonBtn.classList.toggle("active", isMon);
    modeAiBtn.classList.toggle("active", isAi);
    modeLogsBtn.setAttribute("aria-selected", String(isLogs));
    modeMonBtn.setAttribute("aria-selected", String(isMon));
    modeAiBtn.setAttribute("aria-selected", String(isAi));
    body.hidden = !isLogs;
    monBody.hidden = !isMon;
    aiBody.hidden = !isAi;
    toggleModeFields(".logs-only", isLogs);
    toggleModeFields(".mon-only", isMon);
    toggleModeFields(".ai-only", isAi);
  }


  function setMode(next: DockMode): void {
    if (next === mode) return;
    mode = next;
    applyMode();

    if (open && next === "logs") markRead();
    syncTriggers();
    notifyLayout();
  }


  function state(): DockState {
    if (panel.hidden) return "hidden";
    return open ? "expanded" : "collapsed";
  }

  /** РўСЂРёРіРіРµСЂ СЃС‚Р°С‚СѓСЃР±Р°СЂР° Р°РєС‚РёРІРЅРѕРіРѕ СЂРµР¶РёРјР° вЂ” С‚РѕС‡РєР° РІРѕР·РІСЂР°С‚Р° С„РѕРєСѓСЃР° (В§4.5, ADR-0030 в„–10). */
  function activeTrigger(): HTMLElement {
    if (mode === "mon") return monTrigger;
    if (mode === "ai") return aiTrigger;
    return trigger;
  }


  function notifyLayout(): void {
    opts.onLayoutChange?.();
    opts.onModeChange?.(mode, state());
  }


  function syncTriggers(): void {
    const st = state();
    const logsActive = st === "expanded" && mode === "logs";
    const monActive = st === "expanded" && mode === "mon";
    const aiActive = st === "expanded" && mode === "ai";
    trigger.classList.toggle("active", logsActive);
    trigger.setAttribute("aria-expanded", String(logsActive));
    monTrigger.classList.toggle("active", monActive);
    monTrigger.setAttribute("aria-expanded", String(monActive));
    aiTrigger.classList.toggle("active", aiActive);
    aiTrigger.setAttribute("aria-expanded", String(aiActive));
    aiToolbarTrigger.classList.toggle("active", aiActive);
    aiToolbarTrigger.setAttribute("aria-expanded", String(aiActive));
  }


  function expand(moveFocus = true): void {
    if (!panel.hidden && open) return;
    panel.hidden = false;



    panel.style.height = "";
    panel.classList.remove("collapsed");
    open = true;
    caret.setAttribute("aria-expanded", "true");
    markRead();
    render();
    stick = true;
    scrollToBottom();
    syncTriggers();
    notifyLayout();


    if (moveFocus) {
      if (mode === "mon") modeMonBtn.focus({ preventScroll: true });
      else if (mode === "logs") modeLogsBtn.focus({ preventScroll: true });
    }
  }


  function collapse(moveFocus = true): void {
    if (panel.hidden || !open) return;
    endResize(); // РЅРµР·Р°РІРµСЂС€С‘РЅРЅС‹Р№ drag РЅРµ РґРѕР»Р¶РµРЅ РїРµСЂРµР¶РёС‚СЊ СЃРІС‘СЂС‚РєСѓ
    panel.style.height = "";
    panel.classList.add("collapsed");
    open = false;
    caret.setAttribute("aria-expanded", "false");
    syncTriggers();
    notifyLayout();

    if (moveFocus) activeTrigger().focus({ preventScroll: true });
  }


  function hide(moveFocus = true): void {
    if (panel.hidden) return;
    endResize();
    panel.hidden = true;
    panel.style.height = "";
    panel.classList.remove("collapsed", "resizing");
    open = false;
    caret.setAttribute("aria-expanded", "false");
    syncTriggers();
    notifyLayout();

    if (moveFocus) activeTrigger().focus({ preventScroll: true });
  }


  function openMode(next: DockMode): void {
    setMode(next);
    expand(true);
  }



  function triggerMode(next: DockMode): void {
    const st = state();
    if (st === "hidden" || st === "collapsed") {
      openMode(next);
      return;
    }
    if (mode === next) collapse();
    else setMode(next);
  }


  function toggleCollapse(): void {
    const st = state();
    if (st === "collapsed") expand(true);
    else if (st === "expanded") collapse(true);
  }



  function setFilter(next: LogFilter): void {
    if (filter === next) return;
    filter = next;
    for (const btn of filterButtons) {
      const isActive = btn.dataset.filter === next;
      btn.classList.toggle("active", isActive);
      btn.setAttribute("aria-pressed", String(isActive));
    }

    render();
  }

  function clearLog(): void {
    entries.length = 0;
    unread = 0;
    unreadErrors = 0;
    unreadWarnings = 0;
    updateBadge();
    stick = true;
    render();
  }



  function isNearBottom(): boolean {
    return body.scrollHeight - body.scrollTop - body.clientHeight <= STICK_THRESHOLD;
  }

  function scrollToBottom(): void {


    programmaticScroll = true;
    body.scrollTop = body.scrollHeight;


    requestAnimationFrame(() => {
      programmaticScroll = false;
    });
  }

  function onBodyScroll(): void {
    if (programmaticScroll) return;


    stick = isNearBottom();
  }



  function scheduleRender(): void {

    if (panel.hidden) return;
    if (renderQueued) return;
    renderQueued = true;
    requestAnimationFrame(() => {
      renderQueued = false;
      render();
    });
  }

  function render(): void {

    const visible = filter === "all" ? entries : entries.filter((entry) => entry.level !== "info");


    list.setAttribute("aria-live", "off");
    list.textContent = "";
    if (visible.length === 0) {
      list.appendChild(renderEmpty());
    } else {
      let currentDay = "";
      for (const entry of visible) {
        const day = dayKey(entry.timestamp);
        if (day !== currentDay) {
          currentDay = day;

          list.appendChild(renderDaySeparator(entry.timestamp));
        }
        list.appendChild(renderItem(entry));
      }
    }


    count.textContent =
      visible.length === entries.length
        ? String(visible.length)
        : `${visible.length} РёР· ${entries.length}`;
    if (stick) scrollToBottom();
    requestAnimationFrame(() => list.setAttribute("aria-live", "polite"));
  }

  function renderEmpty(): HTMLElement {
    const empty = document.createElement("div");
    empty.className = "log-empty";
    const title = document.createElement("div");
    title.className = "log-empty-title";
    title.textContent = filter === "error" ? "РћС€РёР±РѕРє Рё РїСЂРµРґСѓРїСЂРµР¶РґРµРЅРёР№ РЅРµС‚" : "РЎРѕР±С‹С‚РёР№ РїРѕРєР° РЅРµС‚";
    const hint = document.createElement("div");
    hint.className = "log-empty-hint";
    hint.textContent =
      filter === "error"
        ? "РџРµСЂРµРєР»СЋС‡РёС‚Рµ С„РёР»СЊС‚СЂ РЅР° В«Р’СЃРµВ», С‡С‚РѕР±С‹ СѓРІРёРґРµС‚СЊ РѕСЃС‚Р°Р»СЊРЅС‹Рµ Р·Р°РїРёСЃРё"
        : "РЎРѕР±С‹С‚РёСЏ РїРѕСЏРІСЏС‚СЃСЏ РїСЂРё СЂР°Р±РѕС‚Рµ РїР»Р°РіРёРЅРѕРІ Рё РґРѕРєСѓРјРµРЅС‚РѕРІ";
    empty.append(title, hint);
    return empty;
  }

  function renderDaySeparator(ts: number): HTMLElement {
    const separator = document.createElement("div");
    separator.className = "log-date-separator";
    separator.textContent = formatDay(ts);
    return separator;
  }

  function renderItem(entry: LogEntry): HTMLElement {
    const item = document.createElement("div");
    item.className = "log-item";
    item.dataset.category = entry.category;

    item.dataset.level = entry.level;

    const dot = document.createElement("span");
    dot.className =
      "log-dot " + (entry.level === "error" ? "error" : entry.level === "warn" ? "warning" : entry.category);
    dot.setAttribute("aria-hidden", "true");
    const time = document.createElement("span");
    time.className = "log-time";
    time.textContent = formatTime(entry.timestamp);
    const msg = document.createElement("span");
    msg.className = "log-msg";
    msg.textContent = entry.message;
    item.append(dot, time, msg);
    if (entry.detail) {
      const detail = document.createElement("span");
      detail.className = "log-detail";
      detail.textContent = entry.detail;
      item.append(detail);
    }
    return item;
  }

  function formatTime(ts: number): string {
    const d = new Date(ts);
    const hh = String(d.getHours()).padStart(2, "0");
    const mm = String(d.getMinutes()).padStart(2, "0");
    const ss = String(d.getSeconds()).padStart(2, "0");
    return `${hh}:${mm}:${ss}`;
  }

  function formatDay(ts: number): string {
    const now = new Date();
    if (dayKey(ts) === dayKey(now.getTime())) return "РЎРµРіРѕРґРЅСЏ";
    const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
    if (dayKey(ts) === dayKey(yesterday.getTime())) return "Р’С‡РµСЂР°";
    return DAY_FORMATTER.format(new Date(ts));
  }



  let resizePointer: number | null = null;
  let resizeStartY = 0;
  let resizeStartHeight = 0;

  function endResize(): void {
    if (resizePointer === null) return;
    resizePointer = null;
    panel.classList.remove("resizing");
    document.body.classList.remove("logs-resizing");
  }

  function onResizePointerDown(event: PointerEvent): void {
    if (event.button !== 0 || !event.isPrimary) return; // РѕСЃРЅРѕРІРЅР°СЏ РєРЅРѕРїРєР°, РѕСЃРЅРѕРІРЅРѕР№ СѓРєР°Р·Р°С‚РµР»СЊ
    resizePointer = event.pointerId;
    resizeStartY = event.clientY;
    resizeStartHeight = Math.max(panel.getBoundingClientRect().height, MIN_HEIGHT);

    resizeBar.setPointerCapture(event.pointerId);
    panel.classList.add("resizing");

    document.body.classList.add("logs-resizing");
    event.preventDefault();
  }

  function onResizePointerMove(event: PointerEvent): void {
    if (resizePointer !== event.pointerId) return;

    const next = clampHeight(resizeStartHeight + (resizeStartY - event.clientY));

    if (next > MIN_HEIGHT && !open) expand(false);
    panel.style.height = `${next}px`;
  }

  function onResizePointerUp(event: PointerEvent): void {
    if (resizePointer !== event.pointerId) return;

    const collapsed = panel.getBoundingClientRect().height <= MIN_HEIGHT + 0.5;
    endResize();


    if (collapsed) collapse(false);
  }

  function onResizePointerCancel(event: PointerEvent): void {
    if (resizePointer !== event.pointerId) return;
    endResize();
  }



  function onResizeLostCapture(): void {
    endResize();
  }



  function onFilterClick(event: MouseEvent): void {
    const btn = event.currentTarget;
    if (!(btn instanceof HTMLButtonElement)) return;
    const value = btn.dataset.filter;
    if (value === "all" || value === "error") setFilter(value);
  }



  const onLogsTriggerClick = (): void => triggerMode("logs");
  const onMonTriggerClick = (): void => triggerMode("mon");
  const onAiTriggerClick = (): void => triggerMode("ai");
  const onAiToolbarClick = (): void => triggerMode("ai");
  const onCaretClick = (): void => toggleCollapse();

  const onCloseClick = (): void => hide();
  const onModeLogsClick = (): void => setMode("logs");
  const onModeMonClick = (): void => setMode("mon");
  const onModeAiClick = (): void => setMode("ai");

  caret.addEventListener("click", onCaretClick);
  closeButton.addEventListener("click", onCloseClick);
  trigger.addEventListener("click", onLogsTriggerClick);
  monTrigger.addEventListener("click", onMonTriggerClick);
  aiTrigger.addEventListener("click", onAiTriggerClick);
  aiToolbarTrigger.addEventListener("click", onAiToolbarClick);
  modeLogsBtn.addEventListener("click", onModeLogsClick);
  modeMonBtn.addEventListener("click", onModeMonClick);
  modeAiBtn.addEventListener("click", onModeAiClick);
  clearButton.addEventListener("click", clearLog);
  resizeBar.addEventListener("pointerdown", onResizePointerDown);
  resizeBar.addEventListener("pointermove", onResizePointerMove);
  resizeBar.addEventListener("pointerup", onResizePointerUp);
  resizeBar.addEventListener("pointercancel", onResizePointerCancel);
  resizeBar.addEventListener("lostpointercapture", onResizeLostCapture);
  body.addEventListener("scroll", onBodyScroll);
  for (const btn of filterButtons) btn.addEventListener("click", onFilterClick);



  /** РџРѕРґРїРёСЃРєР° РЅР° Tauri-СЃРѕР±С‹С‚РёРµ СЃ СЃРѕС…СЂР°РЅРµРЅРёРµРј С„СѓРЅРєС†РёРё РѕС‚РїРёСЃРєРё. */
  function subscribe<T>(event: string, handler: (e: Event<T>) => void): void {
    void listen<T>(event, handler)
      .then((un) => {

        if (disposed) un();
        else unlisteners.push(un);
      })
      .catch((err) => console.error("listen:", event, err));
  }

  subscribe<{ plugin_id: string; text: string }>("plugin-message", (event) => {
    const { plugin_id, text } = event.payload;
    push("info", "plugin", `РџР»Р°РіРёРЅ В«${plugin_id}В»: ${text}`);
  });
  subscribe<{ doc_id: string; rev: number }>("document-updated", (event) => {
    push("info", "document", `Р”РѕРєСѓРјРµРЅС‚ РѕР±РЅРѕРІР»С‘РЅ (rev=${event.payload.rev})`);
  });
  subscribe<unknown>("plugins-changed", () => {
    push("info", "plugin", "РџР»Р°РіРёРЅС‹: РѕР±РЅРѕРІР»С‘РЅ СЃРїРёСЃРѕРє РёР»Рё СЃС‚Р°С‚СѓСЃ");
  });
  subscribe<unknown>("plugin-views-changed", () => {
    push("info", "system", "РџСЂРµРґСЃС‚Р°РІР»РµРЅРёСЏ РѕР±РЅРѕРІР»РµРЅС‹");
  });



  /** РџРµСЂРµС…РѕРґ СЃС‚Р°С‚СѓСЃР° РїР»Р°РіРёРЅР°: В«РїР»Р°РіРёРЅ X: active в†’ failedВ» (+ message РІ detail). */
  subscribe<{
    plugin_id: string;
    old_state: string;
    new_state: string;
    message?: string;
  }>("plugin-status-changed", (event) => {
    const { plugin_id, old_state, new_state, message } = event.payload;
    const level = new_state === "failed" ? "error" : new_state === "quarantined" ? "warn" : "info";
    push(level, "plugin", `РџР»Р°РіРёРЅ В«${plugin_id}В»: ${old_state} в†’ ${new_state}`, message);
  });

  /** Р”РµР№СЃС‚РІРёРµ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ РЅР°Рґ РґРѕРєСѓРјРµРЅС‚РѕРј: new/open/save/close (В§5.2). */
  subscribe<{
    type: "new" | "open" | "save" | "close";
    path?: string;
    rev?: number;
  }>("document-event", (event) => {
    const { type, path, rev } = event.payload;
    const name = path ? ` В«${path}В»` : "";
    let message: string;
    switch (type) {
      case "new":
        message = "РЎРѕР·РґР°РЅ РЅРѕРІС‹Р№ РґРѕРєСѓРјРµРЅС‚";
        break;
      case "open":
        message = `РћС‚РєСЂС‹С‚ С„Р°Р№Р»${name}`;
        break;
      case "save":
        message = `РЎРѕС…СЂР°РЅС‘РЅ С„Р°Р№Р»${name}`;
        break;
      case "close":
        message = `Р—Р°РєСЂС‹С‚ С„Р°Р№Р»${name}`;
        break;
    }
    push("info", "document", message, rev !== undefined ? `rev=${rev}` : undefined);
  });

  /** РЎРѕРѕР±С‰РµРЅРёРµ Р¶СѓСЂРЅР°Р»Р° РѕС‚ Rust (host.log РёР· Lua, РѕС€РёР±РєРё IPC). */
  subscribe<{
    level: "info" | "warn" | "error";
    category: "plugin" | "document" | "system";
    message: string;
  }>("log-entry", (event) => {
    const { level, category, message } = event.payload;
    push(level, category, message);
  });


  applyMode();
  syncTriggers();

  return {
    state,
    mode: () => mode,
    collapse,
    hide,
    toggleMode: triggerMode,
    log: push,
    dispose(): void {
      disposed = true; // РґРѕ РѕС‡РёСЃС‚РєРё: pending-listen СѓРІРёРґРёС‚ С„Р»Р°Рі Рё РѕС‚РїРёС€РµС‚СЃСЏ СЃР°Рј
      for (const un of unlisteners) un();
      unlisteners.length = 0;
      caret.removeEventListener("click", onCaretClick);
      closeButton.removeEventListener("click", onCloseClick);
      trigger.removeEventListener("click", onLogsTriggerClick);
      monTrigger.removeEventListener("click", onMonTriggerClick);
      aiTrigger.removeEventListener("click", onAiTriggerClick);
      aiToolbarTrigger.removeEventListener("click", onAiToolbarClick);
      modeLogsBtn.removeEventListener("click", onModeLogsClick);
      modeMonBtn.removeEventListener("click", onModeMonClick);
      modeAiBtn.removeEventListener("click", onModeAiClick);
      clearButton.removeEventListener("click", clearLog);
      resizeBar.removeEventListener("pointerdown", onResizePointerDown);
      resizeBar.removeEventListener("pointermove", onResizePointerMove);
      resizeBar.removeEventListener("pointerup", onResizePointerUp);
      resizeBar.removeEventListener("pointercancel", onResizePointerCancel);
      resizeBar.removeEventListener("lostpointercapture", onResizeLostCapture);
      body.removeEventListener("scroll", onBodyScroll);
      for (const btn of filterButtons) btn.removeEventListener("click", onFilterClick);
      endResize();
    },
  };
}
