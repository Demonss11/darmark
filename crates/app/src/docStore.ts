// docStore — TS-проекция Rust-стора (D5): текст, ревизия, путь и dirty.
//
// Модуль без DOM: ввод приходит из editorView (`setText`), HTML уходит в
// `onRender`. Рендер и его кэш принадлежат Rust; здесь — дебаунс IPC,
// отбрасывание устаревших ответов и подписки. Никаких обращений к document/window.

import {
  newDocument as ipcNew,
  openDocument as ipcOpen,
  updateDocument as ipcUpdate,
  renderDocument as ipcRender,
  undoDocument as ipcUndo,
  redoDocument as ipcRedo,
  saveDocument as ipcSave,
  closeDocument as ipcClose,
  documentSnapshot,
  errorMessage,
} from "./tauri";
import type { DocumentId } from "./ids";
import type { DocumentSnapshot, RenderResult } from "./tauri";

/** Проекция документа: зеркало Rust-стора + буфер редактора и dirty. */
export interface Tab {
  id: DocumentId;
  path: string | null;
  rev: number;
  /** Актуальный буфер редактора (обновляется синхронно вводу, до IPC). */
  text: string;
  /** Текст на момент последнего сохранения/открытия — база для вывода `dirty`. */
  savedText: string;
  /** Производное от `text !== savedText`: незаписанные в файл изменения (§4.2). */
  dirty: boolean;
  name: string;
}

export interface DocStoreOptions {
  /** Вызывается в момент рендера, а не при подписке. */
  renderMapped(): boolean;
  /** Единственная точка применения HTML (previewView.applyRender). */
  onRender(res: RenderResult, source: string): void;
  onStatus(msg: string): void;
}

export interface DocStore {
  tabs(): Tab[];
  activeId(): DocumentId | null;
  active(): Tab | null;
  /** Подписка на изменение списка/активной вкладки; возвращает unsubscribe (§5.6). */
  subscribe(cb: (tabs: Tab[], activeId: DocumentId | null) => void): () => void;

  snapshot(): DocumentSnapshot | null;
  onDocument(cb: (d: DocumentSnapshot) => void): () => void;
  /**
   * Рендер переданного текста без применения результата (ViewContext.render):
   * вызывающий сам решает, куда деть HTML. Не путать с конвейером `setText`.
   */
  renderText(text: string, mapped: boolean): Promise<RenderResult>;

  /** Правка из редактора: сразу в активную вкладку + dirty + дебаунс IPC-рендера. */
  setText(text: string): void;
  /** Отменить дебаунс и немедленно отправить текст активной вкладки в Rust. */
  flush(): Promise<void>;
  /**
   * Отменить последнюю правку (Rust-undo, D5/ADR-0028). Возвращает байтовый
   * (UTF-8) диапазон результата для выделения либо `null`, если отменять нечего.
   */
  undo(): Promise<{ start: number; end: number } | null>;
  /** Повторить отменённую правку; `null`, если повторять нечего. */
  redo(): Promise<{ start: number; end: number } | null>;
  reload(): void;
  /**
   * Внешняя правка документа (событие `document-updated`): pull-снапшот из Rust
   * и применение к соответствующей вкладке. Устаревшие/чужие события игнорируются.
   */
  applyHostUpdate(hostDocId: string, rev: number): void;

  /** Новая вкладка, не закрывая существующие (`open` — то же из файла). */
  newDocument(text?: string): Promise<void>;
  open(path: string): Promise<void>;
  save(path?: string): Promise<void>;
  switchTab(id: DocumentId): void;
  /** Закрыть вкладку + переключиться на соседнюю. */
  closeTab(id: DocumentId): Promise<void>;
}

/** Дебаунс IPC-рендера: не спамить `update_document` на каждое нажатие. */
const DEBOUNCE_MS = 120;

/** Код IPC-ошибки из `CommandError { code, message }` (или `undefined`). */
function errorCode(e: unknown): string | undefined {
  if (e && typeof e === "object" && "code" in e) {
    const code = (e as { code?: unknown }).code;
    if (typeof code === "string") return code;
  }
  return undefined;
}

function baseName(p: string): string {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i >= 0 ? p.slice(i + 1) : p;
}

function makeTab(snap: DocumentSnapshot): Tab {
  return {
    id: snap.id,
    path: snap.path,
    rev: snap.rev,
    text: snap.text,
    savedText: snap.text,
    dirty: false,
    name: snap.path ? baseName(snap.path) : "безымянный",
  };
}

