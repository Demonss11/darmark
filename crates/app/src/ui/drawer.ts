// drawer.ts вЂ” РїСЂР°РІС‹Р№ РјРѕРґР°Р»СЊРЅС‹Р№ РґСЂРѕРІРµСЂ (ADR-0024, РІР°СЂРёР°РЅС‚ C).
//
// РњР°Р»РµРЅСЊРєРёР№ РёРЅС‚РµСЂС„РµР№СЃ (open/close/toggle/isOpen/content/dispose) Рё РјРЅРѕРіРѕ
// СЃРєСЂС‹С‚РѕРіРѕ РїРѕРІРµРґРµРЅРёСЏ: С„РѕРєСѓСЃ-С‚СЂР°Рї, Esc (capture + stopPropagation, РєР°Рє РІ
// dialog.ts), РєР»РёРє РїРѕ backdrop, СЃРёРЅС…СЂРѕРЅРёР·Р°С†РёСЏ `aria-expanded`/`.active` РЅР°
// С‚СЂРёРіРіРµСЂРµ, РІРѕР·РІСЂР°С‚ С„РѕРєСѓСЃР° РЅР° С‚СЂРёРіРіРµСЂ.
//
// Р”СЂРѕРІРµСЂ вЂ” РџРћРЎРўРћРЇРќРќР«Р™ DOM (#drawer + #backdrop вЂ” РїСЂСЏРјС‹Рµ РґРµС‚Рё body), Р° РЅРµ
// С‚СЂР°РЅР·РёС‚РЅС‹Р№ РѕРІРµСЂР»РµР№ openDialog: #plugin-manager Рё #side-count РѕР±СЏР·Р°РЅС‹
// СЃСѓС‰РµСЃС‚РІРѕРІР°С‚СЊ РїРѕСЃС‚РѕСЏРЅРЅРѕ (СЃРѕСЃС‚РѕСЏРЅРёРµ СЂР°СЃРєСЂС‹С‚РёСЏ РєР°СЂС‚РѕС‡РµРє, СЃС‡С‘С‚С‡РёРє).
//
// РџСЂРёРјРёС‚РёРІС‹ РёР· dialog.ts (`inert`, focus-trap) РїРµСЂРµРёСЃРїРѕР»СЊР·СѓСЋС‚СЃСЏ, РЅРѕ РЅРµ
// openDialog: С‚Р°Рј `inert` СЃС‚Р°РІРёС‚СЃСЏ РЅР° Р’РЎР• РїСЂСЏРјС‹Рµ РґРµС‚Рё body РєСЂРѕРјРµ
// #modal-root/#toast-host вЂ” РґР»СЏ РґСЂРѕРІРµСЂР° СЌС‚Рѕ РЅРµРІРµСЂРЅРѕ (СЃР°Рј #drawer СЃС‚Р°Р» Р±С‹
// РёРЅРµСЂС‚РЅС‹Рј), РїРѕСЌС‚РѕРјСѓ РіР»СѓС€РёРј С‚РѕР»СЊРєРѕ #app.

export interface Drawer {
  open(): void;
  close(): void;
  toggle(): void;
  isOpen(): boolean;
  /** РљРѕРЅС‚РµР№РЅРµСЂ СЃРѕРґРµСЂР¶РёРјРѕРіРѕ (#plugin-manager); РјРµРЅРµРґР¶РµСЂ РјРѕРЅС‚РёСЂСѓРµС‚СЃСЏ СЃСЋРґР° РѕРґРёРЅ СЂР°Р·. */
  readonly content: HTMLElement;
  dispose(): void;
}

export interface DrawerOptions {
  root: HTMLElement;        // #drawer
  trigger: HTMLElement;     // #tb-plugins
  closeButton: HTMLElement; // #drawer-close
  backdrop: HTMLElement;    // #backdrop
  title: string;            // "РџР»Р°РіРёРЅС‹"
}


const supportsInert = "inert" in HTMLElement.prototype;

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), ' +
  'textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** Р¤РѕРєСѓСЃРёСЂСѓРµРјС‹Рµ СЌР»РµРјРµРЅС‚С‹ РІРЅСѓС‚СЂРё РєРѕРЅС‚РµР№РЅРµСЂР° (Р±РµР· РЅСѓР»РµРІРѕР№ РіРµРѕРјРµС‚СЂРёРё). */
