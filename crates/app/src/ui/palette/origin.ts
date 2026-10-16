

import type { PaletteCommand } from "./types";

export function originLabel(origin: PaletteCommand["origin"]): string {
  return origin.startsWith("plugin:") ? origin.slice("plugin:".length) : "ПЕНАЛ";
}
