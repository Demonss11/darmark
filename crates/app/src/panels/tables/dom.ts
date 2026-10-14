// DOM-хелперы таблицы.

export function cellText(td: HTMLElement | undefined): string {
  return td?.textContent ?? "";
}

export function headerCells(table: HTMLTableElement): HTMLTableCellElement[] {
  const thead = table.tHead;
  if (!thead || thead.rows.length === 0) return [];
  return Array.from(thead.rows[thead.rows.length - 1].cells);
}

export function bodyRows(table: HTMLTableElement): HTMLTableRowElement[] {
  return Array.from(table.tBodies).flatMap((tb) => Array.from(tb.rows));
}
