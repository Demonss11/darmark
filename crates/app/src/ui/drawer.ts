// drawer.ts — правый модальный дровер.
//
// Маленький интерфейс (open/close/toggle/isOpen/content/dispose) и много
// скрытого поведения: фокус-трап, Esc (capture + stopPropagation, как в
// dialog.ts), клик по backdrop, синхронизация `aria-expanded`/`.active` на
// триггере, возврат фокуса на триггер.
//
// Дровер — ПОСТОЯННЫЙ DOM (#drawer + #backdrop — прямые дети body), а не
// транзитный оверлей openDialog: #plugin-manager и #side-count обязаны
// существовать постоянно (состояние раскрытия карточек, счётчик).
//
// Примитивы из dialog.ts (`inert`, focus-trap) переиспользуются, но не
// openDialog: там `inert` ставится на ВСЕ прямые дети body кроме
// #modal-root/#toast-host — для дровера это неверно (сам #drawer стал бы
// инертным), поэтому глушим только #app.

export interface Drawer {
  open(): void;
  close(): void;
  toggle(): void;
  isOpen(): boolean;
  readonly content: HTMLElement;
  dispose(): void;
}

interface DrawerOptions {
  root: HTMLElement;
  trigger: HTMLElement;
  closeButton: HTMLElement;
  backdrop: HTMLElement;
  title: string;
}

const supportsInert = "inert" in HTMLElement.prototype;

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), ' +
  'textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusable(scope: HTMLElement): HTMLElement[] {
  return Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => el.getClientRects().length > 0
  );
}

const CLOSE_MS = 220;

export function createDrawer(opts: DrawerOptions): Drawer {
  const { root, trigger, closeButton, backdrop } = opts;

  const titleEl = root.querySelector<HTMLElement>("#drawer-title");
  if (!titleEl) throw new Error("#drawer должен содержать #drawer-title");
  const content = root.querySelector<HTMLElement>("#plugin-manager");
  if (!content) throw new Error("#drawer должен содержать #plugin-manager");
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
