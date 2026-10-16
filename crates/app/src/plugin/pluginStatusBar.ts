







import { pluginColor } from "./pluginColor";
import type { PluginInfo, PluginStatus } from "../core/tauri";

const MAX_VISIBLE_PS = 3;
const MESSAGE_MS = 4000;
const STATUSBAR_PERMISSION = "ui:statusbar";

const STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "активен",
  failed: "ошибка",
  quarantined: "карантин",
  stopped: "остановлен",
};

interface PluginStatusBar {
  render(list: PluginInfo[]): void;
  setMessage(id: string, text: string): boolean;
  dispose(): void;
}

interface PluginStatusBarOptions {
  root: HTMLElement;
  hasView(pluginId: string): boolean;
  onActivate(pluginId: string): void;
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
      interactive ? `${id}: ${text}. Открыть представление` : `${id}: ${text}`
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
    item.title = "Ctrl+R — перезагрузить";
    item.setAttribute(
      "aria-label",
      interactive ? `${info.id}: ${text}. Открыть представление` : `${info.id}: ${text}`
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
      more.setAttribute("aria-label", `Ещё плагины: ${more.title}. Открыть панель`);
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
