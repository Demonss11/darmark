// pluginViews.ts — плагинные тир-1 представления как HtmlViewProvider (H2, Фаза 4).
//
// Плагин с `contributes.views[{kind,title,tier:1}]` становится ещё одним
// `HtmlViewProvider` в общем `viewRegistry` (ADR-0022, IDEA-003): нативный
// Markdown-предпросмотр остаётся дефолтным провайдером, а плагинные вкладки —
// выбираемые в `.view-switch`. Контроллер лишь:
//   — держит список представлений (приходит из Rust командой `plugin_views`);
//   — регистрирует/создаёт провайдер и `HtmlView` на каждый view через реестр;
//   — переключает вкладки и отдаёт активному view присланный HTML.
//
// Сам `HtmlView` (возвращаемый `createView`) владеет своим DOM-поведением:
// пишет санитизированный хостом HTML в контейнер и слушает клики
// `data-p-<plugin_id>-action/-payload`, возвращая их в плагин (`plugin_view_action`).
// Никакого доступа к `window`/`fetch`/ФС из контейнера нет.

import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage, pluginViewAction, pluginViews, type PluginViewInfo } from "./tauri";
import type { HtmlView, HtmlViewProvider, Json, ViewContext, ViewRegistry } from "./viewRegistry";
import { pluginColor } from "./pluginColor";

/** Публичный фасад контроллера: перестроение из Rust и освобождение ресурсов. */
export interface PluginViews {
  /** Запросить список представлений и перестроить вкладки/контейнер. */
  refresh(): Promise<void>;
  /**
   * Показать первое представление плагина (по `plugin_id`). Возвращает `false`,
   * если у плагина нет ни одного зарегистрированного представления (no-op).
   */
  openPlugin(pluginId: string): boolean;
  /** Есть ли у плагина зарегистрированное тир-1 представление. */
  hasView(pluginId: string): boolean;
  /** Снять слушатели и очистить вкладки/контейнер. */
  dispose(): void;
}

/** Расширение `HtmlView`: HTML приходит событием хоста, а не из снапшота документа. */
export interface PluginHtmlView extends HtmlView {
  /** Применить HTML, присланный плагином (уже санитизирован хостом). */
  applyHtml(html: string): void;
  /** Активна ли вкладка: активный view пишет в контейнер и обрабатывает клики. */
  setActive(active: boolean): void;
}

export interface PluginViewsOptions {
  /** Общий реестр представлений: плагинные провайдеры живут рядом с preview. */
  registry: ViewRegistry;
  /** Контекст view (`ViewContext`, §5.4); плагинному тир-1 контенту не нужен. */
  ctx: ViewContext;
  /** `.view-switch` панели предпросмотра (core-вкладка уже в разметке). */
  switchEl: HTMLElement;
  /** Контейнер плагинного HTML (`#plugin-view`). */
  containerEl: HTMLElement;
  /** `#preview` — скрывается, пока активна плагинная вкладка. */
  previewEl: HTMLElement;
  /** Сообщение в статусбар (ошибки IPC/действия). */
  status(msg: string): void;
  /** Список представлений обновился (после `render`) — для зависимых панелей. */
  onViewsChanged?(): void;
}

interface PluginViewRuntime {
  info: PluginViewInfo;
  provider: HtmlViewProvider;
  view: PluginHtmlView;
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

/**
 * Создаёт `HtmlViewProvider` одного плагинного представления (тир-1, ADR-0022).
 *
 * `kind` — `view_id` (`<plugin_id>:<kind>`): уникален для нескольких плагинов с
 * одинаковым `kind`. `createView` возвращает [`PluginHtmlView`], который владеет
 * контейнером: пишет HTML и маршрутизирует клики обратно в плагин.
 */
function createPluginViewProvider(
  info: PluginViewInfo,
  containerEl: HTMLElement,
  status: (msg: string) => void
): HtmlViewProvider {
  return {
    kind: info.view_id,
    title: info.title,
    createView(_ctx: ViewContext, _opts: Json): PluginHtmlView {
      const pluginId = info.plugin_id;
      let html = "";
      let active = false;

      const onClick = (e: MouseEvent): void => {
        if (!active) return;
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
        const el = target?.closest<HTMLElement>(`[data-p-${pluginId}-action]`);
        if (!el) return;
        const action = el.getAttribute(`data-p-${pluginId}-action`);
        if (!action) return;
        e.preventDefault();
        const payload = parsePayload(el.getAttribute(`data-p-${pluginId}-payload`));
        void pluginViewAction(info.view_id, action, payload).catch((err) =>
          status(`Плагин: ${errorMessage(err)}`)
        );
      };
      containerEl.addEventListener("click", onClick);

      return {
        applyHtml(next: string): void {
          html = next;
          if (active) containerEl.innerHTML = html;
        },
        setActive(value: boolean): void {
          active = value;
          if (value) containerEl.innerHTML = html;
        },
        // Контент приходит событием `plugin-views-changed`, а не из документа.
        async render(): Promise<void> {},
        dispose(): void {
          containerEl.removeEventListener("click", onClick);
        },
      };
    },
  };
}

export function createPluginViews(opts: PluginViewsOptions): PluginViews {
  const { registry, ctx, switchEl, containerEl, previewEl, status } = opts;

  // Активный плагинный view; `null` — активна core-вкладка «Предпросмотр».
  let activeViewId: string | null = null;
  const runtimes = new Map<string, PluginViewRuntime>();
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
    for (const runtime of runtimes.values()) runtime.view.setActive(false);
    previewEl.hidden = false;
    containerEl.hidden = true;
    setActiveTab(null);
  }

