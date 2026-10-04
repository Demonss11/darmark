// Тонкие типизированные обёртки над Tauri IPC.
// Вся тяжёлая логика (парсинг md) — на Rust-стороне в crate md-core.
import { invoke } from "@tauri-apps/api/core";

export function renderMarkdown(markdown: string): Promise<string> {
  return invoke<string>("render_markdown", { markdown });
}

export function readFile(path: string): Promise<string> {
  return invoke<string>("read_file", { path });
}

export function writeFile(path: string, contents: string): Promise<void> {
  return invoke<void>("write_file", { path, contents });
}

// Нативные диалоги живут на Rust-стороне: только так выбранный каталог
// может быть «запомнен» как разрешённый, не доверяя фронтенду.
export function pickOpenFile(): Promise<string | null> {
  return invoke<string | null>("pick_open_file");
}

export function pickSaveFile(defaultPath: string | null): Promise<string | null> {
  return invoke<string | null>("pick_save_file", { defaultPath });
}
