// Определение цели инспектирования (блок/ячейка/строка/колонка).

export const INTERACTIVE_SELECTOR = "button, input, select, a, .col-filter-menu, th";

type InspectMode = "block" | "cell" | "row" | "col";

export interface InspectTarget {
  el: HTMLElement;
  mode: InspectMode;
}

export function resolveTarget(target: Element | null, shiftKey: boolean): InspectTarget | null {
  if (!target || typeof target.closest !== "function") return null;
  const th = target.closest("th[data-md]") as HTMLElement | null;
  if (th) return { el: th, mode: "col" };
  const tr = target.closest("tr[data-md]") as HTMLElement | null;
  const td = target.closest("td[data-md]") as HTMLElement | null;
  if (td && !shiftKey) return { el: td, mode: "cell" };
  if (tr) return { el: tr, mode: "row" };
  if (td) return { el: td, mode: "cell" };
  const block = target.closest(".md-block[data-md]") as HTMLElement | null;
  if (block) return { el: block, mode: "block" };
  return null;
}
