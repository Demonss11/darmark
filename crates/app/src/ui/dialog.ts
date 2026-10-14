// dialog.ts вЂ” РїРµСЂРµРёСЃРїРѕР»СЊР·СѓРµРјР°СЏ РјРѕРґР°Р»СЊРЅР°СЏ РёРЅС„СЂР°СЃС‚СЂСѓРєС‚СѓСЂР°.
//
// РћРґРёРЅ СЃС‚РµРє РґРёР°Р»РѕРіРѕРІ: РѕРІРµСЂР»РµР№ + РїР°РЅРµР»СЊ СЃ `role="dialog"`, focus-trap, Escape
// (capture + stopPropagation вЂ” РёРЅР°С‡Рµ Escape РґРѕР»РµС‚РёС‚ РґРѕ shell.ts Рё РІС‹РєР»СЋС‡РёС‚
// СЂРµР¶РёРј РёРЅСЃРїРµРєС‚РѕСЂР°), `inert` РЅР° РїСЂСЏРјС‹С… РґРµС‚СЏС… body (РєСЂРѕРјРµ #modal-root/#toast-host)
// Рё РІРѕР·РІСЂР°С‚ С„РѕРєСѓСЃР° РёРЅРёС†РёР°С‚РѕСЂСѓ. РЎР»СѓС€Р°С‚РµР»Рё СЃРЅРёРјР°СЋС‚СЃСЏ AbortController'РѕРј РїСЂРё
// Р·Р°РєСЂС‹С‚РёРё вЂ” РІРёСЃСЏС‡РёС… С…СЌРЅРґР»РµСЂРѕРІ РЅРµ РѕСЃС‚Р°С‘С‚СЃСЏ.

export type DialogVariant = "center" | "top";

export interface DialogOptions {
  /** Р—Р°РіРѕР»РѕРІРѕРє РґРёР°Р»РѕРіР° (СЂРµРЅРґРµСЂРёС‚СЃСЏ РєР°Рє `<h2>`, Р·Р°РґР°С‘С‚ РґРѕСЃС‚СѓРїРЅРѕРµ РёРјСЏ). */
  title: string;
  /** РЎРѕРґРµСЂР¶РёРјРѕРµ: СѓР·РµР», РІСЃС‚Р°РІР»СЏРµРјС‹Р№ РІ С‚РµР»Рѕ РґРёР°Р»РѕРіР°. */
  content: HTMLElement;
  initialFocus?: HTMLElement;
  /** Р—Р°РєСЂС‹РІР°С‚СЊ Р»Рё РїРѕ РєР»РёРєСѓ РЅР° РїРѕРґР»РѕР¶РєСѓ (РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ вЂ” РґР°). */
  closeOnBackdrop?: boolean;
  /** Р’Р°СЂРёР°РЅС‚ СЂР°Р·РјРµС‰РµРЅРёСЏ: С†РµРЅС‚СЂ РёР»Рё РІРµСЂС… (РїР°Р»РёС‚СЂР°). РџРѕ СѓРјРѕР»С‡Р°РЅРёСЋ вЂ” С†РµРЅС‚СЂ. */
  variant?: DialogVariant;
  /** Р’С‹Р·С‹РІР°РµС‚СЃСЏ РїРѕСЃР»Рµ Р·Р°РєСЂС‹С‚РёСЏ (РІ С‚.С‡. РґР»СЏ РѕС‡РёСЃС‚РєРё СЂРµСЃСѓСЂСЃРѕРІ РІС‹Р·С‹РІР°СЋС‰РµРіРѕ). */
  onClose?: () => void;
}

export interface Dialog {
  /** Р—Р°РєСЂС‹С‚СЊ РґРёР°Р»РѕРі (РёРґРµРјРїРѕС‚РµРЅС‚РЅРѕ). */
  close(): void;
  /** РџР°РЅРµР»СЊ РґРёР°Р»РѕРіР° (`.dialog`). */
  readonly element: HTMLElement;
}

interface DialogRecord {
  overlay: HTMLElement;
  panel: HTMLElement;
  close(): void;
}

const stack: DialogRecord[] = [];
let seq = 0;

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), ' +
  'textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';


const supportsInert = "inert" in HTMLElement.prototype;
let inerted: HTMLElement[] = [];

/** РћС‚РєСЂС‹С‚ Р»Рё С…РѕС‚СЏ Р±С‹ РѕРґРёРЅ РґРёР°Р»РѕРі (РґР»СЏ РіРµР№С‚Р° С…РѕС‚РєРµРµРІ РѕР±РѕР»РѕС‡РєРё). */
export function isModalOpen(): boolean {
  return stack.length > 0;
}

/** Р—Р°РєСЂС‹С‚СЊ РІСЃРµ РґРёР°Р»РѕРіРё (С‚РµСЃС‚С‹/С…СѓРєРё; РІРµСЂС…РЅРёР№ Р·Р°РєСЂС‹РІР°РµС‚СЃСЏ РїРµСЂРІС‹Рј). */
export function closeAllDialogs(): void {
  while (stack.length > 0) stack[stack.length - 1].close();
}

