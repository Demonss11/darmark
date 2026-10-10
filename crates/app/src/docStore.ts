// docStore — TS-проекция Rust-стора (D5): текст, ревизия, путь и dirty.
//
// Чистый модуль без DOM: ввод приходит из editorView (`setText`), HTML уходит
// в `onRender` (в Фазе 4 — в previewView.applyRender). Рендер и его кэш
// принадлежат Rust; здесь — дебаунс IPC, защита от гонок ответов и подписки.
// Никаких обращений к `document`/`window`.

import {
  newDocument as ipcNew,
  openDocument as ipcOpen,
  updateDocument as ipcUpdate,
  renderDocument as ipcRender,
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
  dirty: boolean;
  /** Отображаемое имя (baseName пути или «безымянный»). */
  name: string;
}

export interface DocStoreOptions {
  /** Инспектор активен? — вызывается в момент рендера, а не при подписке. */
  renderMapped(): boolean;
  /** Единственная точка применения HTML (previewView.applyRender). */
  onRender(res: RenderResult, source: string): void;
  /** Сообщение в статус-бар (например, ошибка рендера). */
  onStatus(msg: string): void;
}

export interface DocStore {
  /** Список всех открытых вкладок. */
  tabs(): Tab[];
  /** ID активной вкладки или `null`, если нет открытых. */
  activeId(): DocumentId | null;
  /** Активная вкладка или `null`. */
  active(): Tab | null;
  /** Подписка на изменение списка вкладок/активной вкладки; возвращает unsubscribe (§5.6). */
  subscribe(cb: (tabs: Tab[], activeId: DocumentId | null) => void): () => void;

  /** Текущий активный документ как снапшот для ViewContext (§5.4) или `null`. */
  snapshot(): DocumentSnapshot | null;
  /** Подписка на снапшоты созданного документа (ViewContext.onDocument); unsubscribe. */
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
  /** Повторный рендер активной вкладки с текущим `mapped`. */
  reload(): void;
  /**
   * Внешняя правка документа (событие `document-updated`): pull-снапшот из Rust
   * и применение к соответствующей вкладке. Устаревшие/чужие события игнорируются.
   */
  applyHostUpdate(hostDocId: string, rev: number): void;

  /** Создать новую вкладку (не закрывая существующие). */
  newDocument(text?: string): Promise<void>;
  /** Открыть файл в новой вкладке (не закрывая существующие). */
  open(path: string): Promise<void>;
  /** Сохранить активную вкладку. */
  save(path?: string): Promise<void>;
  /** Переключиться на вкладку по ID. */
  switchTab(id: DocumentId): void;
  /** Закрыть вкладку по ID + переключиться на соседнюю. */
  closeTab(id: DocumentId): Promise<void>;
}

/** Дебаунс IPC-рендера: не спамить `update_document` на каждое нажатие. */
const DEBOUNCE_MS = 120;

/** Имя файла из пути (для таба-заглушки). */
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
    if (res.rev < lastRev) return; // устаревший ответ — не трогаем DOM
    lastRev = res.rev;
    if (activeId) {
      tabs = tabs.map((t) => (t.id === activeId ? { ...t, rev: res.rev } : t));
    }
    opts.onRender(res, source);
    notify();
  }

  /** Отправляет текст активной вкладки в Rust; ошибки пробрасываются вызывающему. */
  async function pushText(): Promise<void> {
    const id = activeId;
    if (!id) return;
    const tab = tabs.find((t) => t.id === id);
    if (!tab) return;
    const source = tab.text;
    const res = await ipcUpdate(id, source, opts.renderMapped());
    if (id !== activeId) return; // вкладка сменилась, пока ждали
    applyRes(res, source);
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
      // ViewContext-подписка: доставляем только созданный документ.
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
      tabs = tabs.map((t) => (t.id === id ? { ...t, text, dirty: true } : t));
      notify();
      schedule();
    },

    async flush(): Promise<void> {
      clearTimeout(debounceTimer);
      await pushText();
    },

    reload,

    applyHostUpdate(hostDocId: string, rev: number): void {
      const tab = tabs.find((t) => t.id === hostDocId);
      if (!tab) return; // событие о чужом/неизвестном документе
      if (rev <= tab.rev) return; // устаревшее событие или эхо собственной правки
      // Внешняя (плагинная) правка перекрывает отложенный push: команду вызвал
      // пользователь, её результат авторитетнее неотправленного локального ввода.
      // Отменяем дебаунс только у активной вкладки: у фоновой он относится к
      // собственному вводу и должен отработать как обычно.
      const isActive = tab.id === activeId;
      if (isActive) clearTimeout(debounceTimer);
      void (async () => {
        try {
          const snap = await documentSnapshot(tab.id);
          if (!snap || snap.rev < rev) return; // снапшот устарел или документ исчез
          // Держим `lastRev` на новой ревизии: устаревший ответ IPC (rev < snap.rev)
          // не должен откатить `st` и перерисовать DOM старым текстом (сброс в -1
          // эту защиту инвертировал бы).
          lastRev = snap.rev;
          // dirty=true: правка пришла от плагина и ещё не записана в файл.
          tabs = tabs.map((t) =>
            t.id === tab.id
              ? { ...t, rev: snap.rev, text: snap.text, dirty: true }
              : t
          );
          notify();
          if (isActive) {
            reload(); // перерисовать HTML из нового текста (textarea подтянет editorView)
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
      await pushText(); // текст редактора → Rust до записи (ошибку пробрасываем)
      const tab = tabs.find((t) => t.id === id);
      if (!tab) throw new Error("Документ не создан — сохранение недоступно");
      const meta = await ipcSave(id, path);
      if (id !== activeId) return; // вкладка сменилась, пока ждали записи
      tabs = tabs.map((t) =>
        t.id === id
          ? { ...t, path: meta.path, dirty: false, name: meta.path ? baseName(meta.path) : t.name }
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
        // Переключаемся на соседнюю: предыдущую, если есть, иначе следующую.
        const next = tabs[Math.min(idx, tabs.length - 1)] ?? null;
        activeId = next?.id ?? null;
        lastRev = -1;
        clearTimeout(debounceTimer);
      }
      void ipcClose(id).catch((e) => opts.onStatus(`Ошибка закрытия: ${errorMessage(e)}`));
      if (tabs.length === 0) {
        // Закрыта последняя вкладка — открываем пустую, чтобы редактор не оставался пустым.
        await newDocument();
        return;
      }
      notify();
      if (wasActive && activeId) reload();
    },
  };
}
