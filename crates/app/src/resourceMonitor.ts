// resourceMonitor.ts — вкладка «Мониторинг» дока (IDEA-012; ADR-0031): строки
// процессов `#mon-rows` внутри `#logs-resources` показывают ОЗУ
// главного процесса и каждого активного child'а плагина.
//
// Подписки — только на два host→front события: `resource-metrics` (снимок
// каждые 2 с) и `plugin-process-exited` (child утрачен). Рендер — исключительно
// `textContent` (CSP/XSS, AC-11): ни `innerHTML`, ни разбора чужих данных.
//
// Пороги (`.warn`/`.crit`, TZ §6.5) пишутся в журнал **на переход**, а не на
// каждый тик: владелец ленты — `logPanel` (шов `log`), монитор не дублирует
// подписку на `log-entry`. Состояние `orphan` (TZ §6.6) живёт 5 с и снимается,
// когда тот же `plugin_id` вернулся в свежий снимок с **новым** PID (рестарт).

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { pluginColor } from "./pluginColor";
import type { LogCategory, LogLevel } from "./logPanel";

// --- контракт снимка (TZ §5; зеркало Rust `plugins::resource`) ---

/** Метрика одного процесса в снимке `resource-metrics` (TZ §4.4). */
export interface ProcessMetrics {
  /** Главный — рабочий набор; child — private/commit, байты. */
  mem_bytes: number;
  /** CPU %, опционально (первый тик без предыдущего замера его не даёт). */
  cpu_percent?: number;
  /** Число хендлов; показывается только при превышении порога. */
  handles?: number;
}

/** Главный процесс: лимита Job Object нет, поэтому `mem_limit_bytes` отсутствует. */
export type MainMetrics = ProcessMetrics;

/** Child плагина: знаменатель порогов и прогресс-бара — лимит Job Object. */
export interface ChildMetrics extends ProcessMetrics {
  plugin_id: string;
  pid: number;
  mem_limit_bytes: number;
}

/** Снимок метрик (событие `resource-metrics`). `ts` — `Date.now()` на Rust-стороне. */
export interface ResourceSnapshot {
  ts: number;
  main: MainMetrics;
  children: ChildMetrics[];
}

/** Состояние строки (TZ §5): норма/предупреждение/критично/завершён. */
export type MonState = "ok" | "warn" | "crit" | "orphan";

// --- пороги и тайминги (TZ §6.4–§6.6) ---

/** Байт в мегабайте: «N МБ» = деление на 1024², как в ТЗ §6.3. */
const MB = 1024 * 1024;
/** Пороги плагина — доли лимита Job Object (`PrivateUsage`): warn 75%, crit 90%. */
const CHILD_WARN_RATIO = 0.75;
const CHILD_CRIT_RATIO = 0.9;
/** Пороги рабочего набора главного процесса: warn 800 МБ, crit 1.5 ГБ. */
const MAIN_WARN_BYTES = 800 * MB;
const MAIN_CRIT_BYTES = 1.5 * 1024 * MB;
/** Пороги CPU, % (адvisory — авто-kill'а нет, TZ §6.5). */
const CHILD_CPU_WARN = 50;
const CHILD_CPU_CRIT = 80;
const MAIN_CPU_WARN = 70;
const MAIN_CPU_CRIT = 90;
/** TTL `orphan`-строки, мс (TZ §6.6). */
const ORPHAN_TTL_MS = 5000;
/** Три пропущенных тика (2 с) — «нет данных», а не устаревшее число (TZ §6.4). */
const STALE_MS = 6000;
/** Тик локального обновления возраста и истечения `orphan`, мс. */
const TICK_MS = 1000;

/** Ранг тяжести — чтобы взять максимум из порогов по памяти и по CPU. */
const RANK: Record<MonState, number> = { ok: 0, warn: 1, crit: 2, orphan: 0 };

/** Осиротевшая строка: PID события и дедлайн удаления (TZ §6.6). */
interface OrphanEntry {
  pid: number;
  until: number;
}