function focusable(scope: HTMLElement): HTMLElement[] {
  return Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => el.getClientRects().length > 0
  );
}

/** Р”Р»РёС‚РµР»СЊРЅРѕСЃС‚СЊ Р·Р°РєСЂС‹РІР°СЋС‰РµР№ Р°РЅРёРјР°С†РёРё (РјСЃ) вЂ” СЃРєСЂС‹РІР°РµРј #drawer РїРѕСЃР»Рµ РЅРµС‘. */
const CLOSE_MS = 220;

export function createDrawer(opts: DrawerOptions): Drawer {
  const { root, trigger, closeButton, backdrop } = opts;

  const titleEl = root.querySelector<HTMLElement>("#drawer-title");
  if (!titleEl) throw new Error("#drawer РґРѕР»Р¶РµРЅ СЃРѕРґРµСЂР¶Р°С‚СЊ #drawer-title");
  const content = root.querySelector<HTMLElement>("#plugin-manager");
  if (!content) throw new Error("#drawer РґРѕР»Р¶РµРЅ СЃРѕРґРµСЂР¶Р°С‚СЊ #plugin-manager");
  if (opts.title) titleEl.textContent = opts.title;


  if (!root.hasAttribute("aria-labelledby")) {
    root.setAttribute("aria-labelledby", "drawer-title");
  }



  const app = document.getElementById("app");

  let open = false;

  let closeTimer: number | null = null;

  function setInert(on: boolean): void {
    if (!supportsInert || !app) return;
    (app as HTMLElement & { inert: boolean }).inert = on;
  }

  function show(): void {
    if (open) return;
    if (closeTimer !== null) {
      window.clearTimeout(closeTimer);
      closeTimer = null;
    }
    open = true;
    root.hidden = false;
    backdrop.classList.add("open");

    void root.offsetWidth;
    root.classList.add("open");
    trigger.classList.add("active");
    trigger.setAttribute("aria-expanded", "true");
    setInert(true);
    closeButton.focus({ preventScroll: true });
  }

  function hide(): void {
    if (!open) return;
    open = false;
    root.classList.remove("open");
    backdrop.classList.remove("open");
    trigger.classList.remove("active");
    trigger.setAttribute("aria-expanded", "false");
    setInert(false);


    trigger.focus({ preventScroll: true });

    if (closeTimer !== null) window.clearTimeout(closeTimer);
    closeTimer = window.setTimeout(() => {
      closeTimer = null;
      if (!open) root.hidden = true;
    }, CLOSE_MS);
  }

  function toggle(): void {
    if (open) hide();
    else show();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!open) return;
    if (e.key === "Escape") {


      if (root.querySelector('[role="tooltip"]:not([hidden])')) return;
      e.preventDefault();
      e.stopPropagation();
      hide();
      return;
    }
    if (e.key !== "Tab") return;

    const items = focusable(root);
    if (items.length === 0) {
      e.preventDefault();
      closeButton.focus({ preventScroll: true });
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (e.shiftKey) {
      if (active === first || !root.contains(active)) {
        e.preventDefault();
        last.focus({ preventScroll: true });
      }
    } else if (active === last || !root.contains(active)) {
      e.preventDefault();
      first.focus({ preventScroll: true });
    }
  }


  closeButton.addEventListener("click", hide);
  backdrop.addEventListener("click", hide);


  window.addEventListener("keydown", onKeydown, { capture: true });

  return {
    open: show,
    close: hide,
    toggle,
    isOpen: () => open,
    content,
    dispose(): void {
      if (closeTimer !== null) {
        window.clearTimeout(closeTimer);
        closeTimer = null;
      }
      window.removeEventListener("keydown", onKeydown, { capture: true });
      closeButton.removeEventListener("click", hide);
      backdrop.removeEventListener("click", hide);
      if (open) {
        setInert(false);
        root.classList.remove("open");
        backdrop.classList.remove("open");
        trigger.classList.remove("active");
        trigger.setAttribute("aria-expanded", "false");

        trigger.focus({ preventScroll: true });
      }
      root.hidden = true;
    },
  };
}
