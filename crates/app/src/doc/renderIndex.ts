// renderIndex.ts вЂ” РµРґРёРЅС‹Р№ РёРЅРґРµРєСЃ СЂРµРЅРґРµСЂР° (DESIGN_DOC В§5.5).
//
// Р Р°РЅСЊС€Рµ `inspector.ts` Рё `scrollsync.ts` РЅРµР·Р°РІРёСЃРёРјРѕ СЃС‚СЂРѕРёР»Рё РѕРґРЅРё Рё С‚Рµ Р¶Рµ РєР°СЂС‚С‹
// byteв†”UTF-16 Рё СЃРїРёСЃРєРё Р±Р»РѕРєРѕРІ (РґРІРѕР№РЅР°СЏ СЂР°Р±РѕС‚Р°, РґРІРѕР№РЅРѕРµ СЂР°СЃС…РѕР¶РґРµРЅРёРµ). РўРµРїРµСЂСЊ
// РёРЅРґРµРєСЃ СЃС‚СЂРѕРёС‚СЃСЏ РћР”РРќ СЂР°Р· РЅР° СЂРµРІРёР·РёСЋ: `previewView` РІС‹Р·С‹РІР°РµС‚ `set()` СЃСЂР°Р·Сѓ
// РїРѕСЃР»Рµ СѓСЃС‚Р°РЅРѕРІРєРё `preview.innerHTML`, РїРѕС‚СЂРµР±РёС‚РµР»Рё С‡РёС‚Р°СЋС‚ РЅРµРёР·РјРµРЅСЏРµРјС‹Р№ СЃРЅРёРјРѕРє
// С‡РµСЂРµР· `current()`.
//
// РРЅРІР°СЂРёР°РЅС‚С‹:
// - `source` вЂ” С‚РµРєСЃС‚, СѓС€РµРґС€РёР№ РІ СЂРµРЅРґРµСЂ (РЅРµ С‚РµРєСѓС‰РёР№ Р±СѓС„РµСЂ СЂРµРґР°РєС‚РѕСЂР°): РёРјРµРЅРЅРѕ РїРѕ
//   РЅРµРјСѓ РїРѕСЃС‡РёС‚Р°РЅС‹ Р±Р°Р№С‚РѕРІС‹Рµ СЃРјРµС‰РµРЅРёСЏ `data-md`;
// - РїСЂРё СЃРјРµРЅРµ С‚РµРєСЃС‚Р° РёРЅРґРµРєСЃ СѓСЃС‚Р°СЂРµРІР°РµС‚ РґРѕ РЅРѕРІРѕРіРѕ СЂРµРЅРґРµСЂР° (Р·Р°С‰РёС‚Р° РѕС‚ В«РґРІРѕР№РЅРѕР№
//   РёСЃС‚РёРЅС‹В» Рѕ С‚РµРєСЃС‚Рµ), РїРѕС‚СЂРµР±РёС‚РµР»Рё РІРёРґСЏС‚ `null`.

import {
  type Block,
  type UnitMaps,
  buildLineStarts,
  buildUnitMaps,
  collectBlocks,
} from "../core/mapping";

/** РќРµРёР·РјРµРЅСЏРµРјС‹Р№ СЃРЅРёРјРѕРє РёРЅРґРµРєСЃР° РґР»СЏ РїРѕС‚СЂРµР±РёС‚РµР»РµР№ (inspector/scrollsync). */
export interface RenderIndexSnapshot {
  /** РўРµРєСЃС‚, РїРѕ РєРѕС‚РѕСЂРѕРјСѓ СЂРµРЅРґРµСЂРёР»СЃСЏ HTML Рё СЃС‡РёС‚Р°Р»РёСЃСЊ `data-md`. */
  readonly source: string;
  /** Р РµРІРёР·РёСЏ РґРѕРєСѓРјРµРЅС‚Р°, РєРѕС‚РѕСЂРѕР№ СЃРѕРѕС‚РІРµС‚СЃС‚РІСѓРµС‚ РёРЅРґРµРєСЃ. */
  readonly rev: number;
  readonly maps: UnitMaps;
  readonly lineStarts: Int32Array;
  readonly blocks: Block[];
}

export interface RenderIndex {
  /** РђРєС‚СѓР°Р»СЊРЅС‹Р№ СЃРЅРёРјРѕРє РёР»Рё `null` (РјРµР¶РґСѓ СЃРјРµРЅРѕР№ С‚РµРєСЃС‚Р° Рё СЂРµРЅРґРµСЂРѕРј). */
  current(): RenderIndexSnapshot | null;
  /** РџРµСЂРµСЃС‚СЂРѕРёС‚СЊ РёРЅРґРµРєСЃ РїРѕСЃР»Рµ СѓСЃС‚Р°РЅРѕРІРєРё `preview.innerHTML`. */
  set(source: string, rev: number, root: ParentNode): void;
  /** РЎР±СЂРѕСЃРёС‚СЊ: С‚РµРєСЃС‚ РёР·РјРµРЅРёР»СЃСЏ, РёРЅРґРµРєСЃ СѓСЃС‚Р°СЂРµР» РґРѕ РЅРѕРІРѕРіРѕ СЂРµРЅРґРµСЂР°. */
  clear(): void;
}

export function createRenderIndex(): RenderIndex {
  let snapshot: RenderIndexSnapshot | null = null;
  return {
    current: () => snapshot,
    set(source: string, rev: number, root: ParentNode): void {
      snapshot = {
        source,
        rev,
        maps: buildUnitMaps(source),
        lineStarts: buildLineStarts(source),
        blocks: collectBlocks(root),
      };
    },
    clear(): void {
      snapshot = null;
    },
  };
}
