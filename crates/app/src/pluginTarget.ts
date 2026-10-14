// pluginTarget.ts — определяет плагин-владельца открытого `.lua`-документа.
//
// Раскладка плагинов — `<plugins>/<id>/<entry>.lua`, поэтому родительский
// каталог файла и есть id. `Ctrl+R` перезагружает только владельца, иначе —
// все включённые плагины (fallback; reload не сбрасывает карантин/согласие).

/**
 * Возвращает id включённого плагина, которому принадлежит `.lua`-документ, или
 * `null` — если путь не `.lua`, пуст или его каталог не совпал ни с одним id.
 */
export function ownerPluginId(path: string | null, ids: readonly string[]): string | null {
  if (!path || !/\.lua$/i.test(path)) return null;
  const sep = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (sep <= 0) return null;
  const parent = path.slice(0, sep);
  const parentName = parent.slice(
    Math.max(parent.lastIndexOf("/"), parent.lastIndexOf("\\")) + 1
  );
  // Windows-ФС регистронезависима, id манифеста — только [a-z0-9_-]; сравниваем
  // без регистра и возвращаем канонический id (иначе сработал бы fallback).
  const lower = parentName.toLowerCase();
  return ids.find((id) => id.toLowerCase() === lower) ?? null;
}
