// dialog.ts — переиспользуемая модальная инфраструктура (TZ-UX-SPEC-CLEANUP.md, шаг 3).
//
// Один стек диалогов: оверлей + панель с `role="dialog"`, focus-trap, Escape
// (capture + stopPropagation — чтобы Escape не дошёл до обработчиков `shell.ts`
// и не выключил режим инспектора), `inert` на прямых детях body (кроме
// `#modal-root`/`#toast-host`) и возврат фокуса инициатору. Все слушатели
// снимаются AbortController'ом при закрытии — висячих хэндлеров не остаётся.

export type DialogVariant = "center" | "top";

export interface DialogOptions {
  /** Заголовок диалога (рендерится как `<h2>`, задаёт доступное имя). */
  title: string;
  /** Содержимое: узел, вставляемый в тело диалога. */
  content: HTMLElement;
  /** Элемент, получающий фокус при открытии (иначе — первый фокусируемый). */
  initialFocus?: HTMLElement;
  /** Закрывать ли по клику на подложку (по умолчанию — да). */
  closeOnBackdrop?: boolean;
  /** Вариант размещения: центр или верх (палитра). По умолчанию — центр. */
  variant?: DialogVariant;
  /** Вызывается после закрытия (в т.ч. для очистки ресурсов вызывающего). */
  onClose?: () => void;
}

export interface Dialog {
  /** Закрыть диалог (идемпотентно). */
  close(): void;
  /** Панель диалога (`.dialog`). */
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

// Feature-detect: WebView2 может не знать `inert`; тогда просто не изолируем фон.
const supportsInert = "inert" in HTMLElement.prototype;
let inerted: HTMLElement[] = [];

/** Открыт ли хотя бы один диалог (для гейта хоткеев оболочки). */
export function isModalOpen(): boolean {
  return stack.length > 0;
}

/** Закрыть все диалоги (тесты/хуки; верхний закрывается первым). */
export function closeAllDialogs(): void {
  while (stack.length > 0) stack[stack.length - 1].close();
}

/** Фокусируемые элементы внутри диалога (без нулевой геометрии). */
function focusable(scope: HTMLElement): HTMLElement[] {
  return Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => el.getClientRects().length > 0
  );
}

/** Сделать фон инертным только для первого диалога стека. */
function applyInert(): void {
  if (!supportsInert || inerted.length > 0) return;
  inerted = Array.from(document.body.children).filter(
    (el): el is HTMLElement =>
      el instanceof HTMLElement && el.id !== "modal-root" && el.id !== "toast-host"
  );
  for (const el of inerted) (el as HTMLElement & { inert: boolean }).inert = true;
}

/** Снять `inert` c фона (после закрытия последнего диалога). */
function clearInert(): void {
  for (const el of inerted) (el as HTMLElement & { inert: boolean }).inert = false;
  inerted = [];
}

/** Открыть модальный диалог; возвращает фасад для закрытия и панель. */
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

  // Функция объявлена до записи в record (hoisting), вызывается только позже.
  const record: DialogRecord = { overlay, panel, close };

  function close(): void {
    const idx = stack.indexOf(record);
    if (idx === -1) return;
    stack.splice(idx, 1);
    ac.abort();
    overlay.remove();
    if (stack.length === 0) clearInert();
    // Возврат фокуса — только если инициатор ещё в DOM.
    if (previousActive instanceof HTMLElement && previousActive.isConnected) {
      previousActive.focus({ preventScroll: true });
    }
    opts.onClose?.();
  }

  // Клик именно по подложке (не по панели) закрывает диалог.
  overlay.addEventListener(
    "click",
    (e) => {
      if (e.target === overlay && closeOnBackdrop) close();
    },
    { signal }
  );

  // Focus-trap: Tab/Shift+Tab не покидают верхний диалог.
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

  // Escape — только для верхнего диалога; capture + stopPropagation, чтобы
  // событие не достигло `shell.ts` (там Escape выключает режим инспектора).
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
