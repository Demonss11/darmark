







type ToastKind = "info" | "success" | "warning" | "error";

interface ToastOptions {
  kind?: ToastKind;
  duration?: number;
}

const MAX_TOASTS = 3;
const EXIT_MS = 200;

function dismiss(node: HTMLElement): void {
  if (node.dataset.leaving) return;
  node.dataset.leaving = "1";
  node.classList.remove("toast-show");
  window.setTimeout(() => node.remove(), EXIT_MS);
}

export function toast(message: string, opts: ToastOptions = {}): void {
  const host = document.getElementById("toast-host");
  if (!host) return;

  const kind: ToastKind = opts.kind ?? "info";
  const node = document.createElement("div");
  node.className = `toast toast-${kind}`;
  if (kind === "error") node.setAttribute("role", "alert");

  host.append(node);
  node.textContent = message;
  void node.offsetWidth;
  node.classList.add("toast-show");

  const all = Array.from(host.querySelectorAll<HTMLElement>(".toast"));
  for (let i = 0; i < all.length - MAX_TOASTS; i++) all[i]?.remove();

  const fallback = kind === "info" || kind === "success" ? 3000 : 5000;
  const duration = opts.duration ?? fallback;
  if (duration > 0) {
    window.setTimeout(() => dismiss(node), duration);
  } else {
    node.classList.add("toast-sticky");
    node.addEventListener("click", () => dismiss(node));
  }
}
