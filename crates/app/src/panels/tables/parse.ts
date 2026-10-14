// Разбор значений ячеек и определение типа колонки.

const NUM_RE = /^[-+]?\d{1,3}([ \u00A0\u202F]\d{3})*([.,]\d+)?\s*[%€$₽]?$|^[-+]?\d+([.,]\d+)?\s*[%€$₽]?$/;
const DATE_RE = /^\d{1,2}[./-]\d{1,2}[./-]\d{2,4}$|^\d{4}-\d{1,2}-\d{1,2}([ T]\d{1,2}[:.]\d{2}([:.]\d{2})?)?$/;

function parseNumber(s: string): number | null {
  const t = s.trim();
  if (!t || !NUM_RE.test(t)) return null;
  const cleaned = t
    .replace(/[\s\u00A0\u202F]/g, "")
    .replace(/[%,]/g, ".")
    .replace(/(\d)\.(\d)(?=.*\.)/g, "$1$2");
  const n = parseFloat(cleaned.replace(",", "."));
  return Number.isFinite(n) ? n : null;
}

function parseDate(s: string): number | null {
  const t = s.trim();
  if (!DATE_RE.test(t)) return null;
  let iso = t.replace(" ", "T");
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

export function toCellValue(raw: string): CellValue {
  const s = raw.trim();
  const n = parseNumber(s);
  if (n !== null) return { kind: "num", num: n, str: s.toLowerCase() };
  const d = parseDate(s);
  if (d !== null) return { kind: "date", num: d, str: s.toLowerCase() };
  return { kind: "str", num: 0, str: s.toLowerCase() };
}

export function detectColumnKind(values: string[]): "num" | "date" | "str" {
  let num = 0, date = 0, nonEmpty = 0;
  for (const v of values) {
    const c = toCellValue(v);
    if (!c.str) continue;
    nonEmpty++;
    if (c.kind === "num") num++;
    else if (c.kind === "date") date++;
  }
  if (nonEmpty === 0) return "str";
  if (num / nonEmpty >= 0.7) return "num";
  if (date / nonEmpty >= 0.7) return "date";
  return "str";
}
