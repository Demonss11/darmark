// pluginStatusBar.ts вЂ” per-plugin СЌР»РµРјРµРЅС‚С‹ РІ СЃС‚Р°С‚СѓСЃР±Р°СЂРµ.
//
// РџРѕРєР°Р·С‹РІР°РµРј РІРєР»СЋС‡С‘РЅРЅС‹Рµ РїР»Р°РіРёРЅС‹, РїРѕР»СѓС‡РёРІС€РёРµ СЃРѕРіР»Р°СЃРёРµ `ui:statusbar`: С‚РѕС‡РєР°
// С†РІРµС‚Р° РїР»Р°РіРёРЅР° (`--pc`) + РїРѕСЃР»РµРґРЅРµРµ СЃРѕРѕР±С‰РµРЅРёРµ (`host.show_message`) Р»РёР±Рѕ
// Р»РѕРєР°Р»РёР·РѕРІР°РЅРЅС‹Р№ СЃС‚Р°С‚СѓСЃ. РљР»РёРє РѕС‚РєСЂС‹РІР°РµС‚ РїРµСЂРІРѕРµ РїСЂРµРґСЃС‚Р°РІР»РµРЅРёРµ РїР»Р°РіРёРЅР°; Р»РёС€РЅРёРµ
// СЃРІРѕСЂР°С‡РёРІР°СЋС‚СЃСЏ РІ В«+NВ». РўРѕР»СЊРєРѕ DOM/`textContent` вЂ” СЃРѕРѕР±С‰РµРЅРёСЏ РїР»Р°РіРёРЅРѕРІ РЅРµ
// РїСЂРѕС…РѕРґСЏС‚ С‡РµСЂРµР· `innerHTML`.

import { pluginColor } from "../plugin/pluginColor";
import type { PluginInfo, PluginStatus } from "../core/tauri";

const MAX_VISIBLE_PS = 3;
const MESSAGE_MS = 4000;
/** РџСЂР°РІРѕ, Р±РµР· РєРѕС‚РѕСЂРѕРіРѕ РїР»Р°РіРёРЅ РЅРµ РјРѕР¶РµС‚ РїРёСЃР°С‚СЊ РІ СЃС‚Р°С‚СѓСЃР±Р°СЂ (deny-by-default). */
const STATUSBAR_PERMISSION = "ui:statusbar";

const STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "Р°РєС‚РёРІРµРЅ",
  failed: "РѕС€РёР±РєР°",
  quarantined: "РєР°СЂР°РЅС‚РёРЅ",
  stopped: "РѕСЃС‚Р°РЅРѕРІР»РµРЅ",
};

export interface PluginStatusBar {
  /** РџРµСЂРµСЂРёСЃРѕРІР°С‚СЊ СЌР»РµРјРµРЅС‚С‹ РёР· СЃРІРµР¶РµРіРѕ СЃРЅРёРјРєР° РїР»Р°РіРёРЅРѕРІ. */
  render(list: PluginInfo[]): void;


  setMessage(id: string, text: string): boolean;
  /** РЎРЅСЏС‚СЊ СЃР»СѓС€Р°С‚РµР»Рё/С‚Р°Р№РјРµСЂС‹ Рё РѕС‡РёСЃС‚РёС‚СЊ РєРѕРЅС‚РµР№РЅРµСЂ. */
  dispose(): void;
}

export interface PluginStatusBarOptions {
  /** РљРѕРЅС‚РµР№РЅРµСЂ `#plugin-status` РІ СЃС‚Р°С‚СѓСЃР±Р°СЂРµ. */
  root: HTMLElement;
  /** Р•СЃС‚СЊ Р»Рё Сѓ РїР»Р°РіРёРЅР° РїСЂРµРґСЃС‚Р°РІР»РµРЅРёРµ (РєРЅРѕРїРєР°) РёР»Рё С‚РѕР»СЊРєРѕ РїРѕРґРїРёСЃСЊ (span). */
  hasView(pluginId: string): boolean;
  /** РљР»РёРє РїРѕ СЌР»РµРјРµРЅС‚Сѓ: РѕС‚РєСЂС‹С‚СЊ РїСЂРµРґСЃС‚Р°РІР»РµРЅРёРµ РїР»Р°РіРёРЅР° (no-op, РµСЃР»Рё РµРіРѕ РЅРµС‚). */
  onActivate(pluginId: string): void;
  /** РљР»РёРє РїРѕ В«+NВ»: РѕС‚РєСЂС‹С‚СЊ РґСЂРѕРІРµСЂ В«РџР»Р°РіРёРЅС‹В». */
  onShowPanel(): void;
}

