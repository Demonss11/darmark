// pluginViews.ts — плагинные тир-1 представления как HtmlViewProvider (H2, Фаза 4).
//
// Плагин с `contributes.views[{kind,title,tier:1}]` становится ещё одним
// `HtmlViewProvider` в общем `viewRegistry` (ADR-0022, IDEA-003): нативный
// Markdown-предпросмотр остаётся дефолтным провайдером, плагинные вкладки —
// выбираемыми в `.view-switch`.
//
// Контент санитизирует хост; контроллер пишет его в контейнер и возвращает
// клики `data-p-<plugin_id>-action/-payload` в плагин (`plugin_view_action`).
// Доступа к `window`/`fetch`/ФС из контейнера нет.

import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage, pluginViewAction, pluginViews, type PluginViewInfo } from "../core/tauri";
import type { DocumentSnapshot } from "../core/tauri";
import type { HtmlView, HtmlViewProvider, Json, ViewContext, ViewRegistry } from "../viewRegistry";
import { pluginColor } from "../plugin/pluginColor";

interface PluginViews {
  refresh(): Promise<void>;
  openPlugin(pluginId: string): boolean;
  hasView(pluginId: string): boolean;
  dispose(): void;
}

interface PluginHtmlView extends HtmlView {
  applyHtml(html: string): void;
  setActive(active: boolean): void;
}

interface PluginViewsOptions {
  registry: ViewRegistry;
  ctx: ViewContext;
  switchEl: HTMLElement;
  containerEl: HTMLElement;
  previewEl: HTMLElement;
  backButtonEl: HTMLElement | null;
  status(msg: string): void;
  onViewsChanged?(): void;
}

interface PluginViewRuntime {
  info: PluginViewInfo;
  provider: HtmlViewProvider;
  view: PluginHtmlView;
}

function parsePayload(raw: string | null): Json | undefined {
  if (raw === null) return undefined;
  try {
    return JSON.parse(raw) as Json;
  } catch {
    return raw;
  }
}

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
        async render(_doc: DocumentSnapshot): Promise<void> {},
        dispose(): void {
          containerEl.removeEventListener("click", onClick);
        },
      };
    },
  };
}

export function createPluginViews(opts: PluginViewsOptions): PluginViews {
  const { registry, ctx, switchEl, containerEl, previewEl, backButtonEl, status } = opts;

  let activeViewId: string | null = null;
  const runtimes = new Map<string, PluginViewRuntime>();
  let disposed = false;
  let refreshSeq = 0;

  const unlistenPromise = listen("plugin-views-changed", () => {
    void refresh();
  }).catch(() => null);

  function setActiveTab(viewId: string | null): void {
    for (const tab of switchEl.querySelectorAll<HTMLElement>(".vtab")) {
      const isPlugin = tab.classList.contains("plugin");
      const active = viewId === null ? !isPlugin : isPlugin && tab.dataset.view === viewId;
      tab.classList.toggle("active", active);
      if (active) tab.setAttribute("aria-current", "true");
      else tab.removeAttribute("aria-current");
    }
  }

  function showPreview(): void {
    activeViewId = null;
    for (const runtime of runtimes.values()) runtime.view.setActive(false);
    previewEl.hidden = false;
    containerEl.hidden = true;
    if (backButtonEl) backButtonEl.hidden = true;
    setActiveTab(null);
  }

  function showPlugin(viewId: string): void {
    const runtime = runtimes.get(viewId);
    if (!runtime) {
      showPreview();
      return;
    }
    activeViewId = viewId;
    previewEl.hidden = true;
    containerEl.hidden = false;
    if (backButtonEl) backButtonEl.hidden = false;
    for (const [id, other] of runtimes) other.view.setActive(id === viewId);
    setActiveTab(viewId);
  }

  function syncRuntimes(list: PluginViewInfo[]): void {
    const seen = new Set<string>();
    for (const info of list) {
      seen.add(info.view_id);
      let runtime = runtimes.get(info.view_id);
      if (!runtime) {
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
    for (const [id, runtime] of runtimes) {
      if (!seen.has(id)) {
        runtime.view.dispose();
        registry.unregisterHtml(id);
        runtimes.delete(id);
      }
    }
  }

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

  function render(list: PluginViewInfo[]): void {
    syncRuntimes(list);
    renderTabs(list);
    if (activeViewId !== null && !runtimes.has(activeViewId)) {
      showPreview();
    } else if (activeViewId !== null) {
      showPlugin(activeViewId);
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

  const onBackClick = (): void => showPreview();
  backButtonEl?.addEventListener("click", onBackClick);

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
      backButtonEl?.removeEventListener("click", onBackClick);
      for (const [id, runtime] of runtimes) {
        runtime.view.dispose();
        registry.unregisterHtml(id);
      }
      runtimes.clear();
      for (const tab of switchEl.querySelectorAll<HTMLElement>(".vtab.plugin")) tab.remove();
      containerEl.innerHTML = "";
      containerEl.hidden = true;
      previewEl.hidden = false;
      if (backButtonEl) backButtonEl.hidden = true;
      setActiveTab(null);
      void unlistenPromise.then((fn) => fn?.());
    },
  };
}
