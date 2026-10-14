// pluginManager.ts вЂ” РјРµРЅРµРґР¶РµСЂ РїР»Р°РіРёРЅРѕРІ РІ РґСЂРѕРІРµСЂРµ `#drawer-body` (H2, Р¤Р°Р·Р° 5).
//
// РџРѕРєР°Р·С‹РІР°РµС‚ СЃРЅРёРјРѕРє РїР»Р°РіРёРЅРѕРІ РёР· Rust-С…РѕСЃС‚Р° (`list_plugins`): СЃС‚Р°С‚СѓСЃ, С‚СѓРјР±Р»РµСЂ
// РІРєР»/РІС‹РєР», В«РџРµСЂРµР·Р°РіСЂСѓР·РёС‚СЊВ», РёРЅР»Р°Р№РЅ-СЃРѕРіР»Р°СЃРёРµ РЅР° permissions (В§11.1 Рї.4),
// РєРЅРѕРїРєРё РєРѕРјР°РЅРґ Рё С„РѕСЂРјСѓ РЅР°СЃС‚СЂРѕРµРє РїРѕ `contributes.settings` (S5b). РўРµР»Рѕ РїР°РЅРµР»Рё
// СЃРѕР±РёСЂР°РµС‚СЃСЏ DOM-СѓР·Р»Р°РјРё Рё `textContent` вЂ” СЃС‚СЂРѕРєРё РёР· РјР°РЅРёС„РµСЃС‚Р°/С…РѕСЃС‚Р° РЅРµ РїСЂРѕС…РѕРґСЏС‚
// С‡РµСЂРµР· `innerHTML`.
//
// РЎРѕСЃС‚Р°РІ Рё СЃС‚Р°С‚СѓСЃС‹ РјРµРЅСЏСЋС‚СЃСЏ РїРѕ РёРЅРёС†РёР°С‚РёРІРµ С…РѕСЃС‚Р°: СЃРѕР±С‹С‚РёРµ `plugins-changed` (Р±РµР·
// payload) Р·Р°СЃС‚Р°РІР»СЏРµС‚ РїРµСЂРµС‡РёС‚Р°С‚СЊ СЃРЅРёРјРѕРє. РћР±СЂР°Р±РѕС‚С‡РёРєРё РґРµР»РµРіРёСЂРѕРІР°РЅС‹ РєРѕСЂРЅСЋ, РїРѕСЌС‚РѕРјСѓ
// РїРµСЂРµСЂРёСЃРѕРІРєР° СЃРїРёСЃРєР° РЅРµ РѕСЃС‚Р°РІР»СЏРµС‚ РІРёСЃСЏС‡РёС… СЃР»СѓС€Р°С‚РµР»РµР№.

import { listen } from "@tauri-apps/api/event";
import {
  errorMessage,
  listPlugins,
  reloadPlugin,
  removePlugin,
  runPluginCommand,
  setPluginEnabled,
  setPluginPermissions,
  setPluginSetting,
  type PluginInfo,
  type PluginSettingProperty,
  type PluginStatus,
} from "../core/tauri";
import { openDialog, type Dialog } from "../ui/dialog";
import { ownerPluginId } from "../plugin/pluginTarget";
import { pluginColor } from "../plugin/pluginColor";
import type { Json } from "../viewRegistry";

/** РџСѓР±Р»РёС‡РЅС‹Р№ С„Р°СЃР°Рґ РєРѕРЅС‚СЂРѕР»Р»РµСЂР° РјРµРЅРµРґР¶РµСЂР° РїР»Р°РіРёРЅРѕРІ. */
export interface PluginManager {
  /** РџРµСЂРµС‡РёС‚Р°С‚СЊ СЃРїРёСЃРѕРє РїР»Р°РіРёРЅРѕРІ Рё РїРµСЂРµСЂРёСЃРѕРІР°С‚СЊ РїР°РЅРµР»СЊ. */
  refresh(): Promise<void>;


  reloadForDocument(path: string | null): Promise<void>;
  /** РЎРЅСЏС‚СЊ СЃР»СѓС€Р°С‚РµР»СЊ СЃРѕР±С‹С‚РёСЏ `plugins-changed`. */
  dispose(): void;
}

