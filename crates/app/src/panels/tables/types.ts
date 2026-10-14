// Типы состояния таблицы.

export interface TableState {
  sortCol: number | null;
  sortDir: 1 | -1;
  global: string;
  colFilters: Map<number, Set<string>>;
}

export interface TableEntry {
  wrap: HTMLElement;
  table: HTMLTableElement;
  state: TableState;
  baseOrder: HTMLTableRowElement[];
  kinds: ("num" | "date" | "str")[];
}
