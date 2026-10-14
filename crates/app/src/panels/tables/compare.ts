// Сравнение и фильтрация строк таблицы.

import { cellText } from "./dom";
import { toCellValue } from "./parse";
import type { TableState } from "./types";

export function rowMatches(row: HTMLTableRowElement, st: TableState): boolean {
  const gq = st.global.trim().toLowerCase();
  if (gq) {
    const text = Array.from(row.cells).map(cellText).join(" \u0001 ").toLowerCase();
    if (!text.includes(gq)) return false;
  }
  for (const [col, allowed] of st.colFilters) {
    const cell = row.cells[col];
    const txt = cellText(cell).trim();
    if (!allowed.has(txt)) return false;
  }
  return true;
}

export function compareCells(a: string, b: string, kind: "num" | "date" | "str"): number {
  if (kind === "str") {
    return a.trim().localeCompare(b.trim(), undefined, { numeric: true, sensitivity: "base" });
  }
  const ca = toCellValue(a), cb = toCellValue(b);
  const aOk = ca.kind === kind, bOk = cb.kind === kind;
  if (aOk && bOk) return ca.num - cb.num;
  if (aOk !== bOk) return aOk ? -1 : 1;
  if (!ca.str && !cb.str) return 0;
  if (!ca.str) return 1;
  if (!cb.str) return -1;
  return ca.str.localeCompare(cb.str, undefined, { numeric: true, sensitivity: "base" });
}
