// tables.ts — Excel-подобное поведение таблиц в предпросмотре:
// сортировка по клику на заголовок, глобальный поиск и фильтры значений по колонкам.
// Всё живёт на фронтенде (TS), ядро md-core не трогается.

interface TableState {
  sortCol: number | null;   // индекс сортируемой колонки
  sortDir: 1 | -1;          // 1 = возрастание, -1 = убывание
  global: string;           // строка глобального поиска
  colFilters: Map<number, Set<string>>; // фильтр «разрешённые значения» по колонке
}

interface TableEntry {
  wrap: HTMLElement;              // .table-enhanced (панель инструментов + обёртка со скроллом)
  table: HTMLTableElement;
  state: TableState;
  baseOrder: HTMLTableRowElement[]; // исходный порядок строк из markdown — для возврата без сортировки
}

const registry = new WeakMap<HTMLTableElement, TableEntry>();

// ---------- эвристика типов данных (как в Excel: число/дата/текст) ----------

const NUM_RE = /^[-+]?\d{1,3}([ \u00A0\u202F]\d{3})*([.,]\d+)?\s*[%€$₽]?$|^[-+]?\d+([.,]\d+)?\s*[%€$₽]?$/;
const DATE_RE = /^\d{1,2}[./-]\d{1,2}[./-]\d{2,4}$|^\d{4}-\d{1,2}-\d{1,2}([ T]\d{1,2}[:.]\d{2}([:.]\d{2})?)?$/;

function parseNumber(s: string): number | null {
  const t = s.trim();
  if (!t || !NUM_RE.test(t)) return null;
  const cleaned = t
    .replace(/[\s\u00A0\u202F]/g, "")
    .replace(/[%,]/g, ".")
    .replace(/(\d)\.(\d)(?=.*\.)/g, "$1$2"); // «1.234,56»: разделители тысяч схлопываем
  const n = parseFloat(cleaned.replace(",", "."));
  return Number.isFinite(n) ? n : null;
}

function parseDate(s: string): number | null {
  const t = s.trim();
  if (!DATE_RE.test(t)) return null;
  let iso = t.replace(" ", "T");
  // dd.mm.yyyy → yyyy-mm-dd (европейский формат)
  const m = /^(\d{1,2})[./-](\d{1,2})[./-](\d{4})$/.exec(iso);
  if (m) iso = `${m[3]}-${m[2].padStart(2, "0")}-${m[1].padStart(2, "0")}`;
  else {
    const d = /^(\d{1,2})[./-](\d{1,2})[./-](\d{2})$/.exec(iso);
    if (d) iso = `20${d[3]}-${d[2].padStart(2, "0")}-${d[1].padStart(2, "0")}`;
  }
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? null : ms;
}

type CellValue = { kind: "num" | "date" | "str"; num: number; str: string };

function toCellValue(raw: string): CellValue {
  const s = raw.trim();
  const n = parseNumber(s);
  if (n !== null) return { kind: "num", num: n, str: s.toLowerCase() };
  const d = parseDate(s);
  if (d !== null) return { kind: "date", num: d, str: s.toLowerCase() };
  return { kind: "str", num: 0, str: s.toLowerCase() };
}

// Тип колонки: если ≥70% непустых значений одного типа — сортируем как числа/даты.
function detectColumnKind(values: string[]): "num" | "date" | "str" {
  let num = 0, date = 0, nonEmpty = 0;
  for (const v of values) {
    const c = toCellValue(v);
    if (!c.str) continue; // пустая ячейка не учитывается
    nonEmpty++;
    if (c.kind === "num") num++;
    else if (c.kind === "date") date++;
  }
  if (nonEmpty === 0) return "str";
  if (num / nonEmpty >= 0.7) return "num";
  if (date / nonEmpty >= 0.7) return "date";
  return "str";
}

// ---------- доступ к данным таблицы ----------

function cellText(td: HTMLElement | undefined): string {
  return td?.textContent ?? "";
}

function headerCells(table: HTMLTableElement): HTMLTableCellElement[] {
  const thead = table.tHead;
  if (!thead || thead.rows.length === 0) return [];
  return Array.from(thead.rows[thead.rows.length - 1].cells);
}

function bodyRows(table: HTMLTableElement): HTMLTableRowElement[] {
  return Array.from(table.tBodies).flatMap((tb) => Array.from(tb.rows));
}

// ---------- фильтрация ----------