export function createPluginStatusBar(opts: PluginStatusBarOptions): PluginStatusBar {
  const { root, hasView, onActivate, onShowPanel } = opts;

  const announcer = document.getElementById("plugin-announcer");


  let lastList: PluginInfo[] = [];
  const messages = new Map<string, string>();
  const timers = new Map<string, number>();
  let disposed = false;



  function statusbarPlugins(): PluginInfo[] {
    return lastList.filter(
      (info) => info.enabled && info.granted_permissions.includes(STATUSBAR_PERMISSION)
    );
  }

  function textFor(info: PluginInfo): string {
    return messages.get(info.id) ?? STATUS_LABELS[info.status.state];
  }

  function setItemText(item: HTMLElement, id: string, text: string): void {
    const label = item.querySelector(".ps-text");
    if (label) label.textContent = text;
    const interactive = !item.classList.contains("ps-item-static");
    item.setAttribute(
      "aria-label",
      interactive ? `${id}: ${text}. РћС‚РєСЂС‹С‚СЊ РїСЂРµРґСЃС‚Р°РІР»РµРЅРёРµ` : `${id}: ${text}`
    );
  }

  function makeItem(info: PluginInfo): HTMLElement {
    const text = textFor(info);
    const interactive = hasView(info.id);
    const item = document.createElement(interactive ? "button" : "span");
    item.className = interactive ? "ps-item" : "ps-item ps-item-static";
    if (interactive) (item as HTMLButtonElement).type = "button";
    item.dataset.plugin = info.id;
    item.style.setProperty("--pc", pluginColor(info.id));
    item.title = "Ctrl+R вЂ” РїРµСЂРµР·Р°РіСЂСѓР·РёС‚СЊ";
    item.setAttribute(
      "aria-label",
      interactive ? `${info.id}: ${text}. РћС‚РєСЂС‹С‚СЊ РїСЂРµРґСЃС‚Р°РІР»РµРЅРёРµ` : `${info.id}: ${text}`
    );

    const dot = document.createElement("span");
    dot.className = "pdot";
    dot.setAttribute("aria-hidden", "true");
    const label = document.createElement("span");
    label.className = "ps-text";
    label.textContent = text;
    item.append(dot, label);
    return item;
  }

  function render(list: PluginInfo[]): void {
    lastList = list;
    if (disposed) return;
    const plugins = statusbarPlugins();
    const nodes: HTMLElement[] = plugins.slice(0, MAX_VISIBLE_PS).map(makeItem);
    const hidden = plugins.slice(MAX_VISIBLE_PS);
    if (hidden.length > 0) {
      const more = document.createElement("button");
      more.type = "button";
      more.className = "ps-more";
      more.textContent = `+${hidden.length}`;
      more.title = hidden.map((info) => info.id).join(", ");
      more.setAttribute("aria-label", `Р•С‰С‘ РїР»Р°РіРёРЅС‹: ${more.title}. РћС‚РєСЂС‹С‚СЊ РїР°РЅРµР»СЊ`);
      nodes.push(more);
    }
    root.replaceChildren(...nodes);
  }

  function setMessage(id: string, text: string): boolean {
    if (disposed) return false;
    messages.set(id, text);
    if (announcer) announcer.textContent = `${id}: ${text}`;


    const item = root.querySelector<HTMLElement>(`.ps-item[data-plugin="${id}"]`);
    if (item) setItemText(item, id, text);

    const prev = timers.get(id);
    if (prev !== undefined) window.clearTimeout(prev);
    const timer = window.setTimeout(() => {
      timers.delete(id);
      messages.delete(id);

      const info = lastList.find((plugin) => plugin.id === id);
      if (!info) return;
      const current = root.querySelector<HTMLElement>(`.ps-item[data-plugin="${id}"]`);
      if (current) setItemText(current, id, STATUS_LABELS[info.status.state]);
    }, MESSAGE_MS);
    timers.set(id, timer);


    return item !== null;
  }

  function onClick(e: MouseEvent): void {
    const target = e.target as HTMLElement | null;
    if (target?.closest(".ps-more")) {
      onShowPanel();
      return;
    }
    const item = target?.closest<HTMLElement>(".ps-item");
    const id = item?.dataset.plugin;
    if (id && item && !item.classList.contains("ps-item-static")) onActivate(id);
  }

  root.addEventListener("click", onClick);

  return {
    render,
    setMessage,
    dispose(): void {
      disposed = true;
      root.removeEventListener("click", onClick);
      for (const timer of timers.values()) window.clearTimeout(timer);
      timers.clear();
      messages.clear();
      root.replaceChildren();
    },
  };
}
