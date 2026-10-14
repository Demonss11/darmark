// Р‘СЂРµРЅРґРёСЂРѕРІР°РЅРЅС‹Рµ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂС‹ (DESIGN_DOC В§5.1).
// РЎС‚СЂРѕРєРё, Р° РЅРµ С‡РёСЃР»Р°: РµРґРёРЅС‹Р№ С„РѕСЂРјР°С‚ СЃ Rust (`#[serde(transparent)]`) Рё Lua-РїР»Р°РіРёРЅР°РјРё
// (СЃС‚СЂРѕРєРѕРІС‹Рµ РєР»СЋС‡Рё С‚Р°Р±Р»РёС†); РіРµРЅРµСЂР°С†РёСЋ DocumentId РєРѕРЅС‚СЂРѕР»РёСЂСѓРµС‚ С…РѕСЃС‚.

type Brand<T, K extends string> = T & { readonly __b: K };

export type DocumentId = Brand<string, "doc">;
export type PaneId = Brand<string, "pane">;
export type ViewId = Brand<string, "view">;

/** РџСЂРёРІРѕРґРёС‚ СЃС‚СЂРѕРєСѓ РёР· IPC/РєРѕРЅС„РёРіР° Рє `DocumentId` (id РІС‹РґР°С‘С‚ Rust). */
export const asDocumentId = (s: string): DocumentId => s as DocumentId;
export const asPaneId = (s: string): PaneId => s as PaneId;
export const asViewId = (s: string): ViewId => s as ViewId;


let seq = 0;
const next = (): number => ++seq;

export const newPaneId = (): PaneId => asPaneId(`pane-${next()}`);
export const newViewId = (kind: string): ViewId => asViewId(`${kind}-${next()}`);
