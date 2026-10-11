// logPanel.ts — панель «Журнал» (S11, ADR-0029): ring buffer событий на 200
// записей, подписка на существующие Tauri-события, рендер записей только
// через textContent (без innerHTML в ленте — CSP/XSS), бейдж непрочитанных,
// своротка до полосы-шапки. Новые Rust-события (plugin-status-changed,
// document-event, log-entry) подключаются в Фазе 2; resize, фильтр,
// автопрокрутка и группировка по дате — Фаза 3.

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";

/** Тяжесть записи журнала (§4.2 ТЗ). */
export type LogLevel = "info" | "warn" | "error";

/** Категория — цвет точки и приоритет бейджа. */
export type LogCategory = "plugin" | "document" | "system";

/** Запись журнала. `message` — человекочитаемый текст, не HTML. */
export interface LogEntry {
  id: number;
  timestamp: number;
  level: LogLevel;
  category: LogCategory;
  message: string;
  detail?: string;
}

export interface LogPanel {
  toggle(): void;
  open(): void;
  collapse(): void;
  isOpen(): boolean;
  dispose(): void;
}

export interface LogPanelOptions {
  /** Триггер в статусбаре (#sb-logs): получает aria-expanded/.active. */
  trigger: HTMLElement;
  /** Бейдж непрочитанных (#log-badge). */
  badge: HTMLElement;
}

/** Лимит ring buffer (§4.3): переполнение — молчаливое FIFO-удаление старших. */
const MAX_ENTRIES = 200;

/** Получение элемента по id с проверкой: разметка панели — контракт, падение — сразу. */
function byId<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`logs-panel: не найден элемент #${id}`);
  return el as T;
}

export function createLogPanel(opts: LogPanelOptions): LogPanel {
  const { trigger, badge } = opts;

  const panel = byId<HTMLElement>("logs-panel");
  const body = byId<HTMLElement>("logs-body");
  const list = byId<HTMLElement>("logs-list");
  const caret = byId<HTMLButtonElement>("logs-collapse");
  const count = byId<HTMLElement>("logs-count");

  const entries: LogEntry[] = [];
  let nextId = 0;
  let unread = 0;
  let unreadErrors = 0;
  let unreadWarnings = 0;
  let open = false;
  let renderQueued = false;
  const unlisteners: UnlistenFn[] = [];

  // --- запись ---

  function push(level: LogLevel, category: LogCategory, message: string, detail?: string): void {
    entries.push({ id: nextId++, timestamp: Date.now(), level, category, message, detail });
    if (entries.length > MAX_ENTRIES) entries.splice(0, entries.length - MAX_ENTRIES);
    // Непрочитанные копятся только при свёрнутой панели: в развёрнутой пользователь
    // видит ленту в реальном времени.
    if (!open) {
      unread++;
      if (level === "error") unreadErrors++;
      else if (level === "warn") unreadWarnings++;
      updateBadge();
    }
    scheduleRender();
  }

  // --- бейдж непрочитанных (§6.5) ---

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

  // --- состояние панели (§4.5) ---

  function openPanel(): void {
    if (open) return;
    open = true;
    panel.hidden = false;
    panel.classList.remove("collapsed");
    trigger.classList.add("active");
    trigger.setAttribute("aria-expanded", "true");
    caret.setAttribute("aria-expanded", "true");
    // Панель развёрнута — события прочитаны: сбрасываем счётчики и бейдж.
    unread = 0;
    unreadErrors = 0;
    unreadWarnings = 0;
    updateBadge();
    render();
    body.scrollTop = body.scrollHeight;
  }

  function collapsePanel(): void {
    if (!open) return;
    open = false;
    panel.classList.add("collapsed");
    trigger.classList.remove("active");
    trigger.setAttribute("aria-expanded", "false");
    caret.setAttribute("aria-expanded", "false");
    // Единственная предсказуемая точка возврата фокуса — триггер в статусбаре.
    trigger.focus({ preventScroll: true });
  }

  function toggle(): void {
    if (open) collapsePanel();
    else openPanel();
  }

  // --- рендер (§4.3: по requestAnimationFrame; §4.6: только textContent) ---

  function scheduleRender(): void {
    // Панель скрыта — не пересоздаём DOM ленты; при открытии openPanel вызывает render() напрямую.
    if (panel.hidden) return;
    if (renderQueued) return;
    renderQueued = true;
    requestAnimationFrame(() => {
      renderQueued = false;
      render();
    });
  }

  function render(): void {
    list.textContent = "";
    if (entries.length === 0) {
      const empty = document.createElement("div");
      empty.className = "log-empty";
      empty.textContent = "Событий пока нет";
      list.appendChild(empty);
    } else {
      for (const entry of entries) list.appendChild(renderItem(entry));
    }
    count.textContent = String(entries.length);
  }

  function renderItem(entry: LogEntry): HTMLElement {
    const item = document.createElement("div");
    item.className = "log-item";
    item.dataset.category = entry.category;
    // Точка: уровень warn/error перекрывает цвет категории (§6.3).
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

  // --- обработчики панели ---

  caret.addEventListener("click", toggle);

  // --- события (Фаза 1: существующие, §5.1 ТЗ) ---

  /** Подписка на Tauri-событие с сохранением функции отписки. */
  function subscribe<T>(event: string, handler: (e: Event<T>) => void): void {
    void listen<T>(event, handler)
      .then((un) => unlisteners.push(un))
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

  return {
    toggle,
    open: openPanel,
    collapse: collapsePanel,
    isOpen: () => open,
    dispose(): void {
      for (const un of unlisteners) un();
      unlisteners.length = 0;
      caret.removeEventListener("click", toggle);
    },
  };
}
