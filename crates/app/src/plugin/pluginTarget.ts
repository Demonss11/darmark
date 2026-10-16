





export function ownerPluginId(path: string | null, ids: readonly string[]): string | null {
  if (!path || !/\.lua$/i.test(path)) return null;
  const sep = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (sep <= 0) return null;
  const parent = path.slice(0, sep);
  const parentName = parent.slice(
    Math.max(parent.lastIndexOf("/"), parent.lastIndexOf("\\")) + 1
  );
  const lower = parentName.toLowerCase();
  return ids.find((id) => id.toLowerCase() === lower) ?? null;
}
