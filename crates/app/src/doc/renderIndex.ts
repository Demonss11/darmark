













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
