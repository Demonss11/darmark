// layout.ts — модель панелей (DESIGN_DOC §5.2): дерево Pane/Split и чистые
// операции над ним. Без DOM: результат применяет paneHost/main.
//
// v1: глубина дерева 1 — либо один Pane, либо Split из двух Pane. Операции
// возвращают НОВОЕ дерево либо LayoutError (вход не мутируется).

import type { PaneId, ViewId } from "../core/ids";

export interface Pane {
  kind: "pane";
  id: PaneId;
  views: ViewId[];
  active: ViewId;
}

interface Split {
  kind: "split";
  id: string;
  dir: "row" | "column";
  children: [LayoutNode, LayoutNode];
}

export type LayoutNode = Pane | Split;

const MAX_PANES = 2;

export class LayoutError extends Error {}

type LayoutOp =
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
        return new LayoutError(`MAX_PANES = ${MAX_PANES} — уже достигнуто`);
      }
      const target = list.find((p) => p.id === op.target);
      if (!target) return new LayoutError(`Панель не найдена: ${op.target}`);
      if (list.some((p) => p.id === op.pane.id)) {
        return new LayoutError(`Панель уже есть: ${op.pane.id}`);
      }
      return { kind: "split", id: "root", dir: op.dir, children: [target, op.pane] };
    }
    case "closePane": {
      const list = panes(node);
      if (list.length <= 1) return new LayoutError("Нельзя закрыть единственную панель");
      const remaining = list.filter((p) => p.id !== op.pane);
      if (remaining.length === list.length) {
        return new LayoutError(`Панель не найдена: ${op.pane}`);
      }
      return remaining[0]!;
    }
  }
}
