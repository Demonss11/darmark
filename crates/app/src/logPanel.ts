// logPanel.ts — панель «Журнал» (S11, ADR-0029): ring buffer событий на 200
// записей, подписка на существующие и новые Tauri-события, рендер записей только
// через textContent (без innerHTML в ленте — CSP/XSS), бейдж непрочитанных,
// своротка до полосы-шапки.
// Фаза 3 (§8 ТЗ): фильтр «Все/Ошибки», очистка ленты, группировка по дате,
// drag-resize высоты и автопрокрутка с заморозкой.

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

/** Фильтр ленты (§4.2 ТЗ, AC-8): «Ошибки» — только warn/error. */
type LogFilter = "all" | "error";

/** Лимит ring buffer (§4.3): переполнение — молчаливое FIFO-удаление старших. */
const MAX_ENTRIES = 200;

/** Границы высоты панели при drag-resize (§6.1, AC-3). */
const MIN_HEIGHT = 28;
const MAX_HEIGHT = 520;

/** Допуск «лента прижата к низу», px: больше — считаем, что пользователь ушёл выше. */
const STICK_THRESHOLD = 4;

/** «11 октября» — форматтер создаётся один раз: рендер идёт на каждое событие. */
const DAY_FORMATTER = new Intl.DateTimeFormat("ru-RU", { day: "numeric", month: "long" });

/** Получение элемента по id с проверкой: разметка панели — контракт, падение — сразу. */
function byId<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`logs-panel: не найден элемент #${id}`);
  return el as T;
}

/** Ключ календарного дня в локальной зоне — группировка записей по дате (§6.7). */
function dayKey(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

/** Кламп высоты панели (§6.1): 28–520px. */
function clampHeight(value: number): number {
  return Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, Math.round(value)));
}

