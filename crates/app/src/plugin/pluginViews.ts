// pluginViews.ts вЂ” РїР»Р°РіРёРЅРЅС‹Рµ С‚РёСЂ-1 РїСЂРµРґСЃС‚Р°РІР»РµРЅРёСЏ РєР°Рє HtmlViewProvider (H2, Р¤Р°Р·Р° 4).
//
// РџР»Р°РіРёРЅ СЃ `contributes.views[{kind,title,tier:1}]` СЃС‚Р°РЅРѕРІРёС‚СЃСЏ РµС‰С‘ РѕРґРЅРёРј
// `HtmlViewProvider` РІ РѕР±С‰РµРј `viewRegistry` (ADR-0022, IDEA-003): РЅР°С‚РёРІРЅС‹Р№
// Markdown-РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂ РѕСЃС‚Р°С‘С‚СЃСЏ РґРµС„РѕР»С‚РЅС‹Рј РїСЂРѕРІР°Р№РґРµСЂРѕРј, РїР»Р°РіРёРЅРЅС‹Рµ РІРєР»Р°РґРєРё вЂ”
// РІС‹Р±РёСЂР°РµРјС‹РјРё РІ `.view-switch`.
//
// РљРѕРЅС‚РµРЅС‚ СЃР°РЅРёС‚РёР·РёСЂСѓРµС‚ С…РѕСЃС‚; РєРѕРЅС‚СЂРѕР»Р»РµСЂ РїРёС€РµС‚ РµРіРѕ РІ РєРѕРЅС‚РµР№РЅРµСЂ Рё РІРѕР·РІСЂР°С‰Р°РµС‚
// РєР»РёРєРё `data-p-<plugin_id>-action/-payload` РІ РїР»Р°РіРёРЅ (`plugin_view_action`).
// Р”РѕСЃС‚СѓРїР° Рє `window`/`fetch`/Р¤РЎ РёР· РєРѕРЅС‚РµР№РЅРµСЂР° РЅРµС‚.

import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage, pluginViewAction, pluginViews, type PluginViewInfo } from "../core/tauri";
import type { DocumentSnapshot } from "../core/tauri";
import type { HtmlView, HtmlViewProvider, Json, ViewContext, ViewRegistry } from "../viewRegistry";
import { pluginColor } from "../plugin/pluginColor";

/** РџСѓР±Р»РёС‡РЅС‹Р№ С„Р°СЃР°Рґ РєРѕРЅС‚СЂРѕР»Р»РµСЂР°: РїРµСЂРµСЃС‚СЂРѕРµРЅРёРµ РёР· Rust Рё РѕСЃРІРѕР±РѕР¶РґРµРЅРёРµ СЂРµСЃСѓСЂСЃРѕРІ. */
export interface PluginViews {
  /** Р—Р°РїСЂРѕСЃРёС‚СЊ СЃРїРёСЃРѕРє РїСЂРµРґСЃС‚Р°РІР»РµРЅРёР№ Рё РїРµСЂРµСЃС‚СЂРѕРёС‚СЊ РІРєР»Р°РґРєРё/РєРѕРЅС‚РµР№РЅРµСЂ. */
  refresh(): Promise<void>;
  /** РџРѕРєР°Р·Р°С‚СЊ РїРµСЂРІРѕРµ РїСЂРµРґСЃС‚Р°РІР»РµРЅРёРµ РїР»Р°РіРёРЅР°; `false` вЂ” Сѓ РЅРµРіРѕ РёС… РЅРµС‚ (no-op). */
  openPlugin(pluginId: string): boolean;
  /** Р•СЃС‚СЊ Р»Рё Сѓ РїР»Р°РіРёРЅР° Р·Р°СЂРµРіРёСЃС‚СЂРёСЂРѕРІР°РЅРЅРѕРµ С‚РёСЂ-1 РїСЂРµРґСЃС‚Р°РІР»РµРЅРёРµ. */
  hasView(pluginId: string): boolean;
  /** РЎРЅСЏС‚СЊ СЃР»СѓС€Р°С‚РµР»Рё Рё РѕС‡РёСЃС‚РёС‚СЊ РІРєР»Р°РґРєРё/РєРѕРЅС‚РµР№РЅРµСЂ. */
  dispose(): void;
}

