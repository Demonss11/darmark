// paneHost.ts — монтирование дерева панелей в DOM (DESIGN_DOC §5.2, §5.5).
//
// Каркас разметки: слоты `.pane[data-pane]` с
// `data-view`, разделитель `.divider`, шапка панели `pane-head`/`view-switch`.
// В v1 слоты статичны (`index.html`), а paneHost применяет модель layout:
// показывает панели, присутствующие в дереве, и скрывает отсутствующие.
//
// Инвариант скроллеров: `#editor` и `#preview` — самостоятельные скролл-элементы
// внутри слотов (e2e читает их `scrollTop`), paneHost их не оборачивает.

import { panes, type LayoutNode } from "./layout";
import type { PaneId } from "./ids";

export interface PaneHost {
  /** Применить дерево: видимость слотов, разделитель, активная панель. */
  mount(layout: LayoutNode): void;
  /** Пометить панель активной (фокус). */
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
        // Панель скрывается через `display`, но элемент (и вид) остаётся в DOM:
        // так `#preview` переживает выключение предпросмотра (AC-9) и не требует
        // пересоздания/повторного рендера.
        el.style.display = visible.has(id) ? "" : "none";
      }
      if (divider) divider.style.display = visible.size > 1 ? "" : "none";
      const first = list[0];
      if (first) setActive(first.id);
    },
    setActive,
  };
}
