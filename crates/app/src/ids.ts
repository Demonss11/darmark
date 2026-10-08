// Брендированные идентификаторы (DESIGN_DOC §5.1).
// Строки, а не числа: единый формат с Rust (`#[serde(transparent)]`) и Lua-плагинами
// (строковые ключи таблиц); генерацию DocumentId контролирует хост.

type Brand<T, K extends string> = T & { readonly __b: K };

export type DocumentId = Brand<string, "doc">;
export type PaneId = Brand<string, "pane">;
export type ViewId = Brand<string, "view">;

/** Приводит строку из IPC/конфига к `DocumentId` (id выдаёт Rust). */
export const asDocumentId = (s: string): DocumentId => s as DocumentId;
export const asPaneId = (s: string): PaneId => s as PaneId;
export const asViewId = (s: string): ViewId => s as ViewId;

// Счётчик — источник уникальных id для панелей/видов, которые создаёт фронтенд.
let seq = 0;
const next = (): number => ++seq;

export const newPaneId = (): PaneId => asPaneId(`pane-${next()}`);
export const newViewId = (kind: string): ViewId => asViewId(`${kind}-${next()}`);