/** Р Р°СЃС€РёСЂРµРЅРёРµ `HtmlView`: HTML РїСЂРёС…РѕРґРёС‚ СЃРѕР±С‹С‚РёРµРј С…РѕСЃС‚Р°, Р° РЅРµ РёР· СЃРЅР°РїС€РѕС‚Р° РґРѕРєСѓРјРµРЅС‚Р°. */
export interface PluginHtmlView extends HtmlView {
  /** РџСЂРёРјРµРЅРёС‚СЊ HTML, РїСЂРёСЃР»Р°РЅРЅС‹Р№ РїР»Р°РіРёРЅРѕРј (СѓР¶Рµ СЃР°РЅРёС‚РёР·РёСЂРѕРІР°РЅ С…РѕСЃС‚РѕРј). */
  applyHtml(html: string): void;
  /** РђРєС‚РёРІРЅР° Р»Рё РІРєР»Р°РґРєР°: Р°РєС‚РёРІРЅС‹Р№ view РїРёС€РµС‚ РІ РєРѕРЅС‚РµР№РЅРµСЂ Рё РѕР±СЂР°Р±Р°С‚С‹РІР°РµС‚ РєР»РёРєРё. */
  setActive(active: boolean): void;
}

export interface PluginViewsOptions {
  /** РћР±С‰РёР№ СЂРµРµСЃС‚СЂ РїСЂРµРґСЃС‚Р°РІР»РµРЅРёР№: РїР»Р°РіРёРЅРЅС‹Рµ РїСЂРѕРІР°Р№РґРµСЂС‹ Р¶РёРІСѓС‚ СЂСЏРґРѕРј СЃ preview. */
  registry: ViewRegistry;
  /** РљРѕРЅС‚РµРєСЃС‚ view (`ViewContext`, В§5.4); РїР»Р°РіРёРЅРЅРѕРјСѓ С‚РёСЂ-1 РєРѕРЅС‚РµРЅС‚Сѓ РЅРµ РЅСѓР¶РµРЅ. */
  ctx: ViewContext;
  /** `.view-switch` РїР°РЅРµР»Рё РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂР° (core-РІРєР»Р°РґРєР° СѓР¶Рµ РІ СЂР°Р·РјРµС‚РєРµ). */
  switchEl: HTMLElement;
  /** РљРѕРЅС‚РµР№РЅРµСЂ РїР»Р°РіРёРЅРЅРѕРіРѕ HTML (`#plugin-view`). */
  containerEl: HTMLElement;
  /** `#preview` вЂ” СЃРєСЂС‹РІР°РµС‚СЃСЏ, РїРѕРєР° Р°РєС‚РёРІРЅР° РїР»Р°РіРёРЅРЅР°СЏ РІРєР»Р°РґРєР°. */
  previewEl: HTMLElement;


  backButtonEl: HTMLElement | null;
  /** РЎРѕРѕР±С‰РµРЅРёРµ РІ СЃС‚Р°С‚СѓСЃР±Р°СЂ (РѕС€РёР±РєРё IPC/РґРµР№СЃС‚РІРёСЏ). */
  status(msg: string): void;
  /** РЎРїРёСЃРѕРє РїСЂРµРґСЃС‚Р°РІР»РµРЅРёР№ РѕР±РЅРѕРІРёР»СЃСЏ (РїРѕСЃР»Рµ `render`) вЂ” РґР»СЏ Р·Р°РІРёСЃРёРјС‹С… РїР°РЅРµР»РµР№. */
  onViewsChanged?(): void;
}

interface PluginViewRuntime {
  info: PluginViewInfo;
  provider: HtmlViewProvider;
  view: PluginHtmlView;
}

/** Р Р°Р·Р±РѕСЂ payload РёР· `data-p-*-payload`: JSON, РёРЅР°С‡Рµ СЃС‚СЂРѕРєР°, РёРЅР°С‡Рµ `undefined`. */
function parsePayload(raw: string | null): Json | undefined {
  if (raw === null) return undefined;
  try {
    return JSON.parse(raw) as Json;
  } catch {
    return raw; // РЅРµ JSON вЂ” РїРµСЂРµРґР°С‘Рј РїР»Р°РіРёРЅСѓ РєР°Рє РµСЃС‚СЊ
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
            void openUrl(href).catch((err) => status(`РЎСЃС‹Р»РєР°: ${errorMessage(err)}`));
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
          status(`РџР»Р°РіРёРЅ: ${errorMessage(err)}`)
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
      showPlugin(activeViewId); // РєРѕРЅС‚РµРЅС‚ РјРѕРі РѕР±РЅРѕРІРёС‚СЊСЃСЏ С‚РµРј Р¶Рµ СЃРѕР±С‹С‚РёРµРј
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
      if (!disposed && seq === refreshSeq) status(`РџР»Р°РіРёРЅС‹: ${errorMessage(e)}`);
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