/** Р¤РѕРєСѓСЃРёСЂСѓРµРјС‹Рµ СЌР»РµРјРµРЅС‚С‹ РІРЅСѓС‚СЂРё РґРёР°Р»РѕРіР° (Р±РµР· РЅСѓР»РµРІРѕР№ РіРµРѕРјРµС‚СЂРёРё). */
function focusable(scope: HTMLElement): HTMLElement[] {
  return Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => el.getClientRects().length > 0
  );
}

/** РЎРґРµР»Р°С‚СЊ С„РѕРЅ РёРЅРµСЂС‚РЅС‹Рј С‚РѕР»СЊРєРѕ РґР»СЏ РїРµСЂРІРѕРіРѕ РґРёР°Р»РѕРіР° СЃС‚РµРєР°. */
function applyInert(): void {
  if (!supportsInert || inerted.length > 0) return;
  inerted = Array.from(document.body.children).filter(
    (el): el is HTMLElement =>
      el instanceof HTMLElement && el.id !== "modal-root" && el.id !== "toast-host"
  );
  for (const el of inerted) (el as HTMLElement & { inert: boolean }).inert = true;
}

/** РЎРЅСЏС‚СЊ `inert` c С„РѕРЅР° (РїРѕСЃР»Рµ Р·Р°РєСЂС‹С‚РёСЏ РїРѕСЃР»РµРґРЅРµРіРѕ РґРёР°Р»РѕРіР°). */
function clearInert(): void {
  for (const el of inerted) (el as HTMLElement & { inert: boolean }).inert = false;
  inerted = [];
}

/** РћС‚РєСЂС‹С‚СЊ РјРѕРґР°Р»СЊРЅС‹Р№ РґРёР°Р»РѕРі; РІРѕР·РІСЂР°С‰Р°РµС‚ С„Р°СЃР°Рґ РґР»СЏ Р·Р°РєСЂС‹С‚РёСЏ Рё РїР°РЅРµР»СЊ. */
export function openDialog(opts: DialogOptions): Dialog {
  const root = document.getElementById("modal-root");
  if (!root) throw new Error("#modal-root РЅРµ РЅР°Р№РґРµРЅ");

  const variant = opts.variant ?? "center";
  const closeOnBackdrop = opts.closeOnBackdrop ?? true;
  const previousActive = document.activeElement;

  const overlay = document.createElement("div");
  overlay.className = `dialog-backdrop dialog-backdrop-${variant}`;

  const panel = document.createElement("div");
  panel.className = "dialog";
  panel.tabIndex = -1;
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-modal", "true");

  const titleId = `dialog-title-${++seq}`;
  const title = document.createElement("h2");
  title.className = "dialog-title";
  title.id = titleId;
  title.textContent = opts.title;
  panel.setAttribute("aria-labelledby", titleId);

  const body = document.createElement("div");
  body.className = "dialog-body";
  body.append(opts.content);

  panel.append(title, body);
  overlay.append(panel);

  const ac = new AbortController();
  const { signal } = ac;


  const record: DialogRecord = { overlay, panel, close };

  function close(): void {
    const idx = stack.indexOf(record);
    if (idx === -1) return;
    stack.splice(idx, 1);
    ac.abort();
    overlay.remove();
    if (stack.length === 0) clearInert();
    if (previousActive instanceof HTMLElement && previousActive.isConnected) {
      previousActive.focus({ preventScroll: true });
    }
    opts.onClose?.();
  }


  overlay.addEventListener(
    "click",
    (e) => {
      if (e.target === overlay && closeOnBackdrop) close();
    },
    { signal }
  );


  panel.addEventListener(
    "keydown",
    (e) => {
      if (e.key !== "Tab") return;
      if (stack[stack.length - 1] !== record) return;
      const items = focusable(panel);
      if (items.length === 0) {
        e.preventDefault();
        panel.focus({ preventScroll: true });
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (!first || !last) return;
      const active = document.activeElement;
      if (e.shiftKey) {
        if (active === first || !panel.contains(active)) {
          e.preventDefault();
          last.focus({ preventScroll: true });
        }
      } else if (active === last || !panel.contains(active)) {
        e.preventDefault();
        first.focus({ preventScroll: true });
      }
    },
    { signal }
  );



  window.addEventListener(
    "keydown",
    (e) => {
      if (e.key !== "Escape") return;
      if (stack[stack.length - 1] !== record) return;
      e.preventDefault();
      e.stopPropagation();
      close();
    },
    { capture: true, signal }
  );

  if (stack.length === 0) applyInert();
  stack.push(record);
  root.append(overlay);

  const target = opts.initialFocus ?? focusable(panel)[0] ?? panel;
  target.focus({ preventScroll: true });

  return { close, element: panel };
}
