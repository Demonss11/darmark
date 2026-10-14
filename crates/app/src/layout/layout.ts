// layout.ts вЂ” РјРѕРґРµР»СЊ РїР°РЅРµР»РµР№ (DESIGN_DOC В§5.2): РґРµСЂРµРІРѕ Pane/Split Рё С‡РёСЃС‚С‹Рµ
// РѕРїРµСЂР°С†РёРё РЅР°Рґ РЅРёРј. Р‘РµР· DOM: СЂРµР·СѓР»СЊС‚Р°С‚ РїСЂРёРјРµРЅСЏРµС‚ paneHost/main.
//
// v1: РіР»СѓР±РёРЅР° РґРµСЂРµРІР° 1 вЂ” Р»РёР±Рѕ РѕРґРёРЅ Pane, Р»РёР±Рѕ Split РёР· РґРІСѓС… Pane. РћРїРµСЂР°С†РёРё
// РІРѕР·РІСЂР°С‰Р°СЋС‚ РќРћР’РћР• РґРµСЂРµРІРѕ Р»РёР±Рѕ LayoutError (РІС…РѕРґ РЅРµ РјСѓС‚РёСЂСѓРµС‚СЃСЏ).

import type { PaneId, ViewId } from "../core/ids";

export interface Pane {
  kind: "pane";
  id: PaneId;
  /** D4: РјРѕРґРµР»СЊ РїРѕРґ РІРєР»Р°РґРєРё Р·Р°Р»РѕР¶РµРЅР° СЃСЂР°Р·Сѓ; РІ v1 Сѓ РїР°РЅРµР»Рё РѕРґРёРЅ РІРёРґ. */
  views: ViewId[];
  active: ViewId;
}

export interface Split {
  kind: "split";
  id: string;
  dir: "row" | "column";
  children: [LayoutNode, LayoutNode];
}

export type LayoutNode = Pane | Split;

/** РћРіСЂР°РЅРёС‡РµРЅРёРµ v1. */
export const MAX_PANES = 2;

export class LayoutError extends Error {}

export type LayoutOp =
  | { t: "splitPane"; target: PaneId; dir: "row" | "column"; pane: Pane }
  | { t: "closePane"; pane: PaneId };

export function panes(node: LayoutNode): Pane[] {
  return node.kind === "pane"
    ? [node]
    : [...panes(node.children[0]), ...panes(node.children[1])];
}



export function applyLayout(node: LayoutNode, op: LayoutOp): LayoutNode | LayoutError {
  switch (op.t) {
    case "splitPane": {
      const list = panes(node);
      if (list.length >= MAX_PANES) {
        return new LayoutError(`MAX_PANES = ${MAX_PANES} вЂ” СѓР¶Рµ РґРѕСЃС‚РёРіРЅСѓС‚Рѕ`);
      }
      const target = list.find((p) => p.id === op.target);
      if (!target) return new LayoutError(`РџР°РЅРµР»СЊ РЅРµ РЅР°Р№РґРµРЅР°: ${op.target}`);
      if (list.some((p) => p.id === op.pane.id)) {
        return new LayoutError(`РџР°РЅРµР»СЊ СѓР¶Рµ РµСЃС‚СЊ: ${op.pane.id}`);
      }

      return { kind: "split", id: "root", dir: op.dir, children: [target, op.pane] };
    }
    case "closePane": {
      const list = panes(node);
      if (list.length <= 1) return new LayoutError("РќРµР»СЊР·СЏ Р·Р°РєСЂС‹С‚СЊ РµРґРёРЅСЃС‚РІРµРЅРЅСѓСЋ РїР°РЅРµР»СЊ");
      const remaining = list.filter((p) => p.id !== op.pane);
      if (remaining.length === list.length) {
        return new LayoutError(`РџР°РЅРµР»СЊ РЅРµ РЅР°Р№РґРµРЅР°: ${op.pane}`);
      }
      return remaining[0]!;
    }
  }
}
