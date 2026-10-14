// tables.ts — Excel-подобное поведение таблиц в предпросмотре:
// сортировка по клику на заголовок, глобальный поиск и фильтры значений по колонкам.
// Всё живёт на фронтенде (TS), ядро md-core не трогается.

import { apply, makeTools } from "./apply";
import { bodyRows, cellText, headerCells } from "./dom";
import { createMenuController, type MenuController } from "./menu";
import { detectColumnKind } from "./parse";
import type { TableEntry, TableState } from "./types";

const registry = new WeakMap<HTMLTableElement, TableEntry>();

function enhance(table: HTMLTableElement, menu: MenuController) {
  if (registry.has(table)) return;

  const headers = headerCells(table);
  if (headers.length === 0) return;

  const entryRef: { current: TableEntry | null } = { current: null };
  const tools = makeTools(entryRef);

  const scrollWrap = document.createElement("div");
  scrollWrap.className = "table-scroll";

  const wrap = document.createElement("div");
  wrap.className = "table-enhanced";
  table.parentNode?.insertBefore(wrap, table);
  wrap.append(tools, scrollWrap);
  scrollWrap.appendChild(table);

  const state: TableState = { sortCol: null, sortDir: 1, global: "", colFilters: new Map() };
  const body = bodyRows(table);
  const colCount = headers.length || (body[0]?.cells.length ?? 0);
  const colValues: string[][] = Array.from({ length: colCount }, () => []);
  for (const r of body) {
    for (let c = 0; c < colCount; c++) colValues[c].push(cellText(r.cells[c]));
  }
  const entry: TableEntry = {
    wrap,
    table,
    state,
    baseOrder: body,
    kinds: colValues.map(detectColumnKind),
  };
  entryRef.current = entry;
  registry.set(table, entry);

  headers.forEach((th, i) => {
    const thLabel = th.textContent?.trim() ?? "";
    const sortBtn = document.createElement("button");
    sortBtn.type = "button";
    sortBtn.className = "col-sort-btn";
    if (th.querySelector("a, button, input, select, textarea, label")) {
      sortBtn.textContent = "↕";
      th.appendChild(sortBtn);
    } else {
      while (th.firstChild) sortBtn.appendChild(th.firstChild);
      th.appendChild(sortBtn);
    }
    sortBtn.setAttribute("aria-label", `Сортировать по столбцу «${thLabel}»`);

    const funnel = document.createElement("button");
    funnel.type = "button";
    funnel.className = "col-filter-btn";
    funnel.textContent = "▾";
    funnel.title = "Фильтр по столбцу";
    funnel.setAttribute("aria-label", `Фильтр по столбцу «${thLabel}»`);
    funnel.setAttribute("aria-expanded", "false");
    funnel.setAttribute("data-anchor-id", String(menu.nextAnchorId()));
    funnel.addEventListener("click", (ev) => {
      ev.stopPropagation();
      menu.open(funnel, table, i, entry);
    });
    th.appendChild(funnel);

    const toggle = () => {
      if (state.sortCol !== i) { state.sortCol = i; state.sortDir = 1; }
      else if (state.sortDir === 1) state.sortDir = -1;
      else { state.sortCol = null; state.sortDir = 1; }
      apply(entry);
    };
    sortBtn.addEventListener("click", toggle);
  });

  apply(entry);
}

interface TablesController {
  enhance(): void;
  dispose(): void;
}

export function createTablesController(root: HTMLElement): TablesController {
  const menu = createMenuController();
  const onScroll = () => menu.close();
  root.addEventListener("scroll", onScroll, { passive: true });
  return {
    enhance() {
      menu.close();
      for (const table of root.querySelectorAll<HTMLTableElement>(
        "table:not(.md-frontmatter)",
      )) {
        enhance(table, menu);
      }
    },
    dispose() {
      menu.close();
      root.removeEventListener("scroll", onScroll);
    },
  };
}
