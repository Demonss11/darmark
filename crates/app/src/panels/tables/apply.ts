// Применение сортировки/фильтров к таблице и панель инструментов.

import { compareCells, rowMatches } from "./compare";
import { bodyRows, cellText, headerCells } from "./dom";
import type { TableEntry } from "./types";

export function apply(entry: TableEntry) {
  const { table, state: st } = entry;
  const headers = headerCells(table);
  const rows = bodyRows(table);
  const colCount = headers.length || (rows[0]?.cells.length ?? 0);

  const kinds = entry.kinds;

  let visible = 0;
  for (const r of rows) {
    const show = rowMatches(r, st);
    r.hidden = !show;
    if (show) visible++;
  }

  const tbody = table.tBodies[0] ?? (table as unknown as HTMLElement);
  if (st.sortCol !== null && st.sortCol < colCount) {
    const col = st.sortCol;
    const kind = kinds[col] ?? "str";
    const idx = rows.map((r, i) => ({ r, i }));
    idx.sort((x, y) => {
      const cmp = compareCells(cellText(x.r.cells[col]), cellText(y.r.cells[col]), kind);
      return cmp !== 0 ? cmp * st.sortDir : x.i - y.i;
    });
    for (const { r } of idx) tbody.appendChild(r);
  } else {
    for (const r of entry.baseOrder) tbody.appendChild(r);
  }

  headers.forEach((th, i) => {
    const dir = st.sortCol === i ? st.sortDir : 0;
    th.classList.toggle("sorted-asc", dir === 1);
    th.classList.toggle("sorted-desc", dir === -1);
    th.classList.toggle("filtered", !!st.colFilters.get(i));
    th.setAttribute(
      "aria-sort",
      dir === 1 ? "ascending" : dir === -1 ? "descending" : "none",
    );
  });

  const counter = entry.wrap.querySelector<HTMLElement>(".table-count");
  if (counter) {
    counter.textContent =
      visible === rows.length ? `${rows.length} строк` : `${visible} из ${rows.length}`;
    counter.classList.toggle("active", visible !== rows.length);
  }

  const reset = entry.wrap.querySelector<HTMLButtonElement>(".table-reset");
  if (reset) {
    reset.disabled = st.sortCol === null && !st.global && st.colFilters.size === 0;
  }
}

export function makeTools(entryRef: { current: TableEntry | null }): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "table-tools";

  const search = document.createElement("input");
  search.type = "search";
  search.className = "table-search";
  search.placeholder = "Поиск по таблице…";
  search.setAttribute("aria-label", "Поиск по таблице");
  search.autocomplete = "off";
  search.addEventListener("input", () => {
    const e = entryRef.current!;
    e.state.global = search.value;
    apply(e);
  });

  const count = document.createElement("span");
  count.className = "table-count";

  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "table-reset";
  reset.textContent = "Сбросить";
  reset.title = "Снять сортировку, поиск и все фильтры";
  reset.addEventListener("click", () => {
    const e = entryRef.current!;
    e.state.sortCol = null;
    e.state.sortDir = 1;
    e.state.global = "";
    e.state.colFilters.clear();
    search.value = "";
    apply(e);
  });

  bar.append(search, count, reset);
  return bar;
}
