// renderIndex.ts — единый индекс рендера (DESIGN_DOC §5.5, глоссарий §17).
//
// Раньше `inspector.ts` и `scrollsync.ts` независимо строили одни и те же
// карты byte↔UTF-16 и списки блоков из одного markdown (двойная работа,
// двойное расхождение). Теперь индекс строится ОДИН раз на ревизию:
// `previewView` вызывает `set()` сразу после установки `preview.innerHTML`,
// потребители читают неизменяемый снимок через `current()`.
//
// Инварианты:
// - `source` — текст, ушедший в рендер (не текущий буфер редактора):
//   именно по нему посчитаны байтовые смещения `data-md`;
// - при смене текста индекс устаревает до нового рендера (защита от
//   «двойной истины» о тексте), потребители видят `null`.

import {
  type Block,
  type UnitMaps,
  buildLineStarts,
  buildUnitMaps,
  collectBlocks,
} from "./mapping";

/** Неизменяемый снимок индекса для потребителей (inspector/scrollsync). */
export interface RenderIndexSnapshot {
  /** Текст, по которому рендерился HTML и считались `data-md`. */
  readonly source: string;
  /** Ревизия документа, которой соответствует индекс. */
  readonly rev: number;
  readonly maps: UnitMaps;
  readonly lineStarts: Int32Array;
  readonly blocks: Block[];
}

export interface RenderIndex {
  /** Актуальный снимок или `null` (между сменой текста и рендером). */
  current(): RenderIndexSnapshot | null;
  /** Перестроить индекс после установки `preview.innerHTML`. */
  set(source: string, rev: number, root: ParentNode): void;
  /** Сбросить: текст изменился, индекс устарел до нового рендера. */
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
