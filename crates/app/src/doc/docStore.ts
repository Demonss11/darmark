// docStore вЂ” TS-РїСЂРѕРµРєС†РёСЏ Rust-СЃС‚РѕСЂР° (D5): С‚РµРєСЃС‚, СЂРµРІРёР·РёСЏ, РїСѓС‚СЊ Рё dirty.
//
// РњРѕРґСѓР»СЊ Р±РµР· DOM: РІРІРѕРґ РїСЂРёС…РѕРґРёС‚ РёР· editorView (`setText`), HTML СѓС…РѕРґРёС‚ РІ
// `onRender`. Р РµРЅРґРµСЂ Рё РµРіРѕ РєСЌС€ РїСЂРёРЅР°РґР»РµР¶Р°С‚ Rust; Р·РґРµСЃСЊ вЂ” РґРµР±Р°СѓРЅСЃ IPC,
// РѕС‚Р±СЂР°СЃС‹РІР°РЅРёРµ СѓСЃС‚Р°СЂРµРІС€РёС… РѕС‚РІРµС‚РѕРІ Рё РїРѕРґРїРёСЃРєРё. РќРёРєР°РєРёС… РѕР±СЂР°С‰РµРЅРёР№ Рє document/window.

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
} from "../core/tauri";
import type { DocumentId } from "../core/ids";
import type { DocumentSnapshot, RenderResult } from "../core/tauri";

/** РџСЂРѕРµРєС†РёСЏ РґРѕРєСѓРјРµРЅС‚Р°: Р·РµСЂРєР°Р»Рѕ Rust-СЃС‚РѕСЂР° + Р±СѓС„РµСЂ СЂРµРґР°РєС‚РѕСЂР° Рё dirty. */
export interface Tab {
  id: DocumentId;
  path: string | null;
  rev: number;
  /** РђРєС‚СѓР°Р»СЊРЅС‹Р№ Р±СѓС„РµСЂ СЂРµРґР°РєС‚РѕСЂР° (РѕР±РЅРѕРІР»СЏРµС‚СЃСЏ СЃРёРЅС…СЂРѕРЅРЅРѕ РІРІРѕРґСѓ, РґРѕ IPC). */
  text: string;
  /** РўРµРєСЃС‚ РЅР° РјРѕРјРµРЅС‚ РїРѕСЃР»РµРґРЅРµРіРѕ СЃРѕС…СЂР°РЅРµРЅРёСЏ/РѕС‚РєСЂС‹С‚РёСЏ вЂ” Р±Р°Р·Р° РґР»СЏ РІС‹РІРѕРґР° `dirty`. */
  savedText: string;
  /** РџСЂРѕРёР·РІРѕРґРЅРѕРµ РѕС‚ `text !== savedText`: РЅРµР·Р°РїРёСЃР°РЅРЅС‹Рµ РІ С„Р°Р№Р» РёР·РјРµРЅРµРЅРёСЏ (В§4.2). */
  dirty: boolean;
  name: string;
}

export interface DocStoreOptions {
  /** Р’С‹Р·С‹РІР°РµС‚СЃСЏ РІ РјРѕРјРµРЅС‚ СЂРµРЅРґРµСЂР°, Р° РЅРµ РїСЂРё РїРѕРґРїРёСЃРєРµ. */
  renderMapped(): boolean;
  /** Р•РґРёРЅСЃС‚РІРµРЅРЅР°СЏ С‚РѕС‡РєР° РїСЂРёРјРµРЅРµРЅРёСЏ HTML (previewView.applyRender). */
  onRender(res: RenderResult, source: string): void;
  onStatus(msg: string): void;
}

export interface DocStore {
  tabs(): Tab[];
  activeId(): DocumentId | null;
  active(): Tab | null;
  /** РџРѕРґРїРёСЃРєР° РЅР° РёР·РјРµРЅРµРЅРёРµ СЃРїРёСЃРєР°/Р°РєС‚РёРІРЅРѕР№ РІРєР»Р°РґРєРё; РІРѕР·РІСЂР°С‰Р°РµС‚ unsubscribe (В§5.6). */
  subscribe(cb: (tabs: Tab[], activeId: DocumentId | null) => void): () => void;

  snapshot(): DocumentSnapshot | null;
  onDocument(cb: (d: DocumentSnapshot) => void): () => void;


  renderText(text: string, mapped: boolean): Promise<RenderResult>;

  /** РџСЂР°РІРєР° РёР· СЂРµРґР°РєС‚РѕСЂР°: СЃСЂР°Р·Сѓ РІ Р°РєС‚РёРІРЅСѓСЋ РІРєР»Р°РґРєСѓ + dirty + РґРµР±Р°СѓРЅСЃ IPC-СЂРµРЅРґРµСЂР°. */
  setText(text: string): void;
  /** РћС‚РјРµРЅРёС‚СЊ РґРµР±Р°СѓРЅСЃ Рё РЅРµРјРµРґР»РµРЅРЅРѕ РѕС‚РїСЂР°РІРёС‚СЊ С‚РµРєСЃС‚ Р°РєС‚РёРІРЅРѕР№ РІРєР»Р°РґРєРё РІ Rust. */
  flush(): Promise<void>;


  undo(): Promise<{ start: number; end: number } | null>;
  /** РџРѕРІС‚РѕСЂРёС‚СЊ РѕС‚РјРµРЅС‘РЅРЅСѓСЋ РїСЂР°РІРєСѓ; `null`, РµСЃР»Рё РїРѕРІС‚РѕСЂСЏС‚СЊ РЅРµС‡РµРіРѕ. */
  redo(): Promise<{ start: number; end: number } | null>;
  reload(): void;


