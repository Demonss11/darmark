// pluginColor.ts — детерминированный цвет плагина (`--pc`) из его id.
//
// Единый источник идентичности плагина: палитра команд (palette.ts) и
// статусбар плагинов (pluginStatusBar.ts) берут цвет отсюда, чтобы один и тот
// же плагин везде выглядел одинаково (UX-spec §5).

/** Стабильный цвет плагина: детерминированный HSL-хью из `plugin_id`. */
export function pluginColor(id: string): string {
  let hue = 0;
  for (let i = 0; i < id.length; i++) hue = (hue * 31 + id.charCodeAt(i)) % 360;
  return `hsl(${hue} 65% 62%)`;
}
