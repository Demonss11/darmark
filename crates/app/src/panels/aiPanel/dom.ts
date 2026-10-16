

export function byId<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`aiPanel: не найден элемент #${id}`);
  return el as T;
}

export function must<T>(value: T | null, what: string): T {
  if (!value) throw new Error(`aiPanel: не найден ${what}`);
  return value;
}

export function nowTime(): string {
  const d = new Date();
  const p = (n: number): string => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export function shortModel(model: string): string {
  const parts = model.split("/");
  return parts[parts.length - 1] || model;
}

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
}

export function actionButton(label: string, className: string, ariaLabel?: string): HTMLButtonElement {
  const button = el("button", className);
  button.type = "button";
  button.textContent = label;
  if (ariaLabel) button.setAttribute("aria-label", ariaLabel);
  return button;
}
