





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
} from "../../core/tauri";
import type { DocumentId } from "../../core/ids";
import type { DocumentSnapshot, RenderResult } from "../../core/tauri";
import { DEBOUNCE_MS, errorCode, baseName, makeTab } from "./helpers";
import type { Tab, DocStoreOptions, DocStore } from "./types";

export type { Tab, DocStore } from "./types";


export function createDocStore(opts: DocStoreOptions): DocStore {
  let tabs: Tab[] = [];
  let activeId: DocumentId | null = null;
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
    if (res.rev < lastRev) return;
    lastRev = res.rev;
    if (activeId) {
      tabs = tabs.map((t) => (t.id === activeId ? { ...t, rev: res.rev } : t));
    }
    opts.onRender(res, source);
    notify();
  }

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
      lastRev = -1;
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
      if (res.rev < lastRev) return null;
      lastRev = res.rev;
      tabs = tabs.map((t) =>
        t.id === id
          ? { ...t, text: res.text, rev: res.rev, dirty: res.text !== t.savedText }
          : t
      );
      notify();
      reload();
      return { start: res.start, end: res.end };
    } catch (e) {
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
      if (!tab) return;
      if (rev <= tab.rev) return;
      const isActive = tab.id === activeId;
      if (isActive) clearTimeout(debounceTimer);
      void (async () => {
        try {
          const snap = await documentSnapshot(tab.id);
          if (!snap || snap.rev < rev) return;
          lastRev = snap.rev;
          tabs = tabs.map((t) =>
            t.id === tab.id
              ? { ...t, rev: snap.rev, text: snap.text, dirty: snap.text !== t.savedText }
              : t
          );
          notify();
          if (isActive) {
            reload();
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
        const next = tabs[Math.min(idx, tabs.length - 1)] ?? null;
        activeId = next?.id ?? null;
        lastRev = -1;
        clearTimeout(debounceTimer);
      }
      void ipcClose(id).catch((e: unknown) => opts.onStatus(`Ошибка закрытия: ${errorMessage(e)}`));
      if (tabs.length === 0) {
        await newDocument();
        return;
      }
      notify();
      if (wasActive && activeId) reload();
    },
  };
}
