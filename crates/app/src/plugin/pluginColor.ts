






const PALETTE = [
  "hsl(262 65% 70%)",
  "hsl(217 65% 65%)",
  "hsl(160 65% 55%)",
  "hsl(330 65% 65%)",
  "hsl(25 65% 65%)",
  "hsl(120 65% 55%)",
] as const;

export function pluginColor(id: string): string {
  let hash = 0;
  for (let i = 0; i < id.length; i++) hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  return PALETTE[hash % PALETTE.length];
}
