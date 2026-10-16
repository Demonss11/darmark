







type DialogVariant = "center" | "top";

interface DialogOptions {
  title: string;
  content: HTMLElement;
  initialFocus?: HTMLElement;
  closeOnBackdrop?: boolean;
  variant?: DialogVariant;
  onClose?: () => void;
}

export interface Dialog {
  close(): void;
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

export function isModalOpen(): boolean {
  return stack.length > 0;
}


function focusable(scope: HTMLElement): HTMLElement[] {
  return Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => el.getClientRects().length > 0
  );
}

function applyInert(): void {
  if (!supportsInert || inerted.length > 0) return;
  inerted = Array.from(document.body.children).filter(
    (el): el is HTMLElement =>
      el instanceof HTMLElement && el.id !== "modal-root" && el.id !== "toast-host"
  );
  for (const el of inerted) (el as HTMLElement & { inert: boolean }).inert = true;
}

function clearInert(): void {
  for (const el of inerted) (el as HTMLElement & { inert: boolean }).inert = false;
  inerted = [];
}

export function openDialog(opts: DialogOptions): Dialog {
  const root = document.getElementById("modal-root");
  if (!root) throw new Error("#modal-root не найден");

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
