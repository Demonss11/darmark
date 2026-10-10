// pluginStatusBar.ts — per-plugin элементы в статусбаре (TZ-UX-SPEC-CLEANUP.md, п.5).
//
// Показываем включённые плагины, получившие согласие `ui:statusbar`: точка
// цвета плагина (`--pc`) + последнее сообщение (`host.show_message`) либо
// локализованный статус. Клик по элементу открывает первое представление
// плагина; при лимите (MAX_VISIBLE_PS) лишние сворачиваются в «+N», клик по
// которому раскрывает панель «Плагины».
//
// Только `textContent`/DOM: сообщения плагинов не проходят через `innerHTML`.

import { pluginColor } from "./pluginColor";
import type { PluginInfo, PluginStatus } from "./tauri";

/** Сколько элементов показываем до сворачивания в «+N». */
const MAX_VISIBLE_PS = 3;
/** Сколько держится сообщение плагина до возврата к статусу. */
const MESSAGE_MS = 4000;
/** Право, без которого плагин не может писать в статусбар (deny-by-default). */
const STATUSBAR_PERMISSION = "ui:statusbar";

/** Человекочитаемая подпись статуса для элемента без свежего сообщения. */
const STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "активен",
  failed: "ошибка",
  quarantined: "карантин",
  stopped: "остановлен",
};

export interface PluginStatusBar {
  /** Перерисовать элементы из свежего снимка плагинов. */
  render(list: PluginInfo[]): void;
  /** Показать сообщение плагина (с возвратом к статусу через ~4 с).
   *  Возвращает `false`, если элемент не показан (плагин свёрнут в «+N» или скрыт). */
  setMessage(id: string, text: string): boolean;
  /** Снять слушатели/таймеры и очистить контейнер. */
  dispose(): void;
}

export interface PluginStatusBarOptions {
  /** Контейнер `#plugin-status` в статусбаре. */
  root: HTMLElement;
  /** Есть ли у плагина представление (кнопка) или только подпись (span). */
  hasView(pluginId: string): boolean;
  /** Клик по элементу: открыть представление плагина (no-op, если его нет). */
  onActivate(pluginId: string): void;
  /** Клик по «+N»: открыть дровер «Плагины». */
  onShowPanel(): void;
}

export function createPluginStatusBar(opts: PluginStatusBarOptions): PluginStatusBar {
  const { root, hasView, onActivate, onShowPanel } = opts;
  // Отдельный polite-аннонсер: SR озвучивает сообщение, не читая всю строку.
  const announcer = document.getElementById("plugin-announcer");

  // Последний снимок плагинов — источник базовых статусов при возврате текста.
  let lastList: PluginInfo[] = [];
  // Транзиентные сообщения плагинов и таймеры их авто-сброса.
  const messages = new Map<string, string>();
  const timers = new Map<string, number>();
  let disposed = false;

  /**
   * Наличие представления берётся из `pluginViews` (предикат `hasView`), а не из
   * DOM `#view-switch`: у `plugins-changed` и `plugin-views-changed` независимый
   * порядок доставки, и чтение DOM дало бы гонку (MAJOR ревью).
   */

  /** Плагины, которым место в статусбаре: включённые и с правом `ui:statusbar`. */
  function statusbarPlugins(): PluginInfo[] {
    return lastList.filter(
      (info) => info.enabled && info.granted_permissions.includes(STATUSBAR_PERMISSION)
    );
  }

  function textFor(info: PluginInfo): string {
    return messages.get(info.id) ?? STATUS_LABELS[info.status.state];
  }

  /** Обновляет подпись элемента при смене текста (базовый или сообщение). */
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

    // Точечно обновляем только этот элемент — без полной перерисовки списка.
    const item = root.querySelector<HTMLElement>(`.ps-item[data-plugin="${id}"]`);
    if (item) setItemText(item, id, text);

    const prev = timers.get(id);
    if (prev !== undefined) window.clearTimeout(prev);
    const timer = window.setTimeout(() => {
      timers.delete(id);
      messages.delete(id);
      // Возврат к базовому статусу; берём свежий снимок (статус мог измениться).
      const info = lastList.find((plugin) => plugin.id === id);
      if (!info) return;
      const current = root.querySelector<HTMLElement>(`.ps-item[data-plugin="${id}"]`);
      if (current) setItemText(current, id, STATUS_LABELS[info.status.state]);
    }, MESSAGE_MS);
    timers.set(id, timer);
    // `false` — элемент скрыт (плагин свёрнут в «+N»): вызывающая сторона решает,
    // показать ли текст в другом канале (#stat-msg).
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
