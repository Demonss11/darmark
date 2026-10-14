// mapping.ts вЂ” С‡РёСЃС‚Р°СЏ Р»РѕРіРёРєР° СЃРѕРїРѕСЃС‚Р°РІР»РµРЅРёСЏ РёСЃС…РѕРґРЅРёРєР° Рё Р±Р»РѕРєРѕРІ РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂР°.
//
// РўРѕР»СЊРєРѕ РєРѕРЅРІРµСЂС‚Р°С†РёСЏ РµРґРёРЅРёС† (Р±Р°Р№С‚С‹ UTF-8 в†” UTF-16 code units), РЅР°С‡Р°Р»Р°
// Р»РѕРіРёС‡РµСЃРєРёС… СЃС‚СЂРѕРє Рё РїРѕРёСЃРє Р±Р»РѕРєРѕРІ РїРѕ Р±Р°Р№С‚РѕРІРѕРјСѓ СЃРјРµС‰РµРЅРёСЋ. РћРґРЅСѓ Рё С‚Сѓ Р¶Рµ Р»РѕРіРёРєСѓ
// РёСЃРїРѕР»СЊР·СѓСЋС‚ РёРЅСЃРїРµРєС‚РѕСЂ (`inspector.ts`) Рё СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ СЃРєСЂРѕР»Р»Р° (`scrollsync.ts`),
// РїРѕСЌС‚РѕРјСѓ РѕРЅР° РІС‹РЅРµСЃРµРЅР° СЃСЋРґР°, Р° РЅРµ РїСЂРѕРґСѓР±Р»РёСЂРѕРІР°РЅР°.
//
// Р•РґРёРЅСЃС‚РІРµРЅРЅР°СЏ DOM-Р·Р°РІРёСЃРёРјРѕСЃС‚СЊ вЂ” `collectBlocks`/`parseRange`: РѕРЅРё С‡РёС‚Р°СЋС‚
// `data-md` РёР· РіРѕС‚РѕРІРѕРіРѕ РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂР°. Р’СЃС‘ РѕСЃС‚Р°Р»СЊРЅРѕРµ С‚РµСЃС‚РёСЂСѓРµРјРѕ Р±РµР· DOM.

export interface Block {
  start: number; // Р±Р°Р№С‚РѕРІС‹Р№ offset (inclusive)
  end: number;   // Р±Р°Р№С‚РѕРІС‹Р№ offset (exclusive)
  el: HTMLElement;
}

export interface UnitMaps {
  b2u: Int32Array; // Р±Р°Р№С‚РѕРІС‹Р№ РёРЅРґРµРєСЃ в†’ UTF-16 code units
  u2b: Int32Array; // UTF-16 code units в†’ Р±Р°Р№С‚РѕРІС‹Р№ РёРЅРґРµРєСЃ
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

/** РџСЂРµС„РёРєСЃРЅС‹Р№ РјР°СЃСЃРёРІ UTF-16-РёРЅРґРµРєСЃРѕРІ РЅР°С‡Р°Р» Р»РѕРіРёС‡РµСЃРєРёС… СЃС‚СЂРѕРє (РїРѕ \n); lineStarts[0] === 0. */
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

/** Р Р°Р·РјРµС‡РµРЅРЅС‹Рµ Р±Р»РѕРєРё РёР· РєРѕСЂРЅСЏ (РѕР±С‹С‡РЅРѕ вЂ” РїСЂРµРґРїСЂРѕСЃРјРѕС‚СЂР°), РїРѕ РІРѕР·СЂР°СЃС‚Р°РЅРёСЋ start. */
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

/** РџРµСЂРІС‹Р№ Р±Р»РѕРє, РЅР°С‡РёРЅР°СЋС‰РёР№СЃСЏ РїРѕСЃР»Рµ РїРѕР·РёС†РёРё (РґР»СЏ В«РєР°СЂРµС‚РєРё РІ РІРѕР·РґСѓС…РµВ»). */
export function nextBlockAfter(blocks: Block[], bytePos: number): Block | null {
  for (const block of blocks) {
    if (block.start > bytePos) return block;
  }
  return null;
}