export interface ResourceMonitorOptions {
  /** Контейнер строк `#mon-rows` во вкладке «Мониторинг» дока. */
  root: HTMLElement;
  /**
   * Шов записи в журнал: единственный владелец ленты — `logPanel`, монитор лишь
   * отдаёт готовую запись (без своей подписки на `log-entry`).
   */
  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
}

export interface ResourceMonitor {
  /** Догнать отложенный рендер после активации вкладки «Мониторинг». */
  refresh(): void;
  dispose(): void;
}

/** Округлённое значение в МБ (без единицы) — числитель формата «24 / 64 МБ». */
function mbValue(bytes: number): number {
  return Number.isFinite(bytes) && bytes > 0 ? Math.round(bytes / MB) : 0;
}

/** «64 МБ» — единица подставляется один раз на пару «commit / лимит» (ТЗ §6.3). */
function formatMb(bytes: number): string {
  return `${mbValue(bytes)} МБ`;
}

export function createResourceMonitor(opts: ResourceMonitorOptions): ResourceMonitor {
  const { root, log } = opts;
  // Границы видимости: при скрытом доке или неактивной вкладке «Мониторинг» DOM
  // не трогаем (TZ §3 сценарий 5); снимки продолжают копиться, рендер догоняет
  // при открытии (refresh() из logPanel или ближайший тик).
  const panel = root.closest<HTMLElement>("#logs-panel");
  const monBody = document.getElementById("mon-body");
  // Тихая сводка в шапке дока (режим «Мониторинг»); может отсутствовать в тестах.
  const summary = document.getElementById("mon-summary");

  let snapshot: ResourceSnapshot | null = null;
  /** Последние метрики child'а — нужны для `orphan`-строки после выхода из снимка. */
  const lastChild = new Map<string, ChildMetrics>();
  /** Осиротевшие плагины: `plugin_id` → (PID, дедлайн удаления). */
  const orphans = new Map<string, OrphanEntry>();
  /** Последний порог, о котором писали в журнал: дедуп «на переход» (TZ §6.5).
      Ключи в пространствах имён: `proc:main` и `plugin:<id>` — чтобы плагин с
      id `main` не делил ключ с главным процессом. */
  const loggedState = new Map<string, MonState>();

  /** Текущий узел `.mon-age`: по тику обновляем только его текст, без пересборки. */
  let ageEl: HTMLElement | null = null;
  /** Показываем ли сейчас «нет данных» — чтобы ловить переход границы устаревания. */
  let staleShown = false;
  /** Нужна ли полная пересборка (данные пришли/панель была скрыта). */
  let pendingRender = true;
  let disposed = false;
  const unlisteners: UnlistenFn[] = [];

  // --- вычисление состояния ---

  // Состояние главного процесса по рабочему набору и CPU (TZ §6.5, строгие `>`).
  function mainState(main: MainMetrics): { state: MonState; reason: string } {
    const mem: MonState =
      main.mem_bytes > MAIN_CRIT_BYTES ? "crit" : main.mem_bytes > MAIN_WARN_BYTES ? "warn" : "ok";
    const cpu: MonState =
      main.cpu_percent === undefined
        ? "ok"
        : main.cpu_percent > MAIN_CPU_CRIT
          ? "crit"
          : main.cpu_percent > MAIN_CPU_WARN
            ? "warn"
            : "ok";
    const state = RANK[mem] >= RANK[cpu] ? mem : cpu;
    const parts: string[] = [];
    if (mem !== "ok") parts.push(`память ${formatMb(main.mem_bytes)}`);
    if (cpu !== "ok" && main.cpu_percent !== undefined) {
      parts.push(`CPU ${Math.round(main.cpu_percent)}%`);
    }
    return { state, reason: parts.join(", ") };
  }

  // Состояние плагина по доле commit от лимита Job Object и CPU (TZ §6.5).
  function childState(child: ChildMetrics): { state: MonState; reason: string } {
    const ratio = child.mem_limit_bytes > 0 ? child.mem_bytes / child.mem_limit_bytes : 0;
    const mem: MonState =
      ratio >= CHILD_CRIT_RATIO ? "crit" : ratio >= CHILD_WARN_RATIO ? "warn" : "ok";
    const cpu: MonState =
      child.cpu_percent === undefined
        ? "ok"
        : child.cpu_percent > CHILD_CPU_CRIT
          ? "crit"
          : child.cpu_percent > CHILD_CPU_WARN
            ? "warn"
            : "ok";
    const state = RANK[mem] >= RANK[cpu] ? mem : cpu;
    const parts: string[] = [];
    if (mem !== "ok") {
      parts.push(
        `память ${Math.round(ratio * 100)}% лимита (${formatMb(child.mem_bytes)} из ${formatMb(child.mem_limit_bytes)})`
      );
    }
    if (cpu !== "ok" && child.cpu_percent !== undefined) {
      parts.push(`CPU ${Math.round(child.cpu_percent)}%`);
    }
    return { state, reason: parts.join(", ") };
  }

  /**
   * Запись в журнал **на переход** порога (TZ §6.5): `ok → warn/crit` — warn/error,
   * обратно — info. Повторение того же уровня не пишется (дедуп по `loggedState`).
   */
  function logTransition(
    key: string,
    name: string,
    category: LogCategory,
    next: MonState,
    reason: string
  ): void {
    const prev = loggedState.get(key) ?? "ok";
    if (prev === next) return;
    loggedState.set(key, next);
    if (next === "crit") {
      log("error", category, `${name}: критическое потребление ресурсов`, reason || undefined);
    } else if (next === "warn") {
      log("warn", category, `${name}: повышенное потребление ресурсов`, reason || undefined);
    } else {
      log("info", category, `${name}: потребление ресурсов в норме`);
    }
  }

  /**
   * Пересчёт состояний и журналирование переходов — **до и независимо** от рендера
   * (TZ §6.5, AC-5): при скрытой панели DOM не трогаем, но переходы порога `main`
   * и каждого child'а не теряются (logs-режимный слот, ADR-0030).
   */
  function evaluateSnapshot(snap: ResourceSnapshot): void {
    const main = mainState(snap.main);
    logTransition("proc:main", "ПЕНАЛ", "system", main.state, main.reason);
    for (const child of snap.children) {
      lastChild.set(child.plugin_id, child);
      // `orphan` снимается только по **новому** PID того же `plugin_id` (TZ §6.6).
      const orphan = orphans.get(child.plugin_id);
      if (orphan && orphan.pid !== child.pid) orphans.delete(child.plugin_id);
      const { state, reason } = childState(child);
      logTransition(
        `plugin:${child.plugin_id}`,
        `Плагин «${child.plugin_id}»`,
        "plugin",
        state,
        reason
      );
    }
  }

  // --- рендер (только textContent) ---

  function makeSpan(className: string): HTMLSpanElement {
    const el = document.createElement("span");
    el.className = className;
    return el;
  }

  // «N сек назад» по `ts` последнего снимка (TZ §6.4).
  function ageText(): string {
    if (!snapshot) return "нет данных";
    const secs = Math.max(0, Math.floor((Date.now() - snapshot.ts) / 1000));
    return `${secs} сек назад`;
  }

  function applyState(row: HTMLElement, state: MonState): void {
    if (state === "warn" || state === "crit") row.classList.add(state);
  }

  // Подсказка главного процесса: рабочий набор и CPU; хендли — лишь при пороге.
  function mainTitle(main: MainMetrics, state: MonState): string {
    const parts = [`рабочий набор ${formatMb(main.mem_bytes)}`];
    if (main.cpu_percent !== undefined) parts.push(`CPU ${main.cpu_percent.toFixed(1)}%`);
    if (state !== "ok") {
      parts.push("перегруз");
      if (main.handles !== undefined) parts.push(`хендлы ${main.handles}`);
    }
    return parts.join(", ");
  }

  // Подсказка плагина: commit из лимита Job Object, PID, CPU; хендлы — при пороге.
  function childTitle(child: ChildMetrics, state: MonState): string {
    const ratio =
      child.mem_limit_bytes > 0 ? Math.round((child.mem_bytes / child.mem_limit_bytes) * 100) : 0;
    const parts = [
      `commit ${formatMb(child.mem_bytes)} из ${formatMb(child.mem_limit_bytes)} (${ratio}%)`,
      `PID ${child.pid}`,
    ];
    if (child.cpu_percent !== undefined) parts.push(`CPU ${child.cpu_percent.toFixed(1)}%`);
    if (state !== "ok") {
      parts.push("перегруз");
      if (child.handles !== undefined) parts.push(`хендлы ${child.handles}`);
    }
    return parts.join(", ");
  }

  // Пустая ячейка сетки — держит колонки, когда значения нет (бар у главного).
  function emptyCell(className = ""): HTMLSpanElement {
    return makeSpan(className);
  }

  // Главная строка: `ПЕНАЛ 156 МБ · CPU 3% · 2 сек назад` (TZ §6.3).
  function renderMainRow(main: MainMetrics): HTMLElement {
    const { state } = mainState(main);

    const row = makeSpan("mon-row mon-main mon-grid");
    row.dataset.pid = "main";
    const dot = makeSpan("mon-dot");
    dot.setAttribute("aria-hidden", "true");
    const name = makeSpan("mon-name");
    name.textContent = "ПЕНАЛ";
    const val = makeSpan("mon-val");
    val.textContent = formatMb(main.mem_bytes);
    const age = makeSpan("mon-age");
    age.textContent = ageText();
    ageEl = age;
    // У главного процесса бара нет — колонка «Нагрузка» остаётся пустой.
    const bar = emptyCell();
    // CPU опционален: первый тик без предыдущего замера его не даёт (§4.4).
    const cpu = makeSpan("mon-cpu");
    cpu.textContent = main.cpu_percent !== undefined ? `CPU ${Math.round(main.cpu_percent)}%` : "";
    row.append(dot, name, val, bar, cpu, age);
    applyState(row, state);
    row.title = mainTitle(main, state);
    return row;
  }

  // Строка плагина: `● id 24 / 64 МБ ▮` (TZ §6.3). `orphan` — завершённый child:
  // бар погашен, строка удаляется по TTL (TZ §6.6).
  function renderChildRow(child: ChildMetrics, orphan: boolean): HTMLElement {
    const row = makeSpan("mon-row mon-child mon-grid");
    row.dataset.pluginId = child.plugin_id;
    // Цвет плагина (детерминированный из id) наследуют точка и бар (`--pc`, §6.2).
    row.style.setProperty("--pc", pluginColor(child.plugin_id));

    const dot = makeSpan("mon-dot");
    dot.setAttribute("aria-hidden", "true");
    const name = makeSpan("mon-name");
    name.textContent = child.plugin_id;
    const val = makeSpan("mon-val");
    // Числитель — commit без единицы, знаменатель — «64 МБ» (формат §6.3).
    val.textContent = `${mbValue(child.mem_bytes)} / ${formatMb(child.mem_limit_bytes)}`;
    const bar = emptyCell();
    const cpu = makeSpan("mon-cpu");
    const age = makeSpan("mon-age");

    if (orphan) {
      row.classList.add("orphan");
      cpu.classList.add("empty");
      cpu.textContent = "—";
      age.className = "mon-flag";
      age.textContent = "завершён";
      row.append(dot, name, val, bar, cpu, age);
      row.title = `Плагин ${child.plugin_id}: процесс завершён (PID ${child.pid})`;
      return row;
    }

    const { state } = childState(child);

    const barEl = makeSpan("mon-bar");
    barEl.setAttribute("aria-hidden", "true");
    const fill = document.createElement("i");
    const ratio =
      child.mem_limit_bytes > 0
        ? Math.min(1, Math.max(0, child.mem_bytes / child.mem_limit_bytes))
        : 0;
    fill.style.setProperty("--fill", `${Math.round(ratio * 100)}%`);
    barEl.appendChild(fill);
    bar.appendChild(barEl);
    if (child.cpu_percent !== undefined) {
      cpu.textContent = `CPU ${Math.round(child.cpu_percent)}%`;
    }
    row.append(dot, name, val, bar, cpu, age);
    applyState(row, state);
    row.title = childTitle(child, state);
    return row;
  }

  // Строка-заглушка «нет данных» при устаревшем снимке (TZ §6.4).
  function renderStale(): HTMLElement {
    const row = makeSpan("mon-row mon-main mon-grid");
    row.dataset.pid = "main";
    row.classList.add("mon-stale");
    const dot = makeSpan("mon-dot");
    dot.setAttribute("aria-hidden", "true");
    const name = makeSpan("mon-name");
    name.textContent = "ПЕНАЛ";
    const val = makeSpan("mon-val");
    val.textContent = "нет данных";
    const cpu = makeSpan("mon-cpu empty");
    cpu.textContent = "—";
    const age = makeSpan("mon-age");
    age.textContent = "нет данных";
    ageEl = age;
    row.append(dot, name, val, emptyCell(), cpu, age);
    return row;
  }

  // Пустое состояние: снимок есть, но активных плагинов нет (TZ §6.3).
  function renderEmpty(): HTMLElement {
    const box = makeSpan("mon-empty");
    const title = makeSpan("t");
    title.textContent = "Плагины не запущены";
    const hint = makeSpan("h");
    hint.textContent = "Мониторинг ресурсов редактора продолжается";
    box.append(title, hint);
    return box;
  }

  // Дети: активные из снимка + осиротевшие, порядок детерминирован (по `plugin_id`).
  function renderChildren(): DocumentFragment {
    const frag = document.createDocumentFragment();
    const children = snapshot?.children ?? [];
    const byId = new Map<string, ChildMetrics>();
    // Учёт `lastChild`/`orphans` и снятие `orphan` — в `evaluateSnapshot`, чтобы не
    // зависеть от видимости панели (TZ §6.6).
    for (const child of children) byId.set(child.plugin_id, child);
    const ids = new Set<string>([...byId.keys(), ...orphans.keys()]);
    if (ids.size === 0) {
      frag.appendChild(renderEmpty());
      return frag;
    }
    for (const id of [...ids].sort((a, b) => a.localeCompare(b))) {
      // `orphan` проверяем раньше снимка: выход child'а уже отмечен, но текущий
      // снимок может ещё содержать `plugin_id` — строка не должна «оживать» (§6.6).
      if (orphans.has(id)) {
        const last = lastChild.get(id) ?? byId.get(id);
        if (last) frag.appendChild(renderChildRow(last, true));
        continue;
      }
      const child = byId.get(id);
      if (child) frag.appendChild(renderChildRow(child, false));
    }
    return frag;
  }

  // Тихая сводка в шапке: `ПЕНАЛ 156 МБ · плагинов 2 · 2 сек назад` (TZ §6.4).
  function renderSummary(): void {
    if (!summary) return;
    if (!snapshot) {
      summary.textContent = "";
      return;
    }
    if (Date.now() - snapshot.ts > STALE_MS) {
      summary.textContent = "ПЕНАЛ — нет данных";
      return;
    }
    summary.textContent = `ПЕНАЛ ${formatMb(snapshot.main.mem_bytes)} · плагинов ${snapshot.children.length} · ${ageText()}`;
  }

  // Полная пересборка строк; при скрытом доке/вкладке — отметка об отложенном рендере.
  function renderAll(): void {
    if (panel?.hidden || monBody?.hidden) {
      pendingRender = true;
      return;
    }
    pendingRender = false;
    ageEl = null;
    root.textContent = "";
    renderSummary();
    if (!snapshot) {
      staleShown = false;
      return;
    }
    const stale = Date.now() - snapshot.ts > STALE_MS;
    staleShown = stale;
    if (stale) {
      root.appendChild(renderStale());
      return;
    }
    root.appendChild(renderMainRow(snapshot.main));
    root.appendChild(renderChildren());
  }

  // --- события host → front (TZ §4.4) ---

  // Подписка с сохранением отписки; `dispose()` до разрешения `listen` не течёт.
  function subscribe<T>(event: string, handler: (e: Event<T>) => void): void {
    void listen<T>(event, handler)
      .then((un) => {
        if (disposed) un();
        else unlisteners.push(un);
      })
      .catch((err) => console.error("listen:", event, err));
  }

  subscribe<ResourceSnapshot>("resource-metrics", (event) => {
    snapshot = event.payload;
    // Состояния/переходы считаем на каждом снимке (журнал не зависит от видимости
    // панели); DOM перерисовываем только при видимой панели (TZ §6.5, AC-5).
    evaluateSnapshot(event.payload);
    renderAll();
  });

  subscribe<{ plugin_id: string; pid: number; reason?: string }>(
    "plugin-process-exited",
    (event) => {
      const { plugin_id, pid, reason } = event.payload;
      // Идемпотентность: повторное событие того же PID не продлевает дедлайн.
      const existing = orphans.get(plugin_id);
      if (!existing || existing.pid !== pid) {
        orphans.set(plugin_id, { pid, until: Date.now() + ORPHAN_TTL_MS });
        log(
          "info",
          "plugin",
          `Плагин ${plugin_id}: процесс завершён (PID ${pid})`,
          reason ? `причина: ${reason}` : undefined
        );
      }
      // Плагин ещё не попадал в снимок — рисовать нечего (строка появится сама).
      if (lastChild.has(plugin_id)) renderAll();
    }
  );

  // --- локальный тик: возраст, истечение `orphan`, догон после открытия панели ---

  function tick(): void {
    if (disposed) return;
    const now = Date.now();
    let expired = false;
    for (const [id, entry] of orphans) {
      if (now >= entry.until) {
        orphans.delete(id);
        lastChild.delete(id);
        // Строка удалена — забываем и последний порог: рестарт начнёт с «нормы».
        loggedState.delete(`plugin:${id}`);
        expired = true;
      }
    }
    if (panel?.hidden || monBody?.hidden) {
      pendingRender = true;
      return;
    }
    const stale = snapshot !== null && now - snapshot.ts > STALE_MS;
    // Полная пересборка нужна на смене «данные/нет данных», при истечении orphan
    // и после скрытого дока/вкладки; иначе достаточно обновить возраст (TZ §6.4).
    if (pendingRender || expired || stale !== staleShown) {
      renderAll();
      return;
    }
    // Возраст обновляем только в не-stale строке: в «нет данных» строке `.mon-age`
    // тоже «нет данных», и реальное число не должно её перезаписывать (TZ §6.4, AC-8).
    if (ageEl && snapshot && !staleShown) {
      ageEl.textContent = ageText();
      renderSummary(); // сводка в шапке тикает синхронно со строкой
    }
  }

  const timer = window.setInterval(tick, TICK_MS);

  return {
    refresh(): void {
      // Догон после активации вкладки: невидимость не сбрасывает снимок, но
      // откладывает рендер (pendingRender), пока док/вкладка скрыты.
      if (panel?.hidden || monBody?.hidden) {
        pendingRender = true;
        return;
      }
      renderAll();
    },
    dispose(): void {
      disposed = true;
      window.clearInterval(timer);
      for (const un of unlisteners) un();
      unlisteners.length = 0;
      root.textContent = "";
    },
  };
}
