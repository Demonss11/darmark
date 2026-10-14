// resourceMonitor.ts вЂ” РІРєР»Р°РґРєР° В«РњРѕРЅРёС‚РѕСЂРёРЅРіВ» РґРѕРєР° (IDEA-012; ADR-0031): СЃС‚СЂРѕРєРё
// РїСЂРѕС†РµСЃСЃРѕРІ `#mon-rows` РІРЅСѓС‚СЂРё `#logs-resources` РїРѕРєР°Р·С‹РІР°СЋС‚ РћР—РЈ
// РіР»Р°РІРЅРѕРіРѕ РїСЂРѕС†РµСЃСЃР° Рё РєР°Р¶РґРѕРіРѕ Р°РєС‚РёРІРЅРѕРіРѕ child'Р° РїР»Р°РіРёРЅР°.
//
// РџРѕРґРїРёСЃРєРё вЂ” С‚РѕР»СЊРєРѕ РЅР° РґРІР° hostв†’front СЃРѕР±С‹С‚РёСЏ: `resource-metrics` (СЃРЅРёРјРѕРє
// РєР°Р¶РґС‹Рµ 2 СЃ) Рё `plugin-process-exited` (child СѓС‚СЂР°С‡РµРЅ). Р РµРЅРґРµСЂ вЂ” РёСЃРєР»СЋС‡РёС‚РµР»СЊРЅРѕ
// `textContent` (CSP/XSS, AC-11): РЅРё `innerHTML`, РЅРё СЂР°Р·Р±РѕСЂР° С‡СѓР¶РёС… РґР°РЅРЅС‹С….
//
// РџРѕСЂРѕРіРё (`.warn`/`.crit`, TZ В§6.5) РїРёС€СѓС‚СЃСЏ РІ Р¶СѓСЂРЅР°Р» **РЅР° РїРµСЂРµС…РѕРґ**, Р° РЅРµ РЅР°
// РєР°Р¶РґС‹Р№ С‚РёРє: РІР»Р°РґРµР»РµС† Р»РµРЅС‚С‹ вЂ” `logPanel` (С€РѕРІ `log`), РјРѕРЅРёС‚РѕСЂ РЅРµ РґСѓР±Р»РёСЂСѓРµС‚
// РїРѕРґРїРёСЃРєСѓ РЅР° `log-entry`. РЎРѕСЃС‚РѕСЏРЅРёРµ `orphan` (TZ В§6.6) Р¶РёРІС‘С‚ 5 СЃ Рё СЃРЅРёРјР°РµС‚СЃСЏ,
// РєРѕРіРґР° С‚РѕС‚ Р¶Рµ `plugin_id` РІРµСЂРЅСѓР»СЃСЏ РІ СЃРІРµР¶РёР№ СЃРЅРёРјРѕРє СЃ **РЅРѕРІС‹Рј** PID (СЂРµСЃС‚Р°СЂС‚).

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { pluginColor } from "../plugin/pluginColor";
import type { LogCategory, LogLevel } from "./logPanel";



/** РњРµС‚СЂРёРєР° РѕРґРЅРѕРіРѕ РїСЂРѕС†РµСЃСЃР° РІ СЃРЅРёРјРєРµ `resource-metrics` (TZ В§4.4). */
export interface ProcessMetrics {
  /** Р“Р»Р°РІРЅС‹Р№ вЂ” СЂР°Р±РѕС‡РёР№ РЅР°Р±РѕСЂ; child вЂ” private/commit, Р±Р°Р№С‚С‹. */
  mem_bytes: number;
  /** CPU %, РѕРїС†РёРѕРЅР°Р»СЊРЅРѕ (РїРµСЂРІС‹Р№ С‚РёРє Р±РµР· РїСЂРµРґС‹РґСѓС‰РµРіРѕ Р·Р°РјРµСЂР° РµРіРѕ РЅРµ РґР°С‘С‚). */
  cpu_percent?: number;
  /** Р§РёСЃР»Рѕ С…РµРЅРґР»РѕРІ; РїРѕРєР°Р·С‹РІР°РµС‚СЃСЏ С‚РѕР»СЊРєРѕ РїСЂРё РїСЂРµРІС‹С€РµРЅРёРё РїРѕСЂРѕРіР°. */
  handles?: number;
}

