// Тонкие типизированные обёртки над Tauri IPC.
// Вся тяжёлая логика (парсинг md) — на Rust-стороне в crate md-core.
import { invoke } from "@tauri-apps/api/core";

export function renderMarkdown(markdown: string, mapped = false): Promise<string> {
  return invoke<string>("render_markdown", { markdown, mapped });
}

export function readFile(path: string): Promise<string> {
  return invoke<string>("read_file", { path });
}

export function writeFile(path: string, contents: string): Promise<void> {
  return invoke<void>("write_file", { path, contents });
}
