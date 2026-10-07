// pluginViews.ts — контроллер плагинных тир-1 представлений (H2, Фаза 4).
//
// Роль: показать HTML, присланный Lua-плагином, в отдельном контейнере рядом с
// `#preview` и вернуть клики из DOM обратно в плагин (§4.6/§9.4 DESIGN_DOC,
// ADR-0022). Список представлений и их HTML живут на Rust-стороне; фронт лишь
// строит вкладки в `.view-switch` панели предпросмотра и пишет `innerHTML` в
// контейнер плагинного view. `preview.innerHTML` этот модуль не трогает —
// единственный писатель остаётся `previewView.ts`.
//
// Обратная маршрутизация: делегированный `click` на контейнере читает
// `data-p-<plugin_id>-action`/`-payload` и вызывает `plugin_view_action`.
// `plugin_id` берётся из `PluginViewInfo`, поэтому имя атрибута однозначно.
// Никакого доступа к `window`/`fetch`/ФС из контейнера нет: HTML уже
// санитизирован хостом (ядро md-core).

import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage, pluginViewAction, pluginViews, type PluginViewInfo } from "./tauri";
import type { Json } from "./viewRegistry";

/** Публичный фасад контроллера: перестроение из Rust и освобождение ресурсов. */
export interface PluginViews {
  /** Запросить список представлений и перестроить вкладки/контейнер. */
  refresh(): Promise<void>;
  /** Снять слушатели и очистить вкладки/контейнер. */
  dispose(): void;
}

export interface PluginViewsOptions {
  /** `.view-switch` панели предпросмотра (core-вкладка уже в разметке). */
  switchEl: HTMLElement;
  /** Контейнер плагинного HTML (`#plugin-view`). */
  containerEl: HTMLElement;
  /** `#preview` — скрывается, пока активна плагинная вкладка. */
  previewEl: HTMLElement;
  /** Сообщение в статусбар (ошибки IPC/действия). */
  status(msg: string): void;
}

/** Разбор payload из `data-p-*-payload`: JSON, иначе строка, иначе `undefined`. */
function parsePayload(raw: string | null): Json | undefined {
  if (raw === null) return undefined;
  try {
    return JSON.parse(raw) as Json;
  } catch {
    return raw; // не JSON — передаём плагину как есть
  }
}