export interface PluginManagerOptions {
  /** РљРѕРЅС‚РµР№РЅРµСЂ РјРµРЅРµРґР¶РµСЂР° (`#plugin-manager` РІРЅСѓС‚СЂРё `#drawer-body`). */
  root: HTMLElement;
  /** РЎРѕРѕР±С‰РµРЅРёРµ РІ СЃС‚Р°С‚СѓСЃР±Р°СЂ (РѕС€РёР±РєРё IPC/РґРµР№СЃС‚РІРёР№). */
  status(msg: string): void;
  /** РЎРІРµР¶РёР№ СЃРЅРёРјРѕРє РїР»Р°РіРёРЅРѕРІ РїРѕСЃР»Рµ СѓСЃРїРµС€РЅРѕРіРѕ `list_plugins` (РґР»СЏ СЃС‚Р°С‚СѓСЃР±Р°СЂР°). */
  onPlugins?(list: PluginInfo[]): void;
}

const STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "Р°РєС‚РёРІРµРЅ",
  failed: "РѕС€РёР±РєР°",
  quarantined: "РєР°СЂР°РЅС‚РёРЅ",
  stopped: "РѕСЃС‚Р°РЅРѕРІР»РµРЅ",
};



const QUARANTINE_THRESHOLD = 3;



const PERMISSION_LABELS: Record<string, string> = {
  "document:read": "Р§С‚РµРЅРёРµ РґРѕРєСѓРјРµРЅС‚Р°",
  "document:write": "РџСЂР°РІРєР° РґРѕРєСѓРјРµРЅС‚Р°",
  "view:create": "РЎРІРѕСЏ РІРєР»Р°РґРєР° РІ РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂРµ",
  "view:modify": "Р—Р°РїРёСЃСЊ РІ СЃРІРѕСЋ РІРєР»Р°РґРєСѓ",
  "ui:statusbar": "РЎРѕРѕР±С‰РµРЅРёСЏ РІ СЃС‚Р°С‚СѓСЃР±Р°СЂ",
};

/** Р§РµР»РѕРІРµРєРѕС‡РёС‚Р°РµРјРѕРµ РѕРїРёСЃР°РЅРёРµ РїСЂР°РІР°; С„РѕР»Р±СЌРє вЂ” СЃС‹СЂРѕР№ С‚РѕРєРµРЅ. */
function permissionLabel(permission: string): string {
  return PERMISSION_LABELS[permission] ?? permission;
}

function statusMessage(status: PluginStatus): string | null {
  return status.state === "failed" ? status.message : null;
}



function stateHint(status: PluginStatus): string | null {
  if (status.state === "failed") return "РСЃРїСЂР°РІСЊС‚Рµ РєРѕРґ РїР»Р°РіРёРЅР° Рё РЅР°Р¶РјРёС‚Рµ В«РџРµСЂРµР·Р°РіСЂСѓР·РёС‚СЊВ».";
  if (status.state === "quarantined") {
    return `РђРІС‚РѕРѕС‚РєР»СЋС‡С‘РЅ РїРѕСЃР»Рµ ${QUARANTINE_THRESHOLD} РїР°РґРµРЅРёР№ РїРѕРґСЂСЏРґ. РСЃРїСЂР°РІСЊС‚Рµ Рё РІРєР»СЋС‡РёС‚Рµ С‚СѓРјР±Р»РµСЂРѕРј вЂ” РєР°СЂР°РЅС‚РёРЅ СЃР±СЂРѕСЃРёС‚СЃСЏ.`;
  }
  return null;
}



function documentBoundaryNotice(info: PluginInfo): string | null {
  return info.notices.find((notice) => notice.toLowerCase().includes("РґРѕРєСѓРјРµРЅС‚")) ?? null;
}



function settingValue(info: PluginInfo, key: string, prop: PluginSettingProperty): unknown {
  const stored = info.settings_values[key];
  return stored !== undefined ? stored : prop.default;
}



function renderSettingRow(
  key: string,
  prop: PluginSettingProperty,
  control: HTMLInputElement | HTMLSelectElement
): HTMLElement {
  const row = el("div", "pl-setting");
  const head = el("label", "pl-setting-head");
  const label = el("span", "pl-setting-label");
  label.textContent = prop.title ?? key;
  control.dataset.settingKey = key;
  head.append(label, control);
  row.append(head);
  if (prop.description) {
    const desc = el("div", "pl-setting-desc");
    desc.textContent = prop.description;
    row.append(desc);
  }
  return row;
}

function el(tag: string, className?: string): HTMLElement {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
}