  applyHostUpdate(hostDocId: string, rev: number): void;

  /** РќРѕРІР°СЏ РІРєР»Р°РґРєР°, РЅРµ Р·Р°РєСЂС‹РІР°СЏ СЃСѓС‰РµСЃС‚РІСѓСЋС‰РёРµ (`open` вЂ” С‚Рѕ Р¶Рµ РёР· С„Р°Р№Р»Р°). */
  newDocument(text?: string): Promise<void>;
  open(path: string): Promise<void>;
  save(path?: string): Promise<void>;
  switchTab(id: DocumentId): void;
  /** Р—Р°РєСЂС‹С‚СЊ РІРєР»Р°РґРєСѓ + РїРµСЂРµРєР»СЋС‡РёС‚СЊСЃСЏ РЅР° СЃРѕСЃРµРґРЅСЋСЋ. */
  closeTab(id: DocumentId): Promise<void>;
}

/** Р”РµР±Р°СѓРЅСЃ IPC-СЂРµРЅРґРµСЂР°: РЅРµ СЃРїР°РјРёС‚СЊ `update_document` РЅР° РєР°Р¶РґРѕРµ РЅР°Р¶Р°С‚РёРµ. */
const DEBOUNCE_MS = 120;

/** РљРѕРґ IPC-РѕС€РёР±РєРё РёР· `CommandError { code, message }` (РёР»Рё `undefined`). */
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
    name: snap.path ? baseName(snap.path) : "Р±РµР·С‹РјСЏРЅРЅС‹Р№",
  };
}

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
    if (res.rev < lastRev) return; // СѓСЃС‚Р°СЂРµРІС€РёР№ РѕС‚РІРµС‚
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
      lastRev = -1; // СЃРѕРѕР±С‰РµРЅРёРµ РѕР± РѕС€РёР±РєРµ РґРѕР»Р¶РЅРѕ РїРµСЂРµР¶РёС‚СЊ СЃР»РµРґСѓСЋС‰РёР№ changed:false
      opts.onStatus(`РћС€РёР±РєР° СЂРµРЅРґРµСЂР°: ${errorMessage(e)}`);
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
        opts.onStatus(`РћС€РёР±РєР° СЂРµРЅРґРµСЂР°: ${errorMessage(e)}`);
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
      opts.onStatus(`РћС€РёР±РєР° СЂРµРЅРґРµСЂР°: ${errorMessage(e)}`);
      return null;
    }
    if (id !== activeId) return null;

    try {
      const res = direction === "undo" ? await ipcUndo(id) : await ipcRedo(id);
      if (id !== activeId) return null;
      if (res.rev < lastRev) return null; // РіРѕРЅРєР° РґРІСѓС… РѕС‚РјРµРЅ РїРѕРґСЂСЏРґ

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
      opts.onStatus(`РћС‚РјРµРЅР°/РїРѕРІС‚РѕСЂ: ${errorMessage(e)}`);
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
      if (!id) throw new Error("Р”РѕРєСѓРјРµРЅС‚ РЅРµ СЃРѕР·РґР°РЅ вЂ” СЂРµРЅРґРµСЂ РЅРµРґРѕСЃС‚СѓРїРЅРѕ");
      const tab = tabs.find((t) => t.id === id);
      if (!tab) throw new Error("Р”РѕРєСѓРјРµРЅС‚ РЅРµ СЃРѕР·РґР°РЅ вЂ” СЂРµРЅРґРµСЂ РЅРµРґРѕСЃС‚СѓРїРЅРѕ");


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
      if (!tab) return; // С‡СѓР¶РѕР№/РЅРµРёР·РІРµСЃС‚РЅС‹Р№ РґРѕРєСѓРјРµРЅС‚
      if (rev <= tab.rev) return; // СѓСЃС‚Р°СЂРµРІС€РµРµ СЃРѕР±С‹С‚РёРµ РёР»Рё СЌС…Рѕ СЃРѕР±СЃС‚РІРµРЅРЅРѕР№ РїСЂР°РІРєРё




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
            reload(); // textarea РїРѕРґС‚СЏРЅРµС‚ editorView
          }
        } catch (e) {
          opts.onStatus(`Р’РЅРµС€РЅСЏСЏ РїСЂР°РІРєР°: ${errorMessage(e)}`);
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
      if (!id) throw new Error("Р”РѕРєСѓРјРµРЅС‚ РЅРµ СЃРѕР·РґР°РЅ вЂ” СЃРѕС…СЂР°РЅРµРЅРёРµ РЅРµРґРѕСЃС‚СѓРїРЅРѕ");


      const savedText = await pushText();
      const tab = tabs.find((t) => t.id === id);
      if (!tab) throw new Error("Р”РѕРєСѓРјРµРЅС‚ РЅРµ СЃРѕР·РґР°РЅ вЂ” СЃРѕС…СЂР°РЅРµРЅРёРµ РЅРµРґРѕСЃС‚СѓРїРЅРѕ");
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
      void ipcClose(id).catch((e: unknown) => opts.onStatus(`РћС€РёР±РєР° Р·Р°РєСЂС‹С‚РёСЏ: ${errorMessage(e)}`));
      if (tabs.length === 0) {

        await newDocument();
        return;
      }
      notify();
      if (wasActive && activeId) reload();
    },
  };
}
