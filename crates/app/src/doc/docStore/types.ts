

import type { DocumentId } from "../../core/ids";
import type { DocumentSnapshot, RenderResult } from "../../core/tauri";

export interface Tab {
  id: DocumentId;
  path: string | null;
  rev: number;
  text: string;
  savedText: string;
  dirty: boolean;
  name: string;
}

export interface DocStoreOptions {
  renderMapped(): boolean;
  onRender(res: RenderResult, source: string): void;
  onStatus(msg: string): void;
}

export interface DocStore {
  tabs(): Tab[];
  activeId(): DocumentId | null;
  active(): Tab | null;
  subscribe(cb: (tabs: Tab[], activeId: DocumentId | null) => void): () => void;

  snapshot(): DocumentSnapshot | null;
  onDocument(cb: (d: DocumentSnapshot) => void): () => void;
  renderText(text: string, mapped: boolean): Promise<RenderResult>;

  setText(text: string): void;
  flush(): Promise<void>;
  undo(): Promise<{ start: number; end: number } | null>;
  redo(): Promise<{ start: number; end: number } | null>;
  reload(): void;
  applyHostUpdate(hostDocId: string, rev: number): void;

  newDocument(text?: string): Promise<void>;
  open(path: string): Promise<void>;
  save(path?: string): Promise<void>;
  switchTab(id: DocumentId): void;
  closeTab(id: DocumentId): Promise<void>;
}