/** РЈРЅРёРєР°Р»СЊРЅС‹Р№ id РїРѕРїРѕРІРµСЂР° РёРЅС„РѕСЂРјР°С†РёРё вЂ” СЃРІСЏР·С‹РІР°РµС‚ РєРЅРѕРїРєСѓ (`aria-controls`) СЃ СЃРѕРґРµСЂР¶РёРјС‹Рј. */
function infoPopoverId(pluginId: string): string {
  return `pl-info-pop-${pluginId}`;
}

/** РЈРЅРёРєР°Р»СЊРЅС‹Р№ id С‚РµР»Р° РєР°СЂС‚РѕС‡РєРё вЂ” СЃРІСЏР·С‹РІР°РµС‚ С€РµРІСЂРѕРЅ (`aria-controls`) СЃ Р°РєРєРѕСЂРґРµРѕРЅРѕРј. */
function bodyId(pluginId: string): string {
  return `pl-body-${pluginId}`;
}



function infoIcon(): SVGSVGElement {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("width", "14");
  svg.setAttribute("height", "14");
  svg.setAttribute("aria-hidden", "true");

  const circle = document.createElementNS(NS, "circle");
  circle.setAttribute("cx", "8");
  circle.setAttribute("cy", "8");
  circle.setAttribute("r", "7");
  circle.setAttribute("fill", "none");
  circle.setAttribute("stroke", "currentColor");
  circle.setAttribute("stroke-width", "1.5");

  const text = document.createElementNS(NS, "text");
  text.setAttribute("x", "8");
  text.setAttribute("y", "11.5");
  text.setAttribute("text-anchor", "middle");
  text.setAttribute("font-size", "10");
  text.setAttribute("fill", "currentColor");
  text.setAttribute("font-family", "inherit");
  text.textContent = "i";

  svg.append(circle, text);
  return svg;
}



function chevronIcon(): SVGSVGElement {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");

  const polyline = document.createElementNS(NS, "polyline");
  polyline.setAttribute("points", "9 6 15 12 9 18");

  svg.append(polyline);
  return svg;
}



function shieldIcon(): SVGSVGElement {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", "14");
  svg.setAttribute("height", "14");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.8");

  const outline = document.createElementNS(NS, "path");
  outline.setAttribute("d", "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z");
  const check = document.createElementNS(NS, "polyline");
  check.setAttribute("points", "9 12 11 14 15 10");

  svg.append(outline, check);
  return svg;
}



function renderIdentity(info: PluginInfo): HTMLElement {
  const ident = el("div", "pl-ident");
  ident.title = info.id;

  const name = el("span", "pl-name");
  name.textContent = info.name || info.id;
  ident.append(name);

  if (info.version) {
    const version = el("span", "pl-version");
    version.textContent = `В· ${info.version}`;
    ident.append(version);
  }

  const id = el("span", "pl-id");
  id.textContent = info.id;
  id.title = `id: ${info.id}`;
  ident.append(id);

  return ident;
}


function renderInfoButton(info: PluginInfo): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "pl-info";
  btn.setAttribute("aria-expanded", "false");
  btn.setAttribute("aria-controls", infoPopoverId(info.id));
  btn.setAttribute("aria-label", `РРЅС„РѕСЂРјР°С†РёСЏ Рѕ РіСЂР°РЅРёС†Р°С… РёР·РѕР»СЏС†РёРё РїР»Р°РіРёРЅР° ${info.id}`);
  btn.append(infoIcon());
  return btn;
}



function renderInfoPopover(info: PluginInfo): HTMLElement {
  const pop = el("div", "pl-info-pop");
  pop.id = infoPopoverId(info.id);
  pop.setAttribute("role", "tooltip");
  pop.setAttribute("aria-hidden", "true");
  pop.hidden = true;
  for (const notice of info.notices) {
    const line = el("div", "pl-notice");
    line.textContent = notice;
    pop.append(line);
  }
  return pop;
}


function renderExpandButton(info: PluginInfo, open: boolean): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "pl-item-expand";
  btn.dataset.expand = info.id;
  btn.setAttribute("aria-expanded", String(open));
  btn.setAttribute("aria-controls", bodyId(info.id));
  btn.setAttribute("aria-label", `${open ? "РЎРІРµСЂРЅСѓС‚СЊ" : "Р Р°Р·РІРµСЂРЅСѓС‚СЊ"} РєР°СЂС‚РѕС‡РєСѓ ${info.id}`);
  btn.append(chevronIcon());
  return btn;
}

