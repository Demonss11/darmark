// drawer.ts — правый модальный дровер (ADR-0024, вариант C).
//
// Глубокий модуль: маленький интерфейс (open/close/toggle/isOpen/content/
// dispose), много скрытого поведения — фокус-трап, Esc (capture +
// stopPropagation, как в dialog.ts), клик по backdrop, синхронизация
// `aria-expanded`/`.active` на триггере, возврат фокуса на триггер.
//
// Дровер — ПОСТОЯННЫЙ DOM (#drawer + #backdrop — прямые дети body), а не
// транзитный оверлей openDialog: #plugin-manager и #side-count обязаны
// существовать постоянно (состояние раскрытия карточек, счётчик), а
// openDialog создаёт/удаляет DOM на каждый показ.
//
// Из dialog.ts переиспользуем ПРИМИТИВЫ (feature-detect `inert`, логика
// focus-trap), но не openDialog: там `inert` ставится на ВСЕ прямые дети
// body кроме #modal-root/#toast-host — для дровера это неверно (сам #drawer
// стал бы инертным), поэтому глушим только #app.

export interface Drawer {
  open(): void;
  close(): void;
  toggle(): void;
  isOpen(): boolean;
  /** Контейнер содержимого (#plugin-manager); менеджер монтируется сюда один раз. */
  readonly content: HTMLElement;
  dispose(): void;
}

export interface DrawerOptions {
  root: HTMLElement;        // #drawer
  trigger: HTMLElement;     // #tb-plugins
  closeButton: HTMLElement; // #drawer-close
  backdrop: HTMLElement;    // #backdrop
  title: string;            // "Плагины"
}

// Feature-detect: WebView2 может не знать `inert`; тогда фон не изолируем.
const supportsInert = "inert" in HTMLElement.prototype;

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), ' +
  'textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** Фокусируемые элементы внутри контейнера (без нулевой геометрии). */
function focusable(scope: HTMLElement): HTMLElement[] {
  return Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => el.getClientRects().length > 0
  );
}

/** Длительность закрывающей анимации (мс) — скрываем #drawer после неё. */
const CLOSE_MS = 220;

export function createDrawer(opts: DrawerOptions): Drawer {
  const { root, trigger, closeButton, backdrop } = opts;

  const titleEl = root.querySelector<HTMLElement>("#drawer-title");
  if (!titleEl) throw new Error("#drawer должен содержать #drawer-title");
  const content = root.querySelector<HTMLElement>("#plugin-manager");
  if (!content) throw new Error("#drawer должен содержать #plugin-manager");
  if (opts.title) titleEl.textContent = opts.title;

  // aria-labelledby уже в разметке; на всякий случай обеспечиваем связь.
  if (!root.hasAttribute("aria-labelledby")) {
    root.setAttribute("aria-labelledby", "drawer-title");
  }

  // inert только на #app: #drawer/#backdrop/#modal-root/#toast-host —
  // прямые дети body и должны остаться доступными.
  const app = document.getElementById("app");

  let open = false;
  // Таймер закрывающей анимации (скрытие #drawer после transition).
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
    // reflow: transition transform срабатывает только после показа узла.
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
    // Возврат фокуса на триггер — единственная предсказуемая точка (дровер
    // открывают и из тулбара, и из статусбара).
    trigger.focus({ preventScroll: true });
    // Скрываем после анимации закрытия, чтобы transform-переход был виден.
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
      // Вложенный поповер (role="tooltip", напр. инфо плагина) — верхний слой:
      // отдаём Escape ему, дровер не закрываем (pluginManager поглотит событие).
      if (root.querySelector('[role="tooltip"]:not([hidden])')) return;
      e.preventDefault();
      e.stopPropagation();
      hide();
      return;
    }
    if (e.key !== "Tab") return;
    // Focus-trap: Tab/Shift+Tab не покидают дровер.
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

  // Клик по триггеру привязывает shell.ts (ADR-0024): не дублируем.
  closeButton.addEventListener("click", hide);
  backdrop.addEventListener("click", hide);
  // Escape — на window с capture: как в dialog.ts, чтобы событие не дошло
  // до обработчиков shell.ts (там Escape выключает режим инспектора).
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
        // Возврат фокуса: иначе он останется внутри скрытого дровера.
        trigger.focus({ preventScroll: true });
      }
      root.hidden = true;
    },
  };
}
