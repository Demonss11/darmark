// DOM-хелперы менеджера плагинов.

export function el(tag: string, className?: string): HTMLElement {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
}

export function infoPopoverId(pluginId: string): string {
  return `pl-info-pop-${pluginId}`;
}

export function bodyId(pluginId: string): string {
  return `pl-body-${pluginId}`;
}
