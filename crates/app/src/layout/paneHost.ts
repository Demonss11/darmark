









import { panes, type LayoutNode } from "./layout";
import type { PaneId } from "../core/ids";

interface PaneHost {
  mount(layout: LayoutNode): void;
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
