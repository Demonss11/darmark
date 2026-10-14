// images.ts — отображение локальных картинок в предпросмотре.
//
// Markdown ссылается на картинки относительно файла (`./logo.svg`), но WebView
// резолвит относительный URL от origin приложения (tauri://localhost), где файла
// нет, а CSP не пускает локальные ресурсы. Поэтому после рендера относительные
// `src` заменяются на asset-URL Tauri (`convertFileSrc`), а Rust при открытии/
// сохранении файла заранее расширяет scope asset-протокола его каталогом.
import { convertFileSrc } from "@tauri-apps/api/core";

function hasScheme(src: string): boolean {
  if (src.startsWith("//") || src.startsWith("#")) return true;
  return /^[a-z][a-z0-9+.-]*:/i.test(src);
}

function isWindowsAbsolute(src: string): boolean {
  return /^[a-zA-Z]:[\\/]/.test(src);
}

function separatorOf(p: string): string {
  return p.includes("\\") ? "\\" : "/";
}

function joinPath(baseDir: string, rel: string): string {
  const sep = separatorOf(baseDir);
  const combined = baseDir + sep + rel;
  const windows = isWindowsAbsolute(combined);
  const unix = combined.startsWith("/");
  const stack: string[] = [];
  for (const seg of combined.split(/[\\/]+/)) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (stack.length > 0 && stack[stack.length - 1] !== "..") stack.pop();
      continue;
    }
    stack.push(seg);
  }
  const joined = stack.join(sep);
  if (windows) return joined;
  if (unix) return sep + joined;
  return joined;
}

export function resolveLocalImages(root: HTMLElement, docPath: string | null): void {
  if (!docPath) return;
  const sep = separatorOf(docPath);
  const baseDir = docPath.split(/[\\/]/).slice(0, -1).join(sep);
  const windowsDoc = sep === "\\";
  for (const img of root.querySelectorAll<HTMLImageElement>("img[src]")) {
    const src = img.getAttribute("src");
    if (!src) continue;
    if (isWindowsAbsolute(src) || (!windowsDoc && src.startsWith("/") && !src.startsWith("//"))) {
      img.setAttribute("src", convertFileSrc(src));
    } else if (!hasScheme(src)) {
      img.setAttribute("src", convertFileSrc(joinPath(baseDir, src)));
    }
  }
}
