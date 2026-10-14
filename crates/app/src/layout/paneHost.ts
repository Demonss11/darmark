// paneHost.ts вЂ” РјРѕРЅС‚РёСЂРѕРІР°РЅРёРµ РґРµСЂРµРІР° РїР°РЅРµР»РµР№ РІ DOM (DESIGN_DOC В§5.2, В§5.5).
//
// РљР°СЂРєР°СЃ СЂР°Р·РјРµС‚РєРё СЃС‚Р°С‚РёС‡РµРЅ (`index.html`): СЃР»РѕС‚С‹ `.pane[data-pane]` СЃ
// `data-view`, СЂР°Р·РґРµР»РёС‚РµР»СЊ `.divider`, С€Р°РїРєР° `pane-head`/`view-switch`.
// paneHost РїСЂРёРјРµРЅСЏРµС‚ РјРѕРґРµР»СЊ layout вЂ” РїРѕРєР°Р·С‹РІР°РµС‚ РїСЂРёСЃСѓС‚СЃС‚РІСѓСЋС‰РёРµ РІ РґРµСЂРµРІРµ РїР°РЅРµР»Рё
// Рё СЃРєСЂС‹РІР°РµС‚ РѕС‚СЃСѓС‚СЃС‚РІСѓСЋС‰РёРµ.
//
// РРЅРІР°СЂРёР°РЅС‚ СЃРєСЂРѕР»Р»РµСЂРѕРІ: `#editor` Рё `#preview` вЂ” СЃР°РјРѕСЃС‚РѕСЏС‚РµР»СЊРЅС‹Рµ СЃРєСЂРѕР»Р»-СЌР»РµРјРµРЅС‚С‹
// РІРЅСѓС‚СЂРё СЃР»РѕС‚РѕРІ (e2e С‡РёС‚Р°РµС‚ РёС… `scrollTop`), paneHost РёС… РЅРµ РѕР±РѕСЂР°С‡РёРІР°РµС‚.

import { panes, type LayoutNode } from "./layout";
import type { PaneId } from "../core/ids";

export interface PaneHost {
  mount(layout: LayoutNode): void;
  /** РџРѕРјРµС‚РёС‚СЊ РїР°РЅРµР»СЊ Р°РєС‚РёРІРЅРѕР№ (С„РѕРєСѓСЃ). */
  setActive(paneId: PaneId): void;
}

export function createPaneHost(root: HTMLElement): PaneHost {
  const slots = new Map<string, HTMLElement>();
  for (const el of root.querySelectorAll<HTMLElement>(".pane[data-pane]")) {
    const id = el.dataset.pane;
    if (id) slots.set(id, el);
  }
  const divider = root.querySelector<HTMLElement>(".divider");

  function setActive(paneId: PaneId): void {
    for (const [id, el] of slots) el.classList.toggle("active", id === paneId);
  }

  return {
    mount(layout: LayoutNode): void {
      const list = panes(layout);
      const visible = new Set(list.map((p) => p.id as string));
      for (const [id, el] of slots) {



        el.style.display = visible.has(id) ? "" : "none";
      }
      if (divider) divider.style.display = visible.size > 1 ? "" : "none";
      const first = list[0];
      if (first) setActive(first.id);
    },
    setActive,
  };
}
