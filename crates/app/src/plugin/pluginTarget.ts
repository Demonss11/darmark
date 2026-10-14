// pluginTarget.ts вЂ” РѕРїСЂРµРґРµР»СЏРµС‚ РїР»Р°РіРёРЅ-РІР»Р°РґРµР»СЊС†Р° РѕС‚РєСЂС‹С‚РѕРіРѕ `.lua`-РґРѕРєСѓРјРµРЅС‚Р°.
//
// Р Р°СЃРєР»Р°РґРєР° РїР»Р°РіРёРЅРѕРІ вЂ” `<plugins>/<id>/<entry>.lua`, РїРѕСЌС‚РѕРјСѓ СЂРѕРґРёС‚РµР»СЊСЃРєРёР№
// РєР°С‚Р°Р»РѕРі С„Р°Р№Р»Р° Рё РµСЃС‚СЊ id. `Ctrl+R` РїРµСЂРµР·Р°РіСЂСѓР¶Р°РµС‚ С‚РѕР»СЊРєРѕ РІР»Р°РґРµР»СЊС†Р°, РёРЅР°С‡Рµ вЂ”
// РІСЃРµ РІРєР»СЋС‡С‘РЅРЅС‹Рµ РїР»Р°РіРёРЅС‹ (fallback; reload РЅРµ СЃР±СЂР°СЃС‹РІР°РµС‚ РєР°СЂР°РЅС‚РёРЅ/СЃРѕРіР»Р°СЃРёРµ).

/**
 * Р’РѕР·РІСЂР°С‰Р°РµС‚ id РІРєР»СЋС‡С‘РЅРЅРѕРіРѕ РїР»Р°РіРёРЅР°, РєРѕС‚РѕСЂРѕРјСѓ РїСЂРёРЅР°РґР»РµР¶РёС‚ `.lua`-РґРѕРєСѓРјРµРЅС‚, РёР»Рё
 * `null` вЂ” РµСЃР»Рё РїСѓС‚СЊ РЅРµ `.lua`, РїСѓСЃС‚ РёР»Рё РµРіРѕ РєР°С‚Р°Р»РѕРі РЅРµ СЃРѕРІРїР°Р» РЅРё СЃ РѕРґРЅРёРј id.
 */
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