export function createLogPanel(opts: LogPanelOptions): LogPanel {
  const { trigger, badge } = opts;

  const panel = byId<HTMLElement>("logs-panel");
  const body = byId<HTMLElement>("logs-body");
  const list = byId<HTMLElement>("logs-list");
  const caret = byId<HTMLButtonElement>("logs-collapse");
  const count = byId<HTMLElement>("logs-count");
  const clearButton = byId<HTMLButtonElement>("logs-clear");
  const resizeBar = byId<HTMLElement>("logs-resize");
  const filterButtons = Array.from(panel.querySelectorAll<HTMLButtonElement>(".filter-btn"));

  const entries: LogEntry[] = [];
  let nextId = 0;
  let unread = 0;
  let unreadErrors = 0;
  let unreadWarnings = 0;
  let open = false;
  let filter: LogFilter = "all";
  let renderQueued = false;
  // Автопрокрутка (AC-10): пока пользователь сам не прокрутил вверх, лента
  // «прилипает» к низу — новые записи остаются видимыми без чтения назад вверх.
  let stick = true;
  // Программный скролл (рендер, открытие) не должен выглядеть ручной прокруткой.
  let programmaticScroll = false;
  // Подписки асинхронны: если dispose() вызван раньше, чем промис listen()
  // разрешился, отписку забирает dispose — иначе будет утечка слушателя.
  let disposed = false;
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

  function expandedFilterButton(): HTMLButtonElement | undefined {
    return filterButtons.find((btn) => btn.dataset.filter === filter);
  }

  function setExpanded(next: boolean, moveFocus: boolean): void {
    if (open === next) return;
    open = next;
    panel.hidden = false;
    // Инлайн-высота принадлежит drag-resize: при программной смене состояния
    // возвращаемся к токенам высоты (иначе после драга до 28px панель
    // «развернулась» бы высотой полосы-шапки).
    panel.style.height = "";
    panel.classList.toggle("collapsed", !next);
    trigger.classList.toggle("active", next);
    trigger.setAttribute("aria-expanded", String(next));
    caret.setAttribute("aria-expanded", String(next));
    if (next) {
      // Панель развёрнута — события прочитаны: сбрасываем счётчики и бейдж.
      unread = 0;
      unreadErrors = 0;
      unreadWarnings = 0;
      updateBadge();
      render();
      stick = true;
      scrollToBottom();
      // §7: при открытии фокус уходит на фильтр; при resize не вырываем его из драга.
      if (moveFocus) expandedFilterButton()?.focus({ preventScroll: true });
    } else {
      // Единственная предсказуемая точка возврата фокуса — триггер в статусбаре.
      if (moveFocus) trigger.focus({ preventScroll: true });
    }
  }

  function openPanel(): void {
    if (open) return;
    setExpanded(true, true);
  }

  function collapsePanel(moveFocus = true): void {
    if (!open) return;
    endResize(); // незавершённый drag не должен пережить свёртку
    setExpanded(false, moveFocus);
  }

  function toggle(): void {
    if (open) collapsePanel();
    else openPanel();
  }

  // --- фильтр и очистка (AC-8, AC-9) ---

  function setFilter(next: LogFilter): void {
    if (filter === next) return;
    filter = next;
    for (const btn of filterButtons) {
      const isActive = btn.dataset.filter === next;
      btn.classList.toggle("active", isActive);
      btn.setAttribute("aria-pressed", String(isActive));
    }
    // Фильтр не трогает ring buffer (§4.3): записи скрываются только на рендере.
    render();
  }

  function clearLog(): void {
    // Опустошаем ленту и счётчики; подписки продолжают приносить новые записи.
    entries.length = 0;
    unread = 0;
    unreadErrors = 0;
    unreadWarnings = 0;
    updateBadge();
    stick = true;
    render();
  }

  // --- автопрокрутка с заморозкой (AC-10) ---

  function isNearBottom(): boolean {
    return body.scrollHeight - body.scrollTop - body.clientHeight <= STICK_THRESHOLD;
  }

  function scrollToBottom(): void {
    // Прямое присваивание `scrollTop` — мгновенный переход без анимации: при
    // prefers-reduced-motion движения не возникает, отдельная ветка не нужна.
    programmaticScroll = true;
    body.scrollTop = body.scrollHeight;
    // Сбрасываем флаг следующим кадром: scroll-события программного скролла
    // приходят до rAF и не должны выглядеть ручной прокруткой.
    requestAnimationFrame(() => {
      programmaticScroll = false;
    });
  }

  function onBodyScroll(): void {
    if (programmaticScroll) return;
    // Пользователь прокрутил вверх — замораживаем автопрокрутку; вернулся к низу —
    // прилипание включается снова.
    stick = isNearBottom();
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
    // «Ошибки» — только warn/error (§4.2): info отсекается, сам буфер не меняется.
    const visible = filter === "all" ? entries : entries.filter((entry) => entry.level !== "info");
    // Полная пересборка (§6.2/§7 ТЗ): глушим live-регион на время пересборки, чтобы
    // скринридер не перечитывал всю ленту заново; возвращаем polite после кадра.
    list.setAttribute("aria-live", "off");
    list.textContent = "";
    if (visible.length === 0) {
      list.appendChild(renderEmpty());
    } else {
      let currentDay = "";
      for (const entry of visible) {
        // Разделитель даты — перед первой записью каждого дня (§6.7).
        const day = dayKey(entry.timestamp);
        if (day !== currentDay) {
          currentDay = day;
          list.appendChild(renderDaySeparator(entry.timestamp));
        }
        list.appendChild(renderItem(entry));
      }
    }
    // Счётчик показывает видимые (отфильтрованные) записи; при активном фильтре —
    // «N из M», чтобы не терять общее число (§6.4).
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
    // Стабильный признак уровня для тестов/ассертов (вместо разбора классов точки).
    item.dataset.level = entry.level;
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

  function formatDay(ts: number): string {
    const now = new Date();
    if (dayKey(ts) === dayKey(now.getTime())) return "Сегодня";
    const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
    if (dayKey(ts) === dayKey(yesterday.getTime())) return "Вчера";
    return DAY_FORMATTER.format(new Date(ts));
  }

  // --- drag-resize (§6.1, AC-3): pointer events + pointer capture ---

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
    if (event.button !== 0 || !event.isPrimary) return; // основная кнопка, основной указатель
    resizePointer = event.pointerId;
    resizeStartY = event.clientY;
    resizeStartHeight = Math.max(panel.getBoundingClientRect().height, MIN_HEIGHT);
    // Захват указателя: дальнейшие move/up приходят на handle даже вне его.
    resizeBar.setPointerCapture(event.pointerId);
    panel.classList.add("resizing");
    // Иначе драг выделяет текст рабочей области под курсором.
    document.body.classList.add("logs-resizing");
    event.preventDefault();
  }

  function onResizePointerMove(event: PointerEvent): void {
    if (resizePointer !== event.pointerId) return;
    // Тянем верхнюю кромку: движение вверх увеличивает высоту.
    const next = clampHeight(resizeStartHeight + (resizeStartY - event.clientY));
    // Вытянули из полосы-шапки — панель разворачивается (без переноса фокуса).
    if (next > MIN_HEIGHT && !open) setExpanded(true, false);
    panel.style.height = `${next}px`;
  }

  function onResizePointerUp(event: PointerEvent): void {
    if (resizePointer !== event.pointerId) return;
    // Допуск на субпиксель: высоту задаём целым числом px, но замер может округлиться.
    const collapsed = panel.getBoundingClientRect().height <= MIN_HEIGHT + 0.5;
    endResize();
    // «Схлопнули» до минимума — это и есть свёрнутое состояние (§4.5). Фокус не
    // переносим: симметрично развороту драгом (moveFocus=false).
    if (collapsed) collapsePanel(false);
  }

  function onResizePointerCancel(event: PointerEvent): void {
    if (resizePointer !== event.pointerId) return;
    endResize();
  }

  // Страховка: если захват указателя потерян без pointerup/cancel (например, окно
  // потеряло фокус), состояние драга не должно «залипнуть».
  function onResizeLostCapture(): void {
    endResize();
  }

  // --- обработчики панели ---

  function onFilterClick(event: MouseEvent): void {
    const btn = event.currentTarget;
    if (!(btn instanceof HTMLButtonElement)) return;
    const value = btn.dataset.filter;
    if (value === "all" || value === "error") setFilter(value);
  }

  caret.addEventListener("click", toggle);
  clearButton.addEventListener("click", clearLog);
  resizeBar.addEventListener("pointerdown", onResizePointerDown);
  resizeBar.addEventListener("pointermove", onResizePointerMove);
  resizeBar.addEventListener("pointerup", onResizePointerUp);
  resizeBar.addEventListener("pointercancel", onResizePointerCancel);
  resizeBar.addEventListener("lostpointercapture", onResizeLostCapture);
  body.addEventListener("scroll", onBodyScroll);
  for (const btn of filterButtons) btn.addEventListener("click", onFilterClick);

  // --- события (Фаза 1: существующие, §5.1 ТЗ) ---

  /** Подписка на Tauri-событие с сохранением функции отписки. */
  function subscribe<T>(event: string, handler: (e: Event<T>) => void): void {
    void listen<T>(event, handler)
      .then((un) => {
        // dispose() уже прошёл — отписываемся немедленно, не копим висячий слушатель.
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

  // --- новые события (Фаза 2: Rust-события, §5.2 ТЗ, ADR-0029 §2.4) ---

  /** Переход статуса плагина: «плагин X: active → failed» (+ message в detail). */
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

  /** Действие пользователя над документом: new/open/save/close (§5.2). */
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

  /** Сообщение журнала от Rust (host.log из Lua, ошибки IPC). */
  subscribe<{
    level: "info" | "warn" | "error";
    category: "plugin" | "document" | "system";
    message: string;
  }>("log-entry", (event) => {
    const { level, category, message } = event.payload;
    push(level, category, message);
  });

  return {
    toggle,
    open: openPanel,
    collapse: collapsePanel,
    isOpen: () => open,
    dispose(): void {
      disposed = true; // до очистки: pending-listen увидит флаг и отпишется сам
      for (const un of unlisteners) un();
      unlisteners.length = 0;
      caret.removeEventListener("click", toggle);
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
