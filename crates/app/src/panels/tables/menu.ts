// Выпадающий фильтр колонки.

import { apply } from "./apply";
import { compareCells } from "./compare";
import { bodyRows, cellText } from "./dom";
import { detectColumnKind } from "./parse";
import type { TableEntry } from "./types";

export interface MenuController {
  close(): void;
  open(anchor: HTMLElement, table: HTMLTableElement, col: number, entry: TableEntry): void;
  nextAnchorId(): number;
}

export function createMenuController(): MenuController {
  let openMenu: HTMLElement | null = null;
  let openAnchor: HTMLElement | null = null;
  let outsideHandler: ((e: MouseEvent) => void) | null = null;
  let escapeHandler: ((e: KeyboardEvent) => void) | null = null;
  let anchorSeq = 0;

  function closeMenu(restoreFocus = false) {
    const anchor = openAnchor;
    openMenu?.remove();
    openMenu = null;
    openAnchor = null;
    anchor?.setAttribute("aria-expanded", "false");
    if (outsideHandler) {
      document.removeEventListener("mousedown", outsideHandler, true);
      outsideHandler = null;
    }
    if (escapeHandler) {
      document.removeEventListener("keydown", escapeHandler, true);
      escapeHandler = null;
    }
    if (restoreFocus && anchor?.isConnected) anchor.focus({ preventScroll: true });
  }

  function syncFromCheckboxes(menu: HTMLElement, entry: TableEntry, col: number) {
    const items = Array.from(menu.querySelectorAll<HTMLElement>(".col-filter-item"));
    const checked = items.filter((el) => (el.querySelector("input") as HTMLInputElement).checked);
    if (items.length === 0 || checked.length === items.length) {
      entry.state.colFilters.delete(col);
    } else {
      entry.state.colFilters.set(col, new Set(checked.map((el) => el.dataset.value ?? "")));
    }
    apply(entry);
  }

  function openFilterMenu(anchor: HTMLElement, table: HTMLTableElement, col: number, entry: TableEntry) {
    const wasOpenForSame =
      openMenu?.dataset.anchor === anchor.getAttribute("data-anchor-id");
    closeMenu();
    if (wasOpenForSame) return;

    const menu = document.createElement("div");
    menu.className = "col-filter-menu";
    menu.setAttribute("role", "dialog");
    menu.setAttribute("aria-label", anchor.getAttribute("aria-label") ?? "Фильтр по столбцу");
    menu.setAttribute("data-anchor", anchor.getAttribute("data-anchor-id") ?? "");

    const uniq = new Map<string, number>();
    for (const r of bodyRows(table)) {
      const v = cellText(r.cells[col]).trim();
      if (v) uniq.set(v, (uniq.get(v) ?? 0) + 1);
    }
    const allValues = [...uniq.keys()];
    const kind = detectColumnKind(allValues);
    allValues.sort((a, b) => compareCells(a, b, kind));

    const active = entry.state.colFilters.get(col);

    const mini = document.createElement("input");
    mini.type = "search";
    mini.setAttribute("aria-label", "Поиск значения");
    mini.placeholder = "Найти значение…";
    mini.autocomplete = "off";
    mini.addEventListener("input", () => {
      const q = mini.value.toLowerCase();
      for (const item of menu.querySelectorAll<HTMLElement>(".col-filter-item")) {
        item.hidden = !(item.dataset.value ?? "").toLowerCase().includes(q);
      }
    });

    const list = document.createElement("div");
    list.className = "col-filter-list";
    for (const v of allValues) {
      const label = document.createElement("label");
      label.className = "col-filter-item";
      label.dataset.value = v;
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = !active || active.has(v);
      cb.addEventListener("change", () => syncFromCheckboxes(menu, entry, col));
      const span = document.createElement("span");
      span.textContent = v;
      const cnt = document.createElement("em");
      cnt.textContent = String(uniq.get(v));
      label.append(cb, span, cnt);
      list.appendChild(label);
    }
    if (allValues.length === 0) {
      const empty = document.createElement("div");
      empty.className = "col-filter-empty";
      empty.textContent = "Нет данных";
      list.appendChild(empty);
    }

    const footer = document.createElement("div");
    footer.className = "col-filter-footer";
    const mkBtn = (text: string, fn: () => void) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = text;
      b.addEventListener("click", fn);
      return b;
    };
    const setAll = (on: boolean) => {
      for (const cb of menu.querySelectorAll<HTMLInputElement>(".col-filter-item input")) {
        const item = cb.closest(".col-filter-item") as HTMLElement;
        if (!item.hidden) cb.checked = on;
      }
      syncFromCheckboxes(menu, entry, col);
    };
    footer.append(
      mkBtn("Все", () => setAll(true)),
      mkBtn("Ничего", () => setAll(false)),
    );

    menu.append(mini, list, footer);
    document.body.appendChild(menu);

    const rect = anchor.getBoundingClientRect();
    const w = 260;
    menu.style.position = "fixed";
    menu.style.left = Math.max(4, Math.min(rect.left, window.innerWidth - w - 8)) + "px";
    const below = rect.bottom + 2;
    menu.style.top =
      (below + menu.offsetHeight > window.innerHeight - 8
        ? Math.max(8, rect.top - menu.offsetHeight - 2)
        : below) + "px";
    openMenu = menu;
    openAnchor = anchor;
    anchor.setAttribute("aria-expanded", "true");
    mini.focus();

    outsideHandler = (e: MouseEvent) => {
      if (!menu.contains(e.target as Node) && e.target !== anchor) closeMenu();
    };
    document.addEventListener("mousedown", outsideHandler, true);

    escapeHandler = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      closeMenu(true);
    };
    document.addEventListener("keydown", escapeHandler, true);
  }

  return { close: closeMenu, open: openFilterMenu, nextAnchorId: () => ++anchorSeq };
}
