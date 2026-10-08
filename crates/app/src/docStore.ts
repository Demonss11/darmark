// docStore — TS-проекция Rust-стора (D5): текст, ревизия, путь и dirty.
//
// Чистый модуль без DOM: ввод приходит из editorView (`setText`), HTML уходит
// в `onRender` (в Фазе 4 — в previewView.applyRender). Рендер и его кэш
// принадлежат Rust; здесь — дебаунс IPC, защита от гонки ответов и подписки.
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
export interface DocState {
  id: DocumentId | null;
  path: string | null;
  rev: number;
  /** Актуальный буфер редактора (обновляется синхронно вводу, до IPC). */
  text: string;
  dirty: boolean;
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
  state(): DocState;
  /** Подписка на изменение проекции; возвращает unsubscribe (§5.6). */
  subscribe(cb: (s: DocState) => void): () => void;

  /** Текущий документ как снапшот для ViewContext (§5.4) или `null`. */
  snapshot(): DocumentSnapshot | null;
  /** Подписка на снапшоты созданного документа (ViewContext.onDocument); unsubscribe. */
  onDocument(cb: (d: DocumentSnapshot) => void): () => void;
  /**
   * Рендер переданного текста без применения результата (ViewContext.render):
   * вызывающий сам решает, куда деть HTML. Не путать с конвейером `setText`.
   */
  renderText(text: string, mapped: boolean): Promise<RenderResult>;

  /** Правка из редактора: сразу в проекцию + dirty + дебаунс IPC-рендера. */
  setText(text: string): void;
  /** Отменить дебаунс и немедленно отправить текст в Rust (перед save); ошибку пробрасывает. */
  flush(): Promise<void>;
  /** Повторный рендер с текущим `mapped` (первый рендер, переключение инспектора). */
  reload(): void;
  /**
   * Внешняя правка документа (событие `document-updated`): pull-снапшот из Rust
   * и применение к проекции. Устаревшие/чужие события игнорируются.
   */
  applyHostUpdate(hostDocId: string, rev: number): void;

  newDocument(text?: string): Promise<void>;
  open(path: string): Promise<void>;
  save(path?: string): Promise<void>;
}

/** Дебаунс IPC-рендера: не спамить `update_document` на каждое нажатие. */
const DEBOUNCE_MS = 120;

const EMPTY: DocState = { id: null, path: null, rev: 0, text: "", dirty: false };

export function createDocStore(opts: DocStoreOptions): DocStore {
  let st: DocState = { ...EMPTY };
  // Владелец кэша и ревизии — Rust; `lastRev` лишь отбрасывает устаревшие ответы IPC.
  let lastRev = -1;
  let debounceTimer = 0;
  const subs = new Set<(s: DocState) => void>();

  function notify(): void {
    for (const cb of subs) cb(st);
  }

  function schedule(): void {
    clearTimeout(debounceTimer);
    debounceTimer = window.setTimeout(() => void renderCurrent(), DEBOUNCE_MS);
  }

  function applyRes(res: RenderResult, source: string): void {
    if (res.rev < lastRev) return; // устаревший ответ — не трогаем DOM
    lastRev = res.rev;
    st = { ...st, rev: res.rev };
    opts.onRender(res, source);
    notify();
  }

  /** Отправляет текущий текст в Rust; ошибки пробрасываются вызывающему. */
  async function pushText(): Promise<void> {
    const id = st.id;
    if (!id) return;
    const source = st.text;
    const res = await ipcUpdate(id, source, opts.renderMapped());
    if (id !== st.id) return; // документ сменился, пока ждали
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
    st = { id: snap.id, path: snap.path, rev: snap.rev, text: snap.text, dirty: false };
    notify();
  }

  function reload(): void {
    const id = st.id;
    if (!id) return;
    clearTimeout(debounceTimer);
    void (async () => {
      try {
        const res = await ipcRender(id, opts.renderMapped());
        if (id !== st.id) return;
        applyRes(res, st.text);
      } catch (e) {
        if (id !== st.id) return;
        lastRev = -1;
        opts.onStatus(`Ошибка рендера: ${errorMessage(e)}`);
      }
    })();
  }

  function toSnapshot(s: DocState): DocumentSnapshot | null {
    if (!s.id) return null;
    return { id: s.id, path: s.path, rev: s.rev, text: s.text, dirty_hint: s.dirty };
  }

  return {
    state: () => st,

    subscribe(cb) {
      subs.add(cb);
      return () => subs.delete(cb);
    },

    snapshot: () => toSnapshot(st),

    onDocument(cb) {
      // ViewContext-подписка: доставляем только созданный документ.
      const inner = (s: DocState): void => {
        const snap = toSnapshot(s);
        if (snap) cb(snap);
      };
      subs.add(inner);
      return () => subs.delete(inner);
    },

    async renderText(text: string, mapped: boolean): Promise<RenderResult> {
      const id = st.id;
      if (!id) throw new Error("Документ не создан — рендер недоступен");
      // Тот же текст, что в сторе → render_document (текст не меняется).
      // Иначе update_document: стор применит правку — осознанный путь edit/render.
      if (text === st.text) return ipcRender(id, mapped);
      return ipcUpdate(id, text, mapped);
    },

    setText(text: string): void {
      if (text === st.text) return;
      st = { ...st, text, dirty: true };
      notify();
      schedule();
    },

    async flush(): Promise<void> {
      clearTimeout(debounceTimer);
      await pushText();
    },

    reload,

    applyHostUpdate(hostDocId: string, rev: number): void {
      const id = st.id;
      if (!id || id !== hostDocId) return; // событие о чужом/неизвестном документе
      if (rev <= st.rev) return; // устаревшее событие или эхо собственной правки
      // Внешняя (плагинная) правка перекрывает отложенный push: команду вызвал
      // пользователь, её результат авторитетнее неотправленного локального ввода.
      clearTimeout(debounceTimer);
      void (async () => {
        try {
          const snap = await documentSnapshot(id);
          if (id !== st.id) return; // документ сменился, пока ждали ответ
          if (!snap || snap.rev < rev) return; // снапшот устарел или документ исчез
          // Держим `lastRev` на новой ревизии: устаревший ответ IPC (rev < snap.rev)
          // не должен откатить `st` и перерисовать DOM старым текстом (сброс в -1
          // эту защиту инвертировал бы).
          lastRev = snap.rev;
          // dirty=true: правка пришла от плагина и ещё не записана в файл.
          st = { id: snap.id, path: snap.path, rev: snap.rev, text: snap.text, dirty: true };
          notify();
          reload(); // перерисовать HTML из нового текста (textarea подтянет editorView)
        } catch (e) {
          opts.onStatus(`Внешняя правка: ${errorMessage(e)}`);
        }
      })();
    },

    async newDocument(text = ""): Promise<void> {
      const previous = st.id;
      applySnapshot(await ipcNew(text));
      if (previous) void ipcClose(previous).catch(() => {});
      reload();
    },

    async open(path: string): Promise<void> {
      const previous = st.id;
      applySnapshot(await ipcOpen(path));
      if (previous) void ipcClose(previous).catch(() => {});
      reload();
    },

    async save(path?: string): Promise<void> {
      if (!st.id) throw new Error("Документ не создан — сохранение недоступно");
      await pushText(); // текст редактора → Rust до записи (ошибку пробрасываем)
      const id = st.id;
      if (!id) throw new Error("Документ не создан — сохранение недоступно");
      const meta = await ipcSave(id, path);
      st = { ...st, path: meta.path, dirty: false };
      notify();
    },
  };
}
