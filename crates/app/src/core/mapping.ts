// mapping.ts — чистая логика сопоставления исходника и блоков предпросмотра.
//
// Только конвертация единиц (байты UTF-8 ↔ UTF-16 code units), начала
// логических строк и поиск блоков по байтовому смещению. Одну и ту же логику
// используют инспектор (`inspector.ts`) и синхронизация скролла (`scrollsync.ts`),
// поэтому она вынесена сюда, а не продублирована.
//
// Единственная DOM-зависимость — `collectBlocks`/`parseRange`: они читают
// `data-md` из готового предпросмотра. Всё остальное тестируемо без DOM.

export interface Block {
  start: number;
  end: number;
  el: HTMLElement;
}

export interface UnitMaps {
  b2u: Int32Array;
  u2b: Int32Array;
}

function utf8ByteLength(code: number): number {
  if (code < 0x80) return 1;
  if (code < 0x800) return 2;
  if (code < 0x10000) return 3;
  return 4;
}

export function buildUnitMaps(text: string): UnitMaps {
  let byteLen = 0;
  for (const ch of text) byteLen += utf8ByteLength(ch.codePointAt(0)!);

  const b2u = new Int32Array(byteLen + 1);
  const u2b = new Int32Array(text.length + 1);
  let byte = 0;
  let unit = 0;
  for (const ch of text) {
    const code = ch.codePointAt(0)!;
    const bl = utf8ByteLength(code);
    const ul = code > 0xffff ? 2 : 1;
    for (let i = 0; i < bl; i++) b2u[byte + i] = unit;
    for (let i = 0; i < ul; i++) u2b[unit + i] = byte;
    byte += bl;
    unit += ul;
  }
  b2u[byte] = unit;
  u2b[unit] = byte;
  return { b2u, u2b };
}

export function bytesToUnits(maps: UnitMaps, b: number): number {
  const i = Math.min(Math.max(b, 0), maps.b2u.length - 1);
  return maps.b2u[i]!;
}

export function byteToUtf16(text: string, byte: number): number {
  let consumed = 0;
  let unit = 0;
  for (const ch of text) {
    const bl = utf8ByteLength(ch.codePointAt(0)!);
    if (byte < consumed + bl) return unit;
    consumed += bl;
    unit += ch.length;
  }
  return unit;
}

export function unitsToBytes(maps: UnitMaps, u: number): number {
  const i = Math.min(Math.max(u, 0), maps.u2b.length - 1);
  return maps.u2b[i]!;
}

export function buildLineStarts(text: string): Int32Array {
  const starts: number[] = [0];
  for (let i = 0; i < text.length; i++) {
    if (text.charCodeAt(i) === 10 /* \n */) starts.push(i + 1);
  }
  return Int32Array.from(starts);
}

export function findLineAt(lineStarts: Int32Array, unit: number): number {
  let lo = 0;
  let hi = lineStarts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (lineStarts[mid]! <= unit) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

export function parseRange(el: HTMLElement): { start: number; end: number } | null {
  const raw = el.getAttribute("data-md");
  if (!raw) return null;
  const [s, e] = raw.split(",");
  const start = Number(s);
  const end = Number(e);
  if (!Number.isFinite(start) || !Number.isFinite(end)) return null;
  return { start, end };
}

export function collectBlocks(root: ParentNode): Block[] {
  const list: Block[] = [];
  for (const el of root.querySelectorAll<HTMLElement>(".md-block[data-md]")) {
    const range = parseRange(el);
    if (range) list.push({ start: range.start, end: range.end, el });
  }
  list.sort((a, b) => a.start - b.start);
  return list;
}

export function findBlock(blocks: Block[] | null, bytePos: number): Block | null {
  if (!blocks || blocks.length === 0) return null;
  for (const block of blocks) {
    if (bytePos >= block.start && bytePos < block.end) return block;
  }
  const last = blocks[blocks.length - 1];
  return bytePos === last.end ? last : null;
}

export function findBlockContaining(blocks: Block[], bytePos: number): Block | null {
  for (const block of blocks) {
    if (bytePos >= block.start && bytePos < block.end) return block;
  }
  return null;
}

export function nextBlockAfter(blocks: Block[], bytePos: number): Block | null {
  for (const block of blocks) {
    if (block.start > bytePos) return block;
  }
  return null;
}
