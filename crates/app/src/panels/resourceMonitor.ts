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
import { pluginColor } from "../plugin/pluginColor";
import type { LogCategory, LogLevel } from "./logPanel";


interface ProcessMetrics {
  mem_bytes: number;
  cpu_percent?: number;
  handles?: number;
}

type MainMetrics = ProcessMetrics;

interface ChildMetrics extends ProcessMetrics {
  plugin_id: string;
  pid: number;
  mem_limit_bytes: number;
}

interface ResourceSnapshot {
  ts: number;
  main: MainMetrics;
  children: ChildMetrics[];
}

type MonState = "ok" | "warn" | "crit" | "orphan";


const MB = 1024 * 1024;
const CHILD_WARN_RATIO = 0.75;
const CHILD_CRIT_RATIO = 0.9;
const MAIN_WARN_BYTES = 800 * MB;
const MAIN_CRIT_BYTES = 1.5 * 1024 * MB;
const CHILD_CPU_WARN = 50;
const CHILD_CPU_CRIT = 80;
const MAIN_CPU_WARN = 70;
const MAIN_CPU_CRIT = 90;
const ORPHAN_TTL_MS = 5000;
const STALE_MS = 6000;
const TICK_MS = 1000;

const RANK: Record<MonState, number> = { ok: 0, warn: 1, crit: 2, orphan: 0 };

interface OrphanEntry {
  pid: number;
  until: number;
}

interface ResourceMonitorOptions {
  root: HTMLElement;
  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
}

export interface ResourceMonitor {
  refresh(): void;
  dispose(): void;
}

function mbValue(bytes: number): number {
  return Number.isFinite(bytes) && bytes > 0 ? Math.round(bytes / MB) : 0;
}

function formatMb(bytes: number): string {
  return `${mbValue(bytes)} МБ`;
}

export function createResourceMonitor(opts: ResourceMonitorOptions): ResourceMonitor {
  const { root, log } = opts;
  const panel = root.closest<HTMLElement>("#logs-panel");
  const monBody = document.getElementById("mon-body");
  const summary = document.getElementById("mon-summary");

  let snapshot: ResourceSnapshot | null = null;
  const lastChild = new Map<string, ChildMetrics>();
  const orphans = new Map<string, OrphanEntry>();
  const loggedState = new Map<string, MonState>();

  let ageEl: HTMLElement | null = null;
  let staleShown = false;
  let pendingRender = true;
  let disposed = false;
  const unlisteners: UnlistenFn[] = [];


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

  function evaluateSnapshot(snap: ResourceSnapshot): void {
    const main = mainState(snap.main);
    logTransition("proc:main", "ПЕНАЛ", "system", main.state, main.reason);
    for (const child of snap.children) {
      lastChild.set(child.plugin_id, child);
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


  function makeSpan(className: string): HTMLSpanElement {
    const el = document.createElement("span");
    el.className = className;
    return el;
  }

  function ageText(): string {
    if (!snapshot) return "нет данных";
    const secs = Math.max(0, Math.floor((Date.now() - snapshot.ts) / 1000));
    return `${secs} сек назад`;
  }

  function applyState(row: HTMLElement, state: MonState): void {
    if (state === "warn" || state === "crit") row.classList.add(state);
  }

  function mainTitle(main: MainMetrics, state: MonState): string {
    const parts = [`рабочий набор ${formatMb(main.mem_bytes)}`];
    if (main.cpu_percent !== undefined) parts.push(`CPU ${main.cpu_percent.toFixed(1)}%`);
    if (state !== "ok") {
      parts.push("перегруз");
      if (main.handles !== undefined) parts.push(`хендлы ${main.handles}`);
    }
    return parts.join(", ");
  }

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

  function emptyCell(className = ""): HTMLSpanElement {
    return makeSpan(className);
  }

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
    const bar = emptyCell();
    const cpu = makeSpan("mon-cpu");
    cpu.textContent = main.cpu_percent !== undefined ? `CPU ${Math.round(main.cpu_percent)}%` : "";
    row.append(dot, name, val, bar, cpu, age);
    applyState(row, state);
    row.title = mainTitle(main, state);
    return row;
  }

  function renderChildRow(child: ChildMetrics, orphan: boolean): HTMLElement {
    const row = makeSpan("mon-row mon-child mon-grid");
    row.dataset.pluginId = child.plugin_id;
    row.style.setProperty("--pc", pluginColor(child.plugin_id));

    const dot = makeSpan("mon-dot");
    dot.setAttribute("aria-hidden", "true");
    const name = makeSpan("mon-name");
    name.textContent = child.plugin_id;
    const val = makeSpan("mon-val");
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

  function renderEmpty(): HTMLElement {
    const box = makeSpan("mon-empty");
    const title = makeSpan("t");
    title.textContent = "Плагины не запущены";
    const hint = makeSpan("h");
    hint.textContent = "Мониторинг ресурсов редактора продолжается";
    box.append(title, hint);
    return box;
  }

  function renderChildren(): DocumentFragment {
    const frag = document.createDocumentFragment();
    const children = snapshot?.children ?? [];
    const byId = new Map<string, ChildMetrics>();
    for (const child of children) byId.set(child.plugin_id, child);
    const ids = new Set<string>([...byId.keys(), ...orphans.keys()]);
    if (ids.size === 0) {
      frag.appendChild(renderEmpty());
      return frag;
    }
    for (const id of [...ids].sort((a, b) => a.localeCompare(b))) {
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
    evaluateSnapshot(event.payload);
    renderAll();
  });

  subscribe<{ plugin_id: string; pid: number; reason?: string }>(
    "plugin-process-exited",
    (event) => {
      const { plugin_id, pid, reason } = event.payload;
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
      if (lastChild.has(plugin_id)) renderAll();
    }
  );


  function tick(): void {
    if (disposed) return;
    const now = Date.now();
    let expired = false;
    for (const [id, entry] of orphans) {
      if (now >= entry.until) {
        orphans.delete(id);
        lastChild.delete(id);
        loggedState.delete(`plugin:${id}`);
        expired = true;
      }
    }
    if (panel?.hidden || monBody?.hidden) {
      pendingRender = true;
      return;
    }
    const stale = snapshot !== null && now - snapshot.ts > STALE_MS;
    if (pendingRender || expired || stale !== staleShown) {
      renderAll();
      return;
    }
    if (ageEl && snapshot && !staleShown) {
      ageEl.textContent = ageText();
      renderSummary();
    }
  }

  const timer = window.setInterval(tick, TICK_MS);

  return {
    refresh(): void {
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