export function createDocStore(opts: DocStoreOptions): DocStore {
  let tabs: Tab[] = [];
  let activeId: DocumentId | null = null;
  // Владелец кэша и ревизии — Rust; `lastRev` лишь отбрасывает устаревшие ответы IPC.
  let lastRev = -1;
  let debounceTimer = 0;
  const subs = new Set<(tabs: Tab[], activeId: DocumentId | null) => void>();

  function notify(): void {
    for (const cb of subs) cb(tabs, activeId);
  }

  function schedule(): void {
    clearTimeout(debounceTimer);
    debounceTimer = window.setTimeout(() => void renderCurrent(), DEBOUNCE_MS);
  }

  function applyRes(res: RenderResult, source: string): void {
    if (res.rev < lastRev) return; // устаревший ответ
    lastRev = res.rev;
    if (activeId) {
      tabs = tabs.map((t) => (t.id === activeId ? { ...t, rev: res.rev } : t));
    }
    opts.onRender(res, source);
    notify();
  }

  /**
   * Отправляет текст активной вкладки в Rust. Возвращает именно отправленный
   * текст: `save` берёт его за базу `savedText`, а не буфер, который мог
   * измениться, пока шла запись.
   */
  async function pushText(): Promise<string | null> {
    const id = activeId;
    if (!id) return null;
    const tab = tabs.find((t) => t.id === id);
    if (!tab) return null;
    const source = tab.text;
    const res = await ipcUpdate(id, source, opts.renderMapped());
    if (id !== activeId) return source;
    applyRes(res, source);
    return source;
  }

  async function renderCurrent(): Promise<void> {
    try {
      await pushText();
    } catch (e) {
      lastRev = -1; // сообщение об ошибке должно пережить следующий changed:false
      opts.onStatus(`Ошибка рендера: ${errorMessage(e)}`);
    }
  }

  function applySnapshot(snap: DocumentSnapshot): void {
    clearTimeout(debounceTimer);
    lastRev = -1;
    const tab = makeTab(snap);
    tabs = [...tabs, tab];
    activeId = snap.id;
    notify();
  }

  async function newDocument(text = ""): Promise<void> {
    applySnapshot(await ipcNew(text));
    reload();
  }

  function reload(): void {
    const id = activeId;
    if (!id) return;
    const tab = tabs.find((t) => t.id === id);
    if (!tab) return;
    clearTimeout(debounceTimer);
    void (async () => {
      try {
        const res = await ipcRender(id, opts.renderMapped());
        if (id !== activeId) return;
        applyRes(res, tab.text);
      } catch (e) {
        if (id !== activeId) return;
        lastRev = -1;
        opts.onStatus(`Ошибка рендера: ${errorMessage(e)}`);
      }
    })();
  }

  /**
   * Общий путь undo/redo (D5/ADR-0028): сначала слить отложенный ввод, чтобы Rust
   * отменял с учётом текущего буфера, а не устаревшего, затем применить историю.
   */
  async function history(
    direction: "undo" | "redo"
  ): Promise<{ start: number; end: number } | null> {
    const id = activeId;
    if (!id) return null;
    const tab = tabs.find((t) => t.id === id);
    if (!tab) return null;

    clearTimeout(debounceTimer);
    try {
      await pushText();
    } catch (e) {
      lastRev = -1;
      opts.onStatus(`Ошибка рендера: ${errorMessage(e)}`);
      return null;
    }
    if (id !== activeId) return null;

    try {
      const res = direction === "undo" ? await ipcUndo(id) : await ipcRedo(id);
      if (id !== activeId) return null;
      if (res.rev < lastRev) return null; // гонка двух отмен подряд
      // Держим `lastRev` на новой ревизии: устаревший ответ IPC не откатит текст.
      lastRev = res.rev;
      // dirty выводим из сравнения с сохранённым текстом: отмена до последнего
      // сохранения должна вернуть вкладку в чистое состояние (§4.2).
      tabs = tabs.map((t) =>
        t.id === id
          ? { ...t, text: res.text, rev: res.rev, dirty: res.text !== t.savedText }
          : t
      );
      notify();
      reload();
      return { start: res.start, end: res.end };
    } catch (e) {
      // «Нечего отменять/повторять» — нормальный сценарий: тихо, без тоста.
      const code = errorCode(e);
      if (code === "nothing_to_undo" || code === "nothing_to_redo") return null;
      opts.onStatus(`Отмена/повтор: ${errorMessage(e)}`);
      return null;
    }
  }

  function toSnapshot(tab: Tab | null): DocumentSnapshot | null {
    if (!tab) return null;
    return { id: tab.id, path: tab.path, rev: tab.rev, text: tab.text, dirty_hint: tab.dirty };
  }

  return {
    tabs: () => tabs,
    activeId: () => activeId,
    active: () => tabs.find((t) => t.id === activeId) ?? null,

    subscribe(cb) {
      subs.add(cb);
      return () => subs.delete(cb);
    },

    snapshot: () => toSnapshot(tabs.find((t) => t.id === activeId) ?? null),

    onDocument(cb) {
      // ViewContext-подписка: только созданный документ.
      const inner = (list: Tab[], id: DocumentId | null): void => {
        const tab = list.find((t) => t.id === id);
        if (tab) cb(toSnapshot(tab)!);
      };
      subs.add(inner);
      return () => subs.delete(inner);
    },

    async renderText(text: string, mapped: boolean): Promise<RenderResult> {
      const id = activeId;
      if (!id) throw new Error("Документ не создан — рендер недоступно");
      const tab = tabs.find((t) => t.id === id);
      if (!tab) throw new Error("Документ не создан — рендер недоступно");
      // Тот же текст, что в сторе → render_document (текст не меняется).
      // Иначе update_document: стор применит правку — осознанный путь edit/render.
      if (text === tab.text) return ipcRender(id, mapped);
      return ipcUpdate(id, text, mapped);
    },

    setText(text: string): void {
      const id = activeId;
      if (!id) return;
      const tab = tabs.find((t) => t.id === id);
      if (!tab || text === tab.text) return;
      tabs = tabs.map((t) =>
        t.id === id ? { ...t, text, dirty: text !== t.savedText } : t
      );
      notify();
      schedule();
    },

    async flush(): Promise<void> {
      clearTimeout(debounceTimer);
      await pushText();
    },

    undo: () => history("undo"),
    redo: () => history("redo"),

    reload,

    applyHostUpdate(hostDocId: string, rev: number): void {
      const tab = tabs.find((t) => t.id === hostDocId);
      if (!tab) return; // чужой/неизвестный документ
      if (rev <= tab.rev) return; // устаревшее событие или эхо собственной правки
      // Внешняя (плагинная) правка перекрывает отложенный push: команду вызвал
      // пользователь, её результат авторитетнее неотправленного локального ввода.
      // Дебаунс отменяем только у активной вкладки: у фоновой он относится к
      // собственному вводу и должен отработать как обычно.
      const isActive = tab.id === activeId;
      if (isActive) clearTimeout(debounceTimer);
      void (async () => {
        try {
          const snap = await documentSnapshot(tab.id);
          if (!snap || snap.rev < rev) return;
          // `lastRev` держим на новой ревизии: устаревший ответ IPC не должен
          // откатить `st` и перерисовать DOM старым текстом (сброс в -1 эту
          // защиту инвертировал бы).
          lastRev = snap.rev;
          // dirty выводим из сравнения: плагин мог вернуть текст, совпадающий
          // с сохранённым, — тогда вкладка снова чистая.
          tabs = tabs.map((t) =>
            t.id === tab.id
              ? { ...t, rev: snap.rev, text: snap.text, dirty: snap.text !== t.savedText }
              : t
          );
          notify();
          if (isActive) {
            reload(); // textarea подтянет editorView
          }
        } catch (e) {
          opts.onStatus(`Внешняя правка: ${errorMessage(e)}`);
        }
      })();
    },

    newDocument,

    async open(path: string): Promise<void> {
      applySnapshot(await ipcOpen(path));
      reload();
    },

    async save(path?: string): Promise<void> {
      const id = activeId;
      if (!id) throw new Error("Документ не создан — сохранение недоступно");
      // Ошибку пробрасываем: точку отсчёта dirty даёт pushText, а не буфер,
      // который мог измениться за время записи.
      const savedText = await pushText();
      const tab = tabs.find((t) => t.id === id);
      if (!tab) throw new Error("Документ не создан — сохранение недоступно");
      const meta = await ipcSave(id, path);
      if (id !== activeId) return;
      const base = savedText ?? tab.text;
      tabs = tabs.map((t) =>
        t.id === id
          ? {
              ...t,
              path: meta.path,
              savedText: base,
              dirty: t.text !== base,
              name: meta.path ? baseName(meta.path) : t.name,
            }
          : t
      );
      notify();
    },

    switchTab(id: DocumentId): void {
      if (!tabs.some((t) => t.id === id)) return;
      if (activeId === id) return;
      clearTimeout(debounceTimer);
      lastRev = -1;
      activeId = id;
      notify();
      reload();
    },

    async closeTab(id: DocumentId): Promise<void> {
      const idx = tabs.findIndex((t) => t.id === id);
      if (idx === -1) return;
      const wasActive = activeId === id;
      tabs = tabs.filter((t) => t.id !== id);
      if (wasActive) {
        // Соседняя: предыдущая, если есть, иначе следующая.
        const next = tabs[Math.min(idx, tabs.length - 1)] ?? null;
        activeId = next?.id ?? null;
        lastRev = -1;
        clearTimeout(debounceTimer);
      }
      void ipcClose(id).catch((e) => opts.onStatus(`Ошибка закрытия: ${errorMessage(e)}`));
      if (tabs.length === 0) {
        // Открываем пустую вкладку, чтобы редактор не оставался пустым.
        await newDocument();
        return;
      }
      notify();
      if (wasActive && activeId) reload();
    },
  };
}