export function createPluginViews(opts: PluginViewsOptions): PluginViews {
  const { switchEl, containerEl, previewEl, status } = opts;

  // Активный плагинный view; `null` — активна core-вкладка «Предпросмотр».
  let activeViewId: string | null = null;
  let viewsById = new Map<string, PluginViewInfo>();
  let disposed = false;
  // Защита от гонок: ответ устаревшего `plugin_views` не должен перетирать свежий.
  let refreshSeq = 0;

  // Подписка на изменение списка/контента представлений. Ошибку установки
  // глушим: в обычном (не плагинном) запуске событий просто не будет.
  const unlistenPromise = listen("plugin-views-changed", () => {
    void refresh();
  }).catch(() => null);

  /** Подсветить вкладку: `null` — core, иначе плагинная с данным view_id. */
  function setActiveTab(viewId: string | null): void {
    for (const tab of switchEl.querySelectorAll<HTMLElement>(".vtab")) {
      const isPlugin = tab.classList.contains("plugin");
      const active = viewId === null ? !isPlugin : isPlugin && tab.dataset.view === viewId;
      tab.classList.toggle("active", active);
      if (active) tab.setAttribute("aria-current", "true");
      else tab.removeAttribute("aria-current");
    }
  }

  /** Вернуться к core-вкладке «Предпросмотр» (container остаётся в DOM, скрыт). */
  function showPreview(): void {
    activeViewId = null;
    previewEl.hidden = false;
    containerEl.hidden = true;
    setActiveTab(null);
  }

  /** Показать плагинное представление; единственная запись `containerEl.innerHTML`. */
  function showPlugin(viewId: string): void {
    const info = viewsById.get(viewId);
    if (!info) {
      showPreview();
      return;
    }
    activeViewId = viewId;
    previewEl.hidden = true;
    containerEl.hidden = false;
    containerEl.innerHTML = info.html;
    setActiveTab(viewId);
  }

  /** Перестроить вкладки и восстановить активное состояние. */
  function render(list: PluginViewInfo[]): void {
    viewsById = new Map(list.map((v) => [v.view_id, v]));

    // Core-вкладку не трогаем (заморожена H1): пересобираем только плагинные.
    for (const old of switchEl.querySelectorAll<HTMLElement>(".vtab.plugin")) old.remove();
    for (const info of list) {
      const tab = document.createElement("button");
      tab.type = "button";
      tab.className = "vtab plugin";
      tab.dataset.view = info.view_id;
      tab.title = info.title;
      const origin = document.createElement("span");
      origin.className = "origin";
      tab.append(origin, info.title);
      switchEl.append(tab);
    }

    // Активный плагин исчез (перезагрузка/отключение) — возвращаемся к preview.
    if (activeViewId !== null && !viewsById.has(activeViewId)) {
      showPreview();
    } else if (activeViewId !== null) {
      // Контент мог обновиться тем же событием — перерисовать HTML.
      showPlugin(activeViewId);
    } else {
      showPreview();
    }
  }

  async function refresh(): Promise<void> {
    const seq = ++refreshSeq;
    let list: PluginViewInfo[];
    try {
      list = await pluginViews();
    } catch (e) {
      if (!disposed && seq === refreshSeq) status(`Плагины: ${errorMessage(e)}`);
      return;
    }
    // Ответ устарел (пришёл refresh новее) или контроллер уже освобождён.
    if (disposed || seq !== refreshSeq) return;
    render(list);
  }

  function onSwitchClick(e: MouseEvent): void {
    const tab = (e.target as HTMLElement | null)?.closest<HTMLElement>(".vtab");
    if (!tab) return;
    const viewId = tab.dataset.view;
    if (viewId) showPlugin(viewId);
    else showPreview();
  }

  function onContainerClick(e: MouseEvent): void {
    if (activeViewId === null) return;
    const target = e.target as HTMLElement | null;
    // Внешние ссылки — системным браузером, как в previewView: иначе WebView
    // навигируется на чужой ресурс (CSP навигацию верхнего уровня не блокирует).
    const anchor = target?.closest<HTMLAnchorElement>("a");
    if (anchor) {
      const href = anchor.getAttribute("href") ?? "";
      if (/^https?:/i.test(href)) {
        e.preventDefault();
        void openUrl(href).catch((err) => status(`Ссылка: ${errorMessage(err)}`));
        return;
      }
    }
    const info = viewsById.get(activeViewId);
    if (!info) return;
    const pluginId = info.plugin_id;
    const el = target?.closest<HTMLElement>(`[data-p-${pluginId}-action]`);
    if (!el) return;
    const action = el.getAttribute(`data-p-${pluginId}-action`);
    if (!action) return;
    e.preventDefault();
    const payload = parsePayload(el.getAttribute(`data-p-${pluginId}-payload`));
    void pluginViewAction(activeViewId, action, payload).catch((err) =>
      status(`Плагин: ${errorMessage(err)}`)
    );
  }

  switchEl.addEventListener("click", onSwitchClick);
  containerEl.addEventListener("click", onContainerClick);

  return {
    refresh,

    dispose(): void {
      disposed = true;
      switchEl.removeEventListener("click", onSwitchClick);
      containerEl.removeEventListener("click", onContainerClick);
      for (const tab of switchEl.querySelectorAll<HTMLElement>(".vtab.plugin")) tab.remove();
      containerEl.innerHTML = "";
      containerEl.hidden = true;
      previewEl.hidden = false;
      setActiveTab(null);
      void unlistenPromise.then((fn) => fn?.());
    },
  };
}
