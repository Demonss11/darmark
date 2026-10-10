// toast.ts — тост-уведомления в `#toast-host`.
//
// Неинтерактивные (хост `pointer-events: none`); авто-скрытие; не более трёх
// одновременно (самый старый убирается). Вход/выход — только `opacity`/
// `transform`, их гасит глобальный `prefers-reduced-motion`. Live-region —
// сам хост (`aria-live="polite"`); ошибки дополнительно несут `role="alert"`,
// чтобы озвучиваться сразу (assertive).

export type ToastKind = "info" | "success" | "warning" | "error";

export interface ToastOptions {
  kind?: ToastKind;
  /** Длительность показа в мс; `<= 0` — «залипающий» тост без авто-скрытия. */
  duration?: number;
}

const MAX_TOASTS = 3;
const EXIT_MS = 200;

/** Убрать тост с обратной анимацией (идемпотентно). */
function dismiss(node: HTMLElement): void {
  if (node.dataset.leaving) return;
  node.dataset.leaving = "1";
  node.classList.remove("toast-show");
  window.setTimeout(() => node.remove(), EXIT_MS);
}

/** Показать тост; `duration <= 0` — тост не исчезает сам. */
export function toast(message: string, opts: ToastOptions = {}): void {
  const host = document.getElementById("toast-host");
  if (!host) return;

  const kind: ToastKind = opts.kind ?? "info";
  const node = document.createElement("div");
  node.className = `toast toast-${kind}`;
  // Ошибки — assertive; остальные озвучивает сам `#toast-host` (aria-live="polite"),
  // поэтому свою live-роль не дублируем.
  if (kind === "error") node.setAttribute("role", "alert");

  host.append(node);
  // Текст ставим ПОСЛЕ вставки в live-region — иначе часть скринридеров не
  // объявит готовый узел. Принудительный reflow: без него transition не стартует.
  node.textContent = message;
  void node.offsetWidth;
  node.classList.add("toast-show");

  // Максимум три: лишние старые убираем сразу.
  const all = Array.from(host.querySelectorAll<HTMLElement>(".toast"));
  for (let i = 0; i < all.length - MAX_TOASTS; i++) all[i]?.remove();

  const fallback = kind === "info" || kind === "success" ? 3000 : 5000;
  const duration = opts.duration ?? fallback;
  if (duration > 0) {
    window.setTimeout(() => dismiss(node), duration);
  } else {
    // Залипающий тост: даём закрыть кликом (иначе «мёртвый» клик-барьер).
    node.classList.add("toast-sticky");
    node.addEventListener("click", () => dismiss(node));
  }
}