export function createPluginManager(opts: PluginManagerOptions): PluginManager {
  const { root, status, onPlugins } = opts;
  let disposed = false;

  let refreshSeq = 0;

  let lastList: PluginInfo[] = [];

  let reloading: Promise<void> | null = null;

  let openInfoButton: HTMLButtonElement | null = null;

  const infoPopovers = new WeakMap<HTMLButtonElement, HTMLElement>();


  const settingEnums = new WeakMap<HTMLSelectElement, Json[]>();


  const expanded = new Set<string>();

  function renderBadge(info: PluginInfo): HTMLElement {
    const badge = el("span", `pl-badge ${info.status.state}`);
    badge.textContent = STATUS_LABELS[info.status.state];

    badge.title = statusMessage(info.status) ?? STATUS_LABELS[info.status.state];
    return badge;
  }



  function renderConsent(info: PluginInfo): HTMLElement | null {
    const requested = info.permissions;
    if (requested.length === 0) return null;
    const granted = new Set(info.granted_permissions);
    const grantedCount = requested.filter((permission) => granted.has(permission)).length;

    const section = el("div", "pl-perms");

    const head = el("div", "pl-perms-head");
    const title = el("span", "pl-perms-title");
    title.textContent = `Р”РѕСЃС‚СѓРї: ${grantedCount} РёР· ${requested.length}`;

    const bulk = el("div", "pl-perms-bulk");
    const grantAll = document.createElement("button");
    grantAll.type = "button";
    grantAll.className = "pl-grant-all";
    grantAll.dataset.grant = "all";
    grantAll.textContent = "Р’С‹РґР°С‚СЊ РІСЃРµ";
    grantAll.disabled = grantedCount === requested.length;
    const grantNone = document.createElement("button");
    grantNone.type = "button";
    grantNone.className = "pl-grant-none";
    grantNone.dataset.grant = "none";
    grantNone.textContent = "РЎРЅСЏС‚СЊ РІСЃРµ";
    grantNone.disabled = grantedCount === 0;
    bulk.append(grantAll, grantNone);
    head.append(title, bulk);
    section.append(head);

    for (const permission of requested) {
      const row = el("label", "pl-perm");
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.className = "pl-grant";
      checkbox.dataset.permission = permission;
      checkbox.checked = granted.has(permission);

      checkbox.setAttribute(
        "aria-label",
        `${checkbox.checked ? "РЎРЅСЏС‚СЊ" : "Р’С‹РґР°С‚СЊ"} СЂР°Р·СЂРµС€РµРЅРёРµ ${permissionLabel(permission)}`
      );
      const name = el("span", "pl-perm-name");
      name.textContent = permissionLabel(permission);
      const token = el("span", "pl-perm-token");
      token.textContent = permission;
      token.title = permission;
      const state = el("span", "pl-perm-state");
      state.textContent = checkbox.checked ? "РІС‹РґР°РЅРѕ" : "РѕС‚РєР»РѕРЅРµРЅРѕ";
      row.append(checkbox, name, token, state);
      section.append(row);
    }


    const missing = requested.filter((permission) => !granted.has(permission));
    if (missing.length > 0) {
      const hint = el("div", "pl-consent-hint");
      hint.textContent = `РќРµ РІС‹РґР°РЅРѕ: ${missing.join(", ")}. РџР»Р°РіРёРЅ РЅРµ СЃРјРѕР¶РµС‚ РёС… РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ.`;
      section.append(hint);
    }


    const boundaryNotice = documentBoundaryNotice(info);
    if (boundaryNotice) {
      const boundary = el("div", "pl-boundary");
      const icon = el("span", "ic");
      icon.append(shieldIcon());
      const text = el("span", "pl-boundary-text");
      text.textContent = boundaryNotice;
      boundary.append(icon, text);
      section.append(boundary);
    }

    return section;
  }



  function settingControl(
    prop: PluginSettingProperty,
    current: unknown
  ): HTMLInputElement | HTMLSelectElement | null {
    const options = prop.enum;
    if (Array.isArray(options) && options.length > 0) {
      const select = document.createElement("select");
      select.className = "pl-setting-control";
      options.forEach((value, index) => {
        const option = document.createElement("option");
        option.value = String(index);
        option.textContent = String(value);
        if (value === current) option.selected = true;
        select.append(option);
      });
      settingEnums.set(select, options);
      return select;
    }

    const input = document.createElement("input");
    input.className = "pl-setting-control";
    if (prop.type === "boolean") {
      input.type = "checkbox";
      input.checked = current === true;
    } else if (prop.type === "integer" || prop.type === "number") {
      input.type = "number";
      if (prop.type === "integer") input.step = "1";
      if (typeof current === "number") input.value = String(current);
    } else if (prop.type === "string") {
      input.type = "text";
      if (typeof current === "string") input.value = current;
    } else {
      return null;
    }
    return input;
  }



  function renderSettings(info: PluginInfo): HTMLElement | null {
    const properties = info.settings_schema?.properties;
    if (!properties) return null;

    const rows: HTMLElement[] = [];
    for (const [key, prop] of Object.entries(properties)) {
      const control = settingControl(prop, settingValue(info, key, prop));
      if (control) rows.push(renderSettingRow(key, prop, control));
    }
    if (rows.length === 0) return null;

    const section = el("div", "pl-settings");
    const title = el("div", "pl-settings-title");
    title.textContent = "РќР°СЃС‚СЂРѕР№РєРё";
    section.append(title, ...rows);
    return section;
  }


  function setSettingsBusy(item: HTMLElement, busy: boolean): void {
    for (const control of item.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
      ".pl-setting-control"
    )) {
      control.disabled = busy;
    }
  }


  function readSettingValue(
    control: HTMLInputElement | HTMLSelectElement
  ): Json | undefined {
    if (control instanceof HTMLSelectElement) {
      const options = settingEnums.get(control);
      const index = Number(control.value);
      if (!options || !Number.isInteger(index) || index < 0 || index >= options.length) {
        return undefined;
      }
      return options[index];
    }
    if (control.type === "checkbox") return control.checked;
    if (control.type === "number") {
      const value = Number(control.value);
      return Number.isFinite(value) ? value : undefined;
    }
    return control.value;
  }


  function renderItem(info: PluginInfo): HTMLElement {
    const item = el("div", "pl-item");
    item.dataset.plugin = info.id;
    item.style.setProperty("--pc", pluginColor(info.id));

    const head = el("div", "pl-item-head");

    const toggleLabel = el("label", "pl-toggle");
    toggleLabel.title = info.enabled ? "Р’С‹РєР»СЋС‡РёС‚СЊ РїР»Р°РіРёРЅ" : "Р’РєР»СЋС‡РёС‚СЊ РїР»Р°РіРёРЅ";
    const toggle = document.createElement("input");
    toggle.type = "checkbox";
    toggle.className = "pl-enabled";
    toggle.checked = info.enabled;
    toggle.setAttribute("aria-label", `${info.enabled ? "Р’С‹РєР»СЋС‡РёС‚СЊ" : "Р’РєР»СЋС‡РёС‚СЊ"} РїР»Р°РіРёРЅ ${info.id}`);
    toggleLabel.append(toggle);

    const open = expanded.has(info.id);


    head.append(toggleLabel, renderIdentity(info));

    const infoPopover = info.notices.length > 0 ? renderInfoPopover(info) : null;
    if (infoPopover) {
      const infoButton = renderInfoButton(info);
      infoPopovers.set(infoButton, infoPopover);
      head.append(infoButton);
    }
    head.append(renderBadge(info), renderExpandButton(info, open));

    if (infoPopover) head.append(infoPopover);

    const body = el("div", "pl-item-body");
    body.id = bodyId(info.id);



    const failMessage = statusMessage(info.status);
    if (failMessage) {
      const node = el("div", "pl-notice");
      node.textContent = failMessage;
      body.append(node);
    }


    const hintText = stateHint(info.status);
    if (hintText) {
      const hint = el("div", "pl-state-hint");
      hint.textContent = hintText;
      body.append(hint);
    }

    const actions = el("div", "pl-actions");
    const reload = document.createElement("button");
    reload.type = "button";
    reload.className = "pl-reload";
    reload.textContent = "РџРµСЂРµР·Р°РіСЂСѓР·РёС‚СЊ";
    reload.disabled = !info.enabled;
    actions.append(reload);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "pl-remove";
    remove.textContent = "РЈРґР°Р»РёС‚СЊ";
    actions.append(remove);

    for (const command of info.commands) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "pl-cmd";
      btn.dataset.command = command.id;
      btn.textContent = command.title;
      btn.title = command.keybinding
        ? `${command.title} (${command.keybinding})`
        : command.title;
      btn.setAttribute("aria-label", `Р’С‹РїРѕР»РЅРёС‚СЊ: ${command.title}`);
      actions.append(btn);
    }
    body.append(actions);

    const consent = renderConsent(info);
    if (consent) body.append(consent);

    const settings = renderSettings(info);
    if (settings) body.append(settings);



    item.classList.toggle("open", open);
    item.append(head, body);
    return item;
  }



  function toggleInfoPopover(button: HTMLButtonElement): void {
    const popover = infoPopovers.get(button);
    if (!popover) return;
    const wasOpen = openInfoButton === button;
    closeAllInfoPopovers();
    if (wasOpen) return;
    popover.hidden = false;
    popover.setAttribute("aria-hidden", "false");
    button.setAttribute("aria-expanded", "true");
    openInfoButton = button;
  }


  function closeAllInfoPopovers(): HTMLButtonElement | null {
    const button = openInfoButton;
    if (!button) return null;
    const popover = infoPopovers.get(button);
    if (popover) {
      popover.hidden = true;
      popover.setAttribute("aria-hidden", "true");
    }
    button.setAttribute("aria-expanded", "false");
    openInfoButton = null;
    return button;
  }



  function toggleExpand(btn: HTMLButtonElement): void {
    const id = btn.dataset.expand;
    const item = btn.closest<HTMLElement>(".pl-item");
    if (!id || !item) return;
    const open = !expanded.has(id);
    if (open) expanded.add(id);
    else expanded.delete(id);
    item.classList.toggle("open", open);
    btn.setAttribute("aria-expanded", String(open));
    btn.setAttribute("aria-label", `${open ? "РЎРІРµСЂРЅСѓС‚СЊ" : "Р Р°Р·РІРµСЂРЅСѓС‚СЊ"} РєР°СЂС‚РѕС‡РєСѓ ${id}`);
  }

  function render(list: PluginInfo[]): void {

    closeAllInfoPopovers();

    const ids = new Set(list.map((info) => info.id));
    for (const id of expanded) {
      if (!ids.has(id)) expanded.delete(id);
    }
    if (list.length === 0) {
      const empty = el("div", "side-empty pl-empty");
      const title = el("div", "pl-empty-title");
      title.textContent = "РџР»Р°РіРёРЅС‹ РЅРµ РЅР°Р№РґРµРЅС‹";
      const hint = el("div", "pl-empty-hint");
      hint.append("РџРѕР»РѕР¶РёС‚Рµ РїР»Р°РіРёРЅ РІ ");
      const code = document.createElement("code");
      code.textContent = "%APPDATA%/darmark/plugins/<id>/";
      hint.append(code);


      hint.append(" (plugin.json + main.lua) Рё РїРµСЂРµР·Р°РїСѓСЃС‚РёС‚Рµ РџР•РќРђР›.");
      empty.append(title, hint);
      root.replaceChildren(empty);
      return;
    }
    root.replaceChildren(...list.map(renderItem));
  }

  async function refresh(): Promise<void> {
    const seq = ++refreshSeq;
    let list: PluginInfo[];
    try {
      list = await listPlugins();
    } catch (e) {
      if (!disposed && seq === refreshSeq) status(`РџР»Р°РіРёРЅС‹: ${errorMessage(e)}`);
      return;
    }

    if (disposed || seq !== refreshSeq) return;
    lastList = list;
    render(list);
    onPlugins?.(list);
  }



  async function reloadForDocument(path: string | null): Promise<void> {


    if (reloading) return reloading;
    reloading = doReload(path).finally(() => {
      reloading = null;
    });
    return reloading;
  }

  async function doReload(path: string | null): Promise<void> {

    const source = lastList.length > 0 ? lastList : await listPlugins();
    const enabled = source.filter((info) => info.enabled);
    if (enabled.length === 0) {
      status("РџР»Р°РіРёРЅС‹: РЅРµС‚ РІРєР»СЋС‡С‘РЅРЅС‹С…");
      return;
    }
    const ids = enabled.map((info) => info.id);
    const owner = ownerPluginId(path, ids);
    if (owner) {
      await reloadPlugin(owner);
      status(`РџР»Р°РіРёРЅ ${owner}: РїРµСЂРµР·Р°РіСЂСѓР¶РµРЅ`);
    } else {

      for (const id of ids) await reloadPlugin(id);
      status(`РџР»Р°РіРёРЅС‹: РїРµСЂРµР·Р°РіСЂСѓР¶РµРЅРѕ (${ids.length})`);
    }
    await refresh();
  }

  async function handleToggle(input: HTMLInputElement): Promise<void> {
    const id = input.closest<HTMLElement>(".pl-item")?.dataset.plugin;
    if (!id) return;
    input.disabled = true; // Р·Р°С‰РёС‚Р° РѕС‚ РїРѕРІС‚РѕСЂРЅРѕРіРѕ РєР»РёРєР° РґРѕ РїРµСЂРµСЂРёСЃРѕРІРєРё
    try {
      await setPluginEnabled(id, input.checked);
    } catch (e) {
      status(`РџР»Р°РіРёРЅ ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }


  function setConsentBusy(item: HTMLElement, busy: boolean): void {
    for (const cb of item.querySelectorAll<HTMLInputElement>("input.pl-grant")) {
      cb.disabled = busy;
    }
    for (const btn of item.querySelectorAll<HTMLButtonElement>(".pl-perms-bulk button")) {
      btn.disabled = busy;
    }
  }


  async function handleGrant(input: HTMLInputElement): Promise<void> {
    const item = input.closest<HTMLElement>(".pl-item");
    const id = item?.dataset.plugin;
    if (!item || !id) return;
    const granted = Array.from(item.querySelectorAll<HTMLInputElement>("input.pl-grant"))
      .filter((cb) => cb.checked)
      .map((cb) => cb.dataset.permission)
      .filter((permission): permission is string => !!permission);
    setConsentBusy(item, true);
    try {
      await setPluginPermissions(id, granted);
    } catch (e) {
      status(`РџР»Р°РіРёРЅ ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }


  async function handleGrantBulk(btn: HTMLButtonElement): Promise<void> {
    const item = btn.closest<HTMLElement>(".pl-item");
    const id = item?.dataset.plugin;
    const mode = btn.dataset.grant;
    if (!item || !id || !mode) return;
    const granted =
      mode === "all"
        ? Array.from(item.querySelectorAll<HTMLInputElement>("input.pl-grant"))
            .map((cb) => cb.dataset.permission)
            .filter((permission): permission is string => !!permission)
        : [];
    setConsentBusy(item, true);
    try {
      await setPluginPermissions(id, granted);
    } catch (e) {
      status(`РџР»Р°РіРёРЅ ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }


  async function handleSettingChange(
    control: HTMLInputElement | HTMLSelectElement
  ): Promise<void> {
    const item = control.closest<HTMLElement>(".pl-item");
    const id = item?.dataset.plugin;
    const key = control.dataset.settingKey;
    if (!item || !id || !key) return;
    const value = readSettingValue(control);
    if (value === undefined) return; // РЅРµРІР°Р»РёРґРЅС‹Р№ РІРІРѕРґ вЂ” СЃС‚РѕСЂ РЅРµ С‚СЂРѕРіР°РµРј
    setSettingsBusy(item, true);
    try {
      await setPluginSetting(id, key, value);
    } catch (e) {
      status(`РџР»Р°РіРёРЅ ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }

  async function handleReload(btn: HTMLButtonElement): Promise<void> {
    const id = btn.closest<HTMLElement>(".pl-item")?.dataset.plugin;
    if (!id) return;
    btn.disabled = true;
    try {
      await reloadPlugin(id);
      status(`РџР»Р°РіРёРЅ ${id}: РїРµСЂРµР·Р°РіСЂСѓР¶РµРЅ`);
    } catch (e) {
      status(`РџР»Р°РіРёРЅ ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }



  function confirmRemovePlugin(id: string): Promise<boolean> {
    return new Promise((resolve) => {
      const content = el("div", "pl-confirm-remove");
      const text = el("p", "pl-confirm-text");
      text.textContent = `РџР»Р°РіРёРЅ В«${id}В» Р±СѓРґРµС‚ СѓРґР°Р»С‘РЅ Р±РµР·РІРѕР·РІСЂР°С‚РЅРѕ. Р•РіРѕ С„Р°Р№Р»С‹ Рё РЅР°СЃС‚СЂРѕР№РєРё Р±СѓРґСѓС‚ СЃС‚С‘СЂС‚С‹.`;
      content.append(text);

      const buttons = el("div", "pl-confirm-buttons");
      const cancelBtn = document.createElement("button");
      cancelBtn.type = "button";
      cancelBtn.className = "pl-confirm-cancel";
      cancelBtn.textContent = "РћС‚РјРµРЅР°";
      const confirmBtn = document.createElement("button");
      confirmBtn.type = "button";
      confirmBtn.className = "pl-confirm-ok";
      confirmBtn.textContent = "РЈРґР°Р»РёС‚СЊ";
      buttons.append(cancelBtn, confirmBtn);
      content.append(buttons);

      let dialog: Dialog | null = null;
      const cleanup = (result: boolean) => {
        dialog?.close();
        resolve(result);
      };

      cancelBtn.addEventListener("click", () => cleanup(false));
      confirmBtn.addEventListener("click", () => cleanup(true));

      dialog = openDialog({
        title: "РЈРґР°Р»РµРЅРёРµ РїР»Р°РіРёРЅР°",
        content,
        variant: "center",
        initialFocus: cancelBtn,
        onClose: () => resolve(false),
      });
    });
  }

  async function handleRemove(btn: HTMLButtonElement): Promise<void> {
    const id = btn.closest<HTMLElement>(".pl-item")?.dataset.plugin;
    if (!id) return;
    const confirmed = await confirmRemovePlugin(id);
    if (!confirmed) return;
    btn.disabled = true;
    try {
      await removePlugin(id);
      status(`РџР»Р°РіРёРЅ ${id}: СѓРґР°Р»С‘РЅ`);
    } catch (e) {
      status(`РџР»Р°РіРёРЅ ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }

  async function handleCommand(btn: HTMLButtonElement): Promise<void> {
    const commandId = btn.dataset.command;
    if (!commandId) return;
    btn.disabled = true;
    try {
      await runPluginCommand(commandId);
    } catch (e) {
      status(`РљРѕРјР°РЅРґР° ${commandId}: ${errorMessage(e)}`);
    }
    await refresh();
  }

  function onChange(e: Event): void {
    const target = e.target as HTMLElement | null;
    if (!target) return;
    if (target.classList.contains("pl-enabled")) {
      void handleToggle(target as HTMLInputElement);
      return;
    }
    if (target.classList.contains("pl-grant")) {
      void handleGrant(target as HTMLInputElement);
      return;
    }
    if (target.classList.contains("pl-setting-control")) {
      void handleSettingChange(target as HTMLInputElement | HTMLSelectElement);
    }
  }

  function onClick(e: MouseEvent): void {
    const target = e.target as HTMLElement | null;
    const info = target?.closest<HTMLButtonElement>(".pl-info");
    if (info) {
      toggleInfoPopover(info);
      return;
    }
    const expand = target?.closest<HTMLButtonElement>(".pl-item-expand");
    if (expand) {
      toggleExpand(expand);
      return;
    }
    const reload = target?.closest<HTMLButtonElement>(".pl-reload");
    if (reload) {
      void handleReload(reload);
      return;
    }
    const remove = target?.closest<HTMLButtonElement>(".pl-remove");
    if (remove) {
      void handleRemove(remove);
      return;
    }
    const command = target?.closest<HTMLButtonElement>(".pl-cmd");
    if (command) {
      void handleCommand(command);
      return;
    }
    const bulk = target?.closest<HTMLButtonElement>(".pl-perms-bulk button");
    if (bulk) void handleGrantBulk(bulk);
  }


  function onDocumentClick(e: MouseEvent): void {
    const button = openInfoButton;
    if (!button) return;
    const popover = infoPopovers.get(button);
    const target = e.target as Node | null;
    if (target && (button.contains(target) || popover?.contains(target))) return;
    closeAllInfoPopovers();
  }


  function onDocumentKeydown(e: KeyboardEvent): void {
    if (e.key !== "Escape" || !openInfoButton) return;


    e.preventDefault();
    e.stopPropagation();
    closeAllInfoPopovers()?.focus();
  }

  root.addEventListener("change", onChange);
  root.addEventListener("click", onClick);
  document.addEventListener("click", onDocumentClick);
  document.addEventListener("keydown", onDocumentKeydown);


  const unlistenPromise = listen("plugins-changed", () => {
    void refresh();
  }).catch(() => null);

  return {
    refresh,
    reloadForDocument,

    dispose(): void {
      disposed = true;
      expanded.clear();
      closeAllInfoPopovers();
      root.removeEventListener("change", onChange);
      root.removeEventListener("click", onClick);
      document.removeEventListener("click", onDocumentClick);
      document.removeEventListener("keydown", onDocumentKeydown);
      void unlistenPromise.then((fn) => fn?.());
    },
  };
}