/** Р“Р»Р°РІРЅС‹Р№ РїСЂРѕС†РµСЃСЃ: Р»РёРјРёС‚Р° Job Object РЅРµС‚, РїРѕСЌС‚РѕРјСѓ `mem_limit_bytes` РѕС‚СЃСѓС‚СЃС‚РІСѓРµС‚. */
export type MainMetrics = ProcessMetrics;

/** Child РїР»Р°РіРёРЅР°: Р·РЅР°РјРµРЅР°С‚РµР»СЊ РїРѕСЂРѕРіРѕРІ Рё РїСЂРѕРіСЂРµСЃСЃ-Р±Р°СЂР° вЂ” Р»РёРјРёС‚ Job Object. */
export interface ChildMetrics extends ProcessMetrics {
  plugin_id: string;
  pid: number;
  mem_limit_bytes: number;
}

/** РЎРЅРёРјРѕРє РјРµС‚СЂРёРє (СЃРѕР±С‹С‚РёРµ `resource-metrics`). `ts` вЂ” `Date.now()` РЅР° Rust-СЃС‚РѕСЂРѕРЅРµ. */
export interface ResourceSnapshot {
  ts: number;
  main: MainMetrics;
  children: ChildMetrics[];
}

/** РЎРѕСЃС‚РѕСЏРЅРёРµ СЃС‚СЂРѕРєРё (TZ В§5): РЅРѕСЂРјР°/РїСЂРµРґСѓРїСЂРµР¶РґРµРЅРёРµ/РєСЂРёС‚РёС‡РЅРѕ/Р·Р°РІРµСЂС€С‘РЅ. */
export type MonState = "ok" | "warn" | "crit" | "orphan";



/** Р‘Р°Р№С‚ РІ РјРµРіР°Р±Р°Р№С‚Рµ: В«N РњР‘В» = РґРµР»РµРЅРёРµ РЅР° 1024ВІ, РєР°Рє РІ РўР— В§6.3. */
const MB = 1024 * 1024;
/** РџРѕСЂРѕРіРё РїР»Р°РіРёРЅР° вЂ” РґРѕР»Рё Р»РёРјРёС‚Р° Job Object (`PrivateUsage`): warn 75%, crit 90%. */
const CHILD_WARN_RATIO = 0.75;
const CHILD_CRIT_RATIO = 0.9;
/** РџРѕСЂРѕРіРё СЂР°Р±РѕС‡РµРіРѕ РЅР°Р±РѕСЂР° РіР»Р°РІРЅРѕРіРѕ РїСЂРѕС†РµСЃСЃР°: warn 800 РњР‘, crit 1.5 Р“Р‘. */
const MAIN_WARN_BYTES = 800 * MB;
const MAIN_CRIT_BYTES = 1.5 * 1024 * MB;
/** РџРѕСЂРѕРіРё CPU, % (Р°Рґvisory вЂ” Р°РІС‚Рѕ-kill'Р° РЅРµС‚, TZ В§6.5). */
const CHILD_CPU_WARN = 50;
const CHILD_CPU_CRIT = 80;
const MAIN_CPU_WARN = 70;
const MAIN_CPU_CRIT = 90;
/** TTL `orphan`-СЃС‚СЂРѕРєРё, РјСЃ (TZ В§6.6). */
const ORPHAN_TTL_MS = 5000;
/** РўСЂРё РїСЂРѕРїСѓС‰РµРЅРЅС‹С… С‚РёРєР° (2 СЃ) вЂ” В«РЅРµС‚ РґР°РЅРЅС‹С…В», Р° РЅРµ СѓСЃС‚Р°СЂРµРІС€РµРµ С‡РёСЃР»Рѕ (TZ В§6.4). */
const STALE_MS = 6000;
/** РўРёРє Р»РѕРєР°Р»СЊРЅРѕРіРѕ РѕР±РЅРѕРІР»РµРЅРёСЏ РІРѕР·СЂР°СЃС‚Р° Рё РёСЃС‚РµС‡РµРЅРёСЏ `orphan`, РјСЃ. */
const TICK_MS = 1000;

