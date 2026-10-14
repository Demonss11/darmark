// gutter.ts вЂ” РЅРѕРјРµСЂР° СЃС‚СЂРѕРє СЂРµРґР°РєС‚РѕСЂР°.
//
// `#editor` РѕСЃС‚Р°С‘С‚СЃСЏ СЃРєСЂРѕР»Р»-СЌР»РµРјРµРЅС‚РѕРј; gutter Р»РёС€СЊ РїРµСЂРµСЂРёСЃРѕРІС‹РІР°РµС‚ РЅРѕРјРµСЂР° Рё
// РїРѕРґС‚СЏРіРёРІР°РµС‚ СЃРІРѕР№ scrollTop Р·Р° СЂРµРґР°РєС‚РѕСЂРѕРј. Р“РµРѕРјРµС‚СЂРёСЏ (font-size/line-height/
// padding) Р·Р°РґР°РЅР° РѕР±С‰РёРјРё CSS-РїРµСЂРµРјРµРЅРЅС‹РјРё, РёРЅР°С‡Рµ РЅРѕРјРµСЂР° В«РїРѕРµРґСѓС‚В».

export interface Gutter {
  update(): void;
  /** РЎРёРЅС…СЂРѕРЅРёР·РёСЂРѕРІР°С‚СЊ РїСЂРѕРєСЂСѓС‚РєСѓ gutter СЃ СЂРµРґР°РєС‚РѕСЂРѕРј. */
  syncScroll(): void;
  dispose(): void;
}

export function createGutter(editor: HTMLTextAreaElement, host: HTMLElement): Gutter {
  function update(): void {
    const lines = editor.value.split("\n").length;
    let out = "1";
    for (let i = 2; i <= lines; i++) out += "\n" + i;
    host.textContent = out;
    syncScroll(); // textContent СЃР±СЂР°СЃС‹РІР°РµС‚ scrollTop вЂ” РІРѕР·РІСЂР°С‰Р°РµРј РїРѕР·РёС†РёСЋ
  }

  function syncScroll(): void {
    host.scrollTop = editor.scrollTop;
  }

  editor.addEventListener("scroll", syncScroll);
  update();

  return {
    update,
    syncScroll,
    dispose(): void {
      editor.removeEventListener("scroll", syncScroll);
    },
  };
}