  /** Показать плагинное представление (HTML пишет его `HtmlView`). */
  function showPlugin(viewId: string): void {
    const runtime = runtimes.get(viewId);
    if (!runtime) {
      showPreview();
      return;
    }
    activeViewId = viewId;
    previewEl.hidden = true;
    containerEl.hidden = false;
    for (const [id, other] of runtimes) other.view.setActive(id === viewId);
    setActiveTab(viewId);
  }

  /** Синхронизирует созданные view с пришедшим списком (регистрация/удаление/HTML). */
  function syncRuntimes(list: PluginViewInfo[]): void {
    const seen = new Set<string>();
    for (const info of list) {
      seen.add(info.view_id);
      let runtime = runtimes.get(info.view_id);
      if (!runtime) {
        // Провайдер регистрируется в общем реестре (kind = view_id) и создаётся через него.
        const provider = createPluginViewProvider(info, containerEl, status);
        registry.registerHtml(provider);
        const view = registry.createHtmlView<PluginHtmlView>(info.view_id, ctx, {});
        runtime = { info, provider, view };
        runtimes.set(info.view_id, runtime);
      } else {
        runtime.info = info;
      }
      runtime.view.applyHtml(info.html);
    }
    // Представления, исчезнувшие из списка (выключение/перезагрузка плагина).
    for (const [id, runtime] of runtimes) {
      if (!seen.has(id)) {
        runtime.view.dispose();
        registry.unregisterHtml(id);
        runtimes.delete(id);
      }
    }
  }

  /** Пересобрать только плагинные вкладки (core-вкладка заморожена H1). */
  function renderTabs(list: PluginViewInfo[]): void {
    for (const old of switchEl.querySelectorAll<HTMLElement>(".vtab.plugin")) old.remove();
    for (const info of list) {
      const tab = document.createElement("button");
      tab.type = "button";
      tab.className = "vtab plugin";
      tab.dataset.view = info.view_id;
      tab.title = info.title;
      tab.style.setProperty("--pc", pluginColor(info.plugin_id));
      const origin = document.createElement("span");
      origin.className = "origin";
      tab.append(origin, info.title);
      switchEl.append(tab);
    }
  }

  /** Перестроить вкладки и восстановить активное состояние. */
  function render(list: PluginViewInfo[]): void {
    syncRuntimes(list);
    renderTabs(list);
    // Активный плагин исчез (перезагрузка/отключение) — возвращаемся к preview.
    if (activeViewId !== null && !runtimes.has(activeViewId)) {
      showPreview();
    } else if (activeViewId !== null) {
      showPlugin(activeViewId); // контент мог обновиться тем же событием
    } else {
      showPreview();
    }
    opts.onViewsChanged?.();
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

  switchEl.addEventListener("click", onSwitchClick);

  return {
    refresh,

    openPlugin(pluginId: string): boolean {
      for (const runtime of runtimes.values()) {
        if (runtime.info.plugin_id === pluginId) {
          showPlugin(runtime.info.view_id);
          return true;
        }
      }
      return false;
    },

    hasView(pluginId: string): boolean {
      for (const runtime of runtimes.values()) {
        if (runtime.info.plugin_id === pluginId) return true;
      }
      return false;
    },

    dispose(): void {
      disposed = true;
      switchEl.removeEventListener("click", onSwitchClick);
      for (const [id, runtime] of runtimes) {
        runtime.view.dispose();
        registry.unregisterHtml(id);
      }
      runtimes.clear();
      for (const tab of switchEl.querySelectorAll<HTMLElement>(".vtab.plugin")) tab.remove();
      containerEl.innerHTML = "";
      containerEl.hidden = true;
      previewEl.hidden = false;
      setActiveTab(null);
      void unlistenPromise.then((fn) => fn?.());
    },
  };
}