/** Р Р°РЅРі С‚СЏР¶РµСЃС‚Рё вЂ” С‡С‚РѕР±С‹ РІР·СЏС‚СЊ РјР°РєСЃРёРјСѓРј РёР· РїРѕСЂРѕРіРѕРІ РїРѕ РїР°РјСЏС‚Рё Рё РїРѕ CPU. */
const RANK: Record<MonState, number> = { ok: 0, warn: 1, crit: 2, orphan: 0 };

/** РћСЃРёСЂРѕС‚РµРІС€Р°СЏ СЃС‚СЂРѕРєР°: PID СЃРѕР±С‹С‚РёСЏ Рё РґРµРґР»Р°Р№РЅ СѓРґР°Р»РµРЅРёСЏ (TZ В§6.6). */
interface OrphanEntry {
  pid: number;
  until: number;
}

export interface ResourceMonitorOptions {
  /** РљРѕРЅС‚РµР№РЅРµСЂ СЃС‚СЂРѕРє `#mon-rows` РІРѕ РІРєР»Р°РґРєРµ В«РњРѕРЅРёС‚РѕСЂРёРЅРіВ» РґРѕРєР°. */
  root: HTMLElement;


  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
}

export interface ResourceMonitor {
  /** Р”РѕРіРЅР°С‚СЊ РѕС‚Р»РѕР¶РµРЅРЅС‹Р№ СЂРµРЅРґРµСЂ РїРѕСЃР»Рµ Р°РєС‚РёРІР°С†РёРё РІРєР»Р°РґРєРё В«РњРѕРЅРёС‚РѕСЂРёРЅРіВ». */
  refresh(): void;
  dispose(): void;
}

/** РћРєСЂСѓРіР»С‘РЅРЅРѕРµ Р·РЅР°С‡РµРЅРёРµ РІ РњР‘ (Р±РµР· РµРґРёРЅРёС†С‹) вЂ” С‡РёСЃР»РёС‚РµР»СЊ С„РѕСЂРјР°С‚Р° В«24 / 64 РњР‘В». */
function mbValue(bytes: number): number {
  return Number.isFinite(bytes) && bytes > 0 ? Math.round(bytes / MB) : 0;
}

/** В«64 РњР‘В» вЂ” РµРґРёРЅРёС†Р° РїРѕРґСЃС‚Р°РІР»СЏРµС‚СЃСЏ РѕРґРёРЅ СЂР°Р· РЅР° РїР°СЂСѓ В«commit / Р»РёРјРёС‚В» (РўР— В§6.3). */
function formatMb(bytes: number): string {
  return `${mbValue(bytes)} РњР‘`;
}