function rowMatches(row: HTMLTableRowElement, st: TableState): boolean {
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

// ---------- сравнение ячеек с учётом типа колонки ----------

function compareCells(a: string, b: string, kind: "num" | "date" | "str"): number {
  if (kind === "str") {
    return a.trim().localeCompare(b.trim(), undefined, { numeric: true, sensitivity: "base" });
  }
  const ca = toCellValue(a), cb = toCellValue(b);
  const aOk = ca.kind === kind, bOk = cb.kind === kind;
  if (aOk && bOk) return ca.num - cb.num;
  if (aOk !== bOk) return aOk ? -1 : 1; // распознанные значения идут первыми
  if (!ca.str && !cb.str) return 0;
  if (!ca.str) return 1; // пустые — в конце
  if (!cb.str) return -1;
  return ca.str.localeCompare(cb.str, undefined, { numeric: true, sensitivity: "base" });
}

// ---------- применение состояния к DOM ----------

function apply(entry: TableEntry) {
  const { table, state: st } = entry;
  const headers = headerCells(table);
  const rows = bodyRows(table);
  const colCount = headers.length || (rows[0]?.cells.length ?? 0);

  // типы колонок считаем по всем строкам тела — стабильно при пересортировке
  const colValues: string[][] = Array.from({ length: colCount }, () => []);
  for (const r of rows) {
    for (let c = 0; c < colCount; c++) colValues[c].push(cellText(r.cells[c]));
  }
  const kinds = colValues.map(detectColumnKind);

  // 1. видимость строк (поиск + фильтры по колонкам)
  let visible = 0;
  for (const r of rows) {
    const show = rowMatches(r, st);
    r.hidden = !show;
    if (show) visible++;
  }

  // 2. порядок строк: сортировка либо возврат к исходному порядку из markdown
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
    // «без сортировки» = именно исходный порядок, а не инверсия предыдущей сортировки
    for (const r of entry.baseOrder) tbody.appendChild(r);
  }

  // 3. индикаторы состояний в заголовках
  headers.forEach((th, i) => {
    th.classList.toggle("sorted-asc", st.sortCol === i && st.sortDir === 1);
    th.classList.toggle("sorted-desc", st.sortCol === i && st.sortDir === -1);
    th.classList.toggle("filtered", !!st.colFilters.get(i));
  });

  // 4. счётчик строк в панели инструментов
  const counter = entry.wrap.querySelector<HTMLElement>(".table-count");
  if (counter) {
    counter.textContent =
      visible === rows.length ? `${rows.length} строк` : `${visible} из ${rows.length}`;
    counter.classList.toggle("active", visible !== rows.length);
  }

  // 5. кнопка сброса активна только когда что-то применено
  const reset = entry.wrap.querySelector<HTMLButtonElement>(".table-reset");
  if (reset) {
    reset.disabled = st.sortCol === null && !st.global && st.colFilters.size === 0;
  }
}

// ---------- панель инструментов над таблицей ----------

function makeTools(entryRef: { current: TableEntry | null }): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "table-tools";

  const search = document.createElement("input");
  search.type = "search";
  search.className = "table-search";
  search.placeholder = "Поиск по таблице…";
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

// ---------- выпадающий фильтр значений колонки (в духе Excel) ----------

let openMenu: HTMLElement | null = null;
let outsideHandler: ((e: MouseEvent) => void) | null = null;

function closeMenu() {
  openMenu?.remove();
  openMenu = null;
  if (outsideHandler) {
    document.removeEventListener("mousedown", outsideHandler, true);
    outsideHandler = null;
  }
}

function syncFromCheckboxes(menu: HTMLElement, entry: TableEntry, col: number) {
  const items = Array.from(menu.querySelectorAll<HTMLElement>(".col-filter-item"));
  const checked = items.filter((el) => (el.querySelector("input") as HTMLInputElement).checked);
  if (items.length === 0 || checked.length === items.length) {
    entry.state.colFilters.delete(col); // все значения выбраны = фильтр снят
  } else {
    entry.state.colFilters.set(col, new Set(checked.map((el) => el.dataset.value ?? "")));
  }
  apply(entry);
}

function openFilterMenu(anchor: HTMLElement, table: HTMLTableElement, col: number, entry: TableEntry) {
  const wasOpenForSame =
    openMenu?.dataset.anchor === anchor.getAttribute("data-anchor-id");
  closeMenu();
  if (wasOpenForSame) return; // повторный клик по той же воронке — закрыть

  const menu = document.createElement("div");
  menu.className = "col-filter-menu";
  menu.setAttribute("data-anchor", anchor.getAttribute("data-anchor-id") ?? "");

  // уникальные значения колонки с количеством повторов
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
  mini.placeholder = "Найти значение…";
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
      if (!item.hidden) cb.checked = on; // как в Excel: влияет только на видимые
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
  menu.style.top = Math.min(rect.bottom + 2, window.innerHeight - 20) + "px";
  openMenu = menu;
  mini.focus();

  outsideHandler = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node) && e.target !== anchor) closeMenu();
  };
  document.addEventListener("mousedown", outsideHandler, true);
}

// ---------- украшение одной таблицы ----------

let anchorSeq = 0;

function enhance(table: HTMLTableElement) {
  if (registry.has(table)) return;

  const headers = headerCells(table);
  if (headers.length === 0) return; // без thead сортировать и фильтровать нечего

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
  const entry: TableEntry = { wrap, table, state, baseOrder: bodyRows(table) };
  entryRef.current = entry;
  registry.set(table, entry);

  // клик по заголовку: без сортировки → ↑ → ↓ → без сортировки
  headers.forEach((th, i) => {
    th.tabIndex = 0;
    th.setAttribute("role", "button");

    const funnel = document.createElement("button");
    funnel.type = "button";
    funnel.className = "col-filter-btn";
    funnel.textContent = "▾";
    funnel.title = "Фильтр по столбцу";
    funnel.setAttribute("data-anchor-id", String(++anchorSeq));
    funnel.addEventListener("click", (ev) => {
      ev.stopPropagation();
      openFilterMenu(funnel, table, i, entry);
    });
    th.appendChild(funnel);

    const toggle = () => {
      if (state.sortCol !== i) { state.sortCol = i; state.sortDir = 1; }
      else if (state.sortDir === 1) state.sortDir = -1;
      else { state.sortCol = null; state.sortDir = 1; }
      apply(entry);
    };
    th.addEventListener("click", toggle);
    th.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); toggle(); }
    });
  });

  apply(entry);
}

// закрыть меню при прокрутке предпросмотра (позиция fixed иначе «отъедет»)
export function attachMenuAutoClose(scroller: HTMLElement) {
  scroller.addEventListener("scroll", closeMenu, { passive: true });
}

// ---------- публичный API ----------

export function enhanceTables(root: ParentNode) {
  for (const table of root.querySelectorAll<HTMLTableElement>("table")) enhance(table);
}
