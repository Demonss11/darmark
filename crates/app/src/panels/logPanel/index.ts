// logPanel/index.ts — нижний док (S11, ADR-0029/0030): контроллер режимов
// «Журнал | Мониторинг | AI-ассистент» плюс тело режима «Журнал» — ring buffer
// событий на 200 записей, подписка на Tauri-события, бейдж непрочитанных.
// Записи рендерятся только через textContent (CSP/XSS).
//
// Состояния дока: hidden (нет в layout, дефолт) / collapsed (полоса-шапка) /
// expanded (тело режима видно). Скрытие — кнопка × (#logs-close) и прогрессивный
// Esc (развёрнут → свернуть, свёрнут → скрыть; обработчик — shell.ts).
//
// Фаза 3 (§8 ТЗ): фильтр «Все/Ошибки», очистка ленты, группировка по дате,
// drag-resize высоты и автопрокрутка с заморозкой.

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import {
  MAX_ENTRIES,
  MIN_HEIGHT,
  STICK_THRESHOLD,
  DAY_FORMATTER,
  byId,
  dayKey,
  clampHeight,
} from "./helpers";
import type {
  LogLevel,
  LogCategory,
  LogEntry,
  LogPanel,
  LogPanelOptions,
  DockMode,
  DockState,
  LogFilter,
} from "./types";

export type { LogLevel, LogCategory, DockMode, DockState, LogPanel } from "./types";


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
      badge.setAttribute("aria-label", `Непрочитанных событий: ${unread}`);
      badge.className = "badge" + (unreadErrors > 0 ? " error" : unreadWarnings > 0 ? " warning" : "");
    } else {
      badge.hidden = true;
      badge.className = "badge";
      badge.textContent = "0";
    }
  }

  function markRead(): void {
    unread = 0;
    unreadErrors = 0;
    unreadWarnings = 0;
    updateBadge();
  }


  const MODE_TITLE: Record<DockMode, string> = {
    logs: "Журнал",
    mon: "Мониторинг",
    ai: "AI-ассистент",
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
    endResize();
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
        : `${visible.length} из ${entries.length}`;
    if (stick) scrollToBottom();
    requestAnimationFrame(() => list.setAttribute("aria-live", "polite"));
  }

  function renderEmpty(): HTMLElement {
    const empty = document.createElement("div");
    empty.className = "log-empty";
    const title = document.createElement("div");
    title.className = "log-empty-title";
    title.textContent = filter === "error" ? "Ошибок и предупреждений нет" : "Событий пока нет";
    const hint = document.createElement("div");
    hint.className = "log-empty-hint";
    hint.textContent =
      filter === "error"
        ? "Переключите фильтр на «Все», чтобы увидеть остальные записи"
        : "События появятся при работе плагинов и документов";
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
    if (dayKey(ts) === dayKey(now.getTime())) return "Сегодня";
    const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
    if (dayKey(ts) === dayKey(yesterday.getTime())) return "Вчера";
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
    if (event.button !== 0 || !event.isPrimary) return;
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
    push("info", "plugin", `Плагин «${plugin_id}»: ${text}`);
  });
  subscribe<{ doc_id: string; rev: number }>("document-updated", (event) => {
    push("info", "document", `Документ обновлён (rev=${event.payload.rev})`);
  });
  subscribe<unknown>("plugins-changed", () => {
    push("info", "plugin", "Плагины: обновлён список или статус");
  });
  subscribe<unknown>("plugin-views-changed", () => {
    push("info", "system", "Представления обновлены");
  });


  subscribe<{
    plugin_id: string;
    old_state: string;
    new_state: string;
    message?: string;
  }>("plugin-status-changed", (event) => {
    const { plugin_id, old_state, new_state, message } = event.payload;
    const level = new_state === "failed" ? "error" : new_state === "quarantined" ? "warn" : "info";
    push(level, "plugin", `Плагин «${plugin_id}»: ${old_state} → ${new_state}`, message);
  });

  subscribe<{
    type: "new" | "open" | "save" | "close";
    path?: string;
    rev?: number;
  }>("document-event", (event) => {
    const { type, path, rev } = event.payload;
    const name = path ? ` «${path}»` : "";
    let message: string;
    switch (type) {
      case "new":
        message = "Создан новый документ";
        break;
      case "open":
        message = `Открыт файл${name}`;
        break;
      case "save":
        message = `Сохранён файл${name}`;
        break;
      case "close":
        message = `Закрыт файл${name}`;
        break;
    }
    push("info", "document", message, rev !== undefined ? `rev=${rev}` : undefined);
  });

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
      disposed = true;
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