export function createResourceMonitor(opts: ResourceMonitorOptions): ResourceMonitor {
  const { root, log } = opts;



  const panel = root.closest<HTMLElement>("#logs-panel");
  const monBody = document.getElementById("mon-body");

  const summary = document.getElementById("mon-summary");

  let snapshot: ResourceSnapshot | null = null;
  /** РџРѕСЃР»РµРґРЅРёРµ РјРµС‚СЂРёРєРё child'Р° вЂ” РЅСѓР¶РЅС‹ РґР»СЏ `orphan`-СЃС‚СЂРѕРєРё РїРѕСЃР»Рµ РІС‹С…РѕРґР° РёР· СЃРЅРёРјРєР°. */
  const lastChild = new Map<string, ChildMetrics>();
  /** РћСЃРёСЂРѕС‚РµРІС€РёРµ РїР»Р°РіРёРЅС‹: `plugin_id` в†’ (PID, РґРµРґР»Р°Р№РЅ СѓРґР°Р»РµРЅРёСЏ). */
  const orphans = new Map<string, OrphanEntry>();


  const loggedState = new Map<string, MonState>();

  /** РўРµРєСѓС‰РёР№ СѓР·РµР» `.mon-age`: РїРѕ С‚РёРєСѓ РѕР±РЅРѕРІР»СЏРµРј С‚РѕР»СЊРєРѕ РµРіРѕ С‚РµРєСЃС‚, Р±РµР· РїРµСЂРµСЃР±РѕСЂРєРё. */
  let ageEl: HTMLElement | null = null;
  /** РџРѕРєР°Р·С‹РІР°РµРј Р»Рё СЃРµР№С‡Р°СЃ В«РЅРµС‚ РґР°РЅРЅС‹С…В» вЂ” С‡С‚РѕР±С‹ Р»РѕРІРёС‚СЊ РїРµСЂРµС…РѕРґ РіСЂР°РЅРёС†С‹ СѓСЃС‚Р°СЂРµРІР°РЅРёСЏ. */
  let staleShown = false;
  /** РќСѓР¶РЅР° Р»Рё РїРѕР»РЅР°СЏ РїРµСЂРµСЃР±РѕСЂРєР° (РґР°РЅРЅС‹Рµ РїСЂРёС€Р»Рё/РїР°РЅРµР»СЊ Р±С‹Р»Р° СЃРєСЂС‹С‚Р°). */
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
    if (mem !== "ok") parts.push(`РїР°РјСЏС‚СЊ ${formatMb(main.mem_bytes)}`);
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
        `РїР°РјСЏС‚СЊ ${Math.round(ratio * 100)}% Р»РёРјРёС‚Р° (${formatMb(child.mem_bytes)} РёР· ${formatMb(child.mem_limit_bytes)})`
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
      log("error", category, `${name}: РєСЂРёС‚РёС‡РµСЃРєРѕРµ РїРѕС‚СЂРµР±Р»РµРЅРёРµ СЂРµСЃСѓСЂСЃРѕРІ`, reason || undefined);
    } else if (next === "warn") {
      log("warn", category, `${name}: РїРѕРІС‹С€РµРЅРЅРѕРµ РїРѕС‚СЂРµР±Р»РµРЅРёРµ СЂРµСЃСѓСЂСЃРѕРІ`, reason || undefined);
    } else {
      log("info", category, `${name}: РїРѕС‚СЂРµР±Р»РµРЅРёРµ СЂРµСЃСѓСЂСЃРѕРІ РІ РЅРѕСЂРјРµ`);
    }
  }



  function evaluateSnapshot(snap: ResourceSnapshot): void {
    const main = mainState(snap.main);
    logTransition("proc:main", "РџР•РќРђР›", "system", main.state, main.reason);
    for (const child of snap.children) {
      lastChild.set(child.plugin_id, child);

      const orphan = orphans.get(child.plugin_id);
      if (orphan && orphan.pid !== child.pid) orphans.delete(child.plugin_id);
      const { state, reason } = childState(child);
      logTransition(
        `plugin:${child.plugin_id}`,
        `РџР»Р°РіРёРЅ В«${child.plugin_id}В»`,
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
    if (!snapshot) return "РЅРµС‚ РґР°РЅРЅС‹С…";
    const secs = Math.max(0, Math.floor((Date.now() - snapshot.ts) / 1000));
    return `${secs} СЃРµРє РЅР°Р·Р°Рґ`;
  }

  function applyState(row: HTMLElement, state: MonState): void {
    if (state === "warn" || state === "crit") row.classList.add(state);
  }


  function mainTitle(main: MainMetrics, state: MonState): string {
    const parts = [`СЂР°Р±РѕС‡РёР№ РЅР°Р±РѕСЂ ${formatMb(main.mem_bytes)}`];
    if (main.cpu_percent !== undefined) parts.push(`CPU ${main.cpu_percent.toFixed(1)}%`);
    if (state !== "ok") {
      parts.push("РїРµСЂРµРіСЂСѓР·");
      if (main.handles !== undefined) parts.push(`С…РµРЅРґР»С‹ ${main.handles}`);
    }
    return parts.join(", ");
  }


  function childTitle(child: ChildMetrics, state: MonState): string {
    const ratio =
      child.mem_limit_bytes > 0 ? Math.round((child.mem_bytes / child.mem_limit_bytes) * 100) : 0;
    const parts = [
      `commit ${formatMb(child.mem_bytes)} РёР· ${formatMb(child.mem_limit_bytes)} (${ratio}%)`,
      `PID ${child.pid}`,
    ];
    if (child.cpu_percent !== undefined) parts.push(`CPU ${child.cpu_percent.toFixed(1)}%`);
    if (state !== "ok") {
      parts.push("РїРµСЂРµРіСЂСѓР·");
      if (child.handles !== undefined) parts.push(`С…РµРЅРґР»С‹ ${child.handles}`);
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
    name.textContent = "РџР•РќРђР›";
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
      cpu.textContent = "вЂ”";
      age.className = "mon-flag";
      age.textContent = "Р·Р°РІРµСЂС€С‘РЅ";
      row.append(dot, name, val, bar, cpu, age);
      row.title = `РџР»Р°РіРёРЅ ${child.plugin_id}: РїСЂРѕС†РµСЃСЃ Р·Р°РІРµСЂС€С‘РЅ (PID ${child.pid})`;
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
    name.textContent = "РџР•РќРђР›";
    const val = makeSpan("mon-val");
    val.textContent = "РЅРµС‚ РґР°РЅРЅС‹С…";
    const cpu = makeSpan("mon-cpu empty");
    cpu.textContent = "вЂ”";
    const age = makeSpan("mon-age");
    age.textContent = "РЅРµС‚ РґР°РЅРЅС‹С…";
    ageEl = age;
    row.append(dot, name, val, emptyCell(), cpu, age);
    return row;
  }


  function renderEmpty(): HTMLElement {
    const box = makeSpan("mon-empty");
    const title = makeSpan("t");
    title.textContent = "РџР»Р°РіРёРЅС‹ РЅРµ Р·Р°РїСѓС‰РµРЅС‹";
    const hint = makeSpan("h");
    hint.textContent = "РњРѕРЅРёС‚РѕСЂРёРЅРі СЂРµСЃСѓСЂСЃРѕРІ СЂРµРґР°РєС‚РѕСЂР° РїСЂРѕРґРѕР»Р¶Р°РµС‚СЃСЏ";
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
      summary.textContent = "РџР•РќРђР› вЂ” РЅРµС‚ РґР°РЅРЅС‹С…";
      return;
    }
    summary.textContent = `РџР•РќРђР› ${formatMb(snapshot.main.mem_bytes)} В· РїР»Р°РіРёРЅРѕРІ ${snapshot.children.length} В· ${ageText()}`;
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
          `РџР»Р°РіРёРЅ ${plugin_id}: РїСЂРѕС†РµСЃСЃ Р·Р°РІРµСЂС€С‘РЅ (PID ${pid})`,
          reason ? `РїСЂРёС‡РёРЅР°: ${reason}` : undefined
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
      renderSummary(); // СЃРІРѕРґРєР° РІ С€Р°РїРєРµ С‚РёРєР°РµС‚ СЃРёРЅС…СЂРѕРЅРЅРѕ СЃРѕ СЃС‚СЂРѕРєРѕР№
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
