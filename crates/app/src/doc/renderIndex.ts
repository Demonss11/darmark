// renderIndex.ts — единый индекс рендера (DESIGN_DOC §5.5).
//
// Раньше `inspector.ts` и `scrollsync.ts` независимо строили одни и те же карты
// byte↔UTF-16 и списки блоков (двойная работа, двойное расхождение). Теперь
// индекс строится ОДИН раз на ревизию: `previewView` вызывает `set()` сразу
// после установки `preview.innerHTML`, потребители читают неизменяемый снимок
// через `current()`.
//
// Инварианты:
// - `source` — текст, ушедший в рендер (не текущий буфер редактора): именно по
//   нему посчитаны байтовые смещения `data-md`;
// - при смене текста индекс устаревает до нового рендера (защита от «двойной
//   истины» о тексте), потребители видят `null`.

import {
  type Block,
  type UnitMaps,
  buildLineStarts,
  buildUnitMaps,
  collectBlocks,
} from "../core/mapping";

interface RenderIndexSnapshot {
  readonly source: string;
  readonly rev: number;
  readonly maps: UnitMaps;
  readonly lineStarts: Int32Array;
  readonly blocks: Block[];
}

export interface RenderIndex {
  current(): RenderIndexSnapshot | null;
  set(source: string, rev: number, root: ParentNode): void;
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
