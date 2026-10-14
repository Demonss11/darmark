// IPC-команды документов.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentId } from "../ids";

export interface DocumentSnapshot {
  id: DocumentId;
  path: string | null;
  rev: number;
  text: string;
  dirty_hint: boolean;
}

interface DocMeta {
  id: DocumentId;
  path: string | null;
  rev: number;
  dirty_hint: boolean;
}

export interface RenderResult {
  html: string;
  rev: number;
  changed: boolean;
}

export function newDocument(text?: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("new_document", { text: text ?? null });
}

export function openDocument(path: string): Promise<DocumentSnapshot> {
  return invoke<DocumentSnapshot>("open_document", { path });
}

export function updateDocument(
  id: DocumentId,
  text: string,
  mapped = false
): Promise<RenderResult> {
  return invoke<RenderResult>("update_document", { id, text, mapped });
}

export function renderDocument(id: DocumentId, mapped = false): Promise<RenderResult> {
  return invoke<RenderResult>("render_document", { id, mapped });
}

export interface UndoResult {
  text: string;
  rev: number;
  start: number;
  end: number;
}

export function undoDocument(id: DocumentId): Promise<UndoResult> {
  return invoke<UndoResult>("undo_document", { id });
}

export function redoDocument(id: DocumentId): Promise<UndoResult> {
  return invoke<UndoResult>("redo_document", { id });
}

export function saveDocument(id: DocumentId, path?: string): Promise<DocMeta> {
  return invoke<DocMeta>("save_document", { id, path: path ?? null });
}

export function closeDocument(id: DocumentId): Promise<void> {
  return invoke<void>("close_document", { id });
}

export function documentSnapshot(id: DocumentId): Promise<DocumentSnapshot | null> {
  return invoke<DocumentSnapshot | null>("document_snapshot", { id });
}
