// Хелперы проекции документа.

import type { DocumentSnapshot } from "../../core/tauri";
import type { Tab } from "./types";

export const DEBOUNCE_MS = 120;

export function errorCode(e: unknown): string | undefined {
  if (e && typeof e === "object" && "code" in e) {
    const code = (e as { code?: unknown }).code;
    if (typeof code === "string") return code;
  }
  return undefined;
}

export function baseName(p: string): string {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i >= 0 ? p.slice(i + 1) : p;
}

export function makeTab(snap: DocumentSnapshot): Tab {
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
