// previewView.ts — предпросмотр по тир-1 контракту (HtmlView), встроен на хосте (D2).
//
// Единственное место, где выполняется `preview.innerHTML = html` (ТЗ-H1, Фаза 4).
// Сюда переехали из `main.ts:onRender` применение HTML, украшение таблиц
// (`createTablesController`, per-view), `resolveLocalImages`, делегирование
// внешних ссылок и перестроение общего `RenderIndex`. Контракт
// `HtmlView.render(doc)` + `ViewContext` ничего хост-специфичного не знает —
// тот же класс может быть заменён плагином.
//
// Конвейер: docStore (`onRender`) вызывает `applyRender(res, source)`;
// `source` — текст, ушедший в рендер (не текущий буфер редактора), по нему
// строится индекс, иначе `data-md` разъедется с HTML.

import { openUrl } from "@tauri-apps/plugin-opener";
import { createTablesController } from "./panels/tables";
import { resolveLocalImages } from "./images";
import type { RenderIndex } from "./doc/renderIndex";
import type { HtmlView, HtmlViewProvider, Json, ViewContext } from "./viewRegistry";
import type { DocumentSnapshot, RenderResult } from "./core/tauri";

/** Расширение HtmlView для композиции: sink из docStore + управление mapped. */
export interface PreviewView extends HtmlView {
  /**
   * Единственная точка применения HTML: docStore (`onRender`) вызывает её на
   * каждый непротухший `RenderResult`. `source` — текст, отправленный в рендер.
   */
  applyRender(res: RenderResult, source: string): void;
  /** Текст изменился до рендера: индекс невалиден, потребители сбрасывают привязки. */
  invalidate(): void;
  /** Режим инспектора меняет HTML (`mapped`) — preview владеет этим флагом. */
  setMapped(mapped: boolean): void;
  /** Путь документа сменился (после «Сохранить как»): перерезолвить картинки. */
  refreshImages(): void;
}

/**
 * Живые ссылки на DOM-зависимости. Формально `opts` контракта — `Json` (§5.3),
 * но встроенный на хосте провайдер (D2) вправе передавать больше; плагин
 * получит строгий `Json`.
 */
export interface PreviewViewOptions {
  previewEl: HTMLElement;
  index: RenderIndex;
  /** После перестроения/сброса индекса: уведомить inspector/scrollsync (main.ts). */
  onIndexed(): void;
}

export const previewViewProvider: HtmlViewProvider = {
  kind: "preview",
  title: "Предпросмотр",
  createView(ctx: ViewContext, opts: Json): HtmlView {
    const o = opts as unknown as PreviewViewOptions;
    const { previewEl, index } = o;

    let mapped = false;

    // Внешние ссылки открываем системным браузером через плагин opener (P0.2):
    // window.open в WebView не гарантирует внешнее открытие и обходит CSP.
    const handleClick = (e: MouseEvent): void => {
      const anchor = (e.target as HTMLElement).closest("a");
      if (!anchor) return;
      const href = anchor.getAttribute("href") ?? "";
      if (/^https?:/i.test(href)) {
        e.preventDefault();
        void openUrl(href).catch((err) => ctx.status(`Ссылка: ${String(err)}`));
      }
    };
    previewEl.addEventListener("click", handleClick);
    // Таблицы: состояние меню — ЭКЗЕМПЛЯРА preview-вида (§5.6): dispose() убирает
    // меню из body и снимает слушатель.
    const tables = createTablesController(previewEl);

    // HTML, последний раз присвоенный `previewEl.innerHTML`. `res.changed` хоста
    // считается per-document, а предпросмотр один на всё приложение: при
    // возврате на ранее отрендеренную вкладку `changed` может быть `false`,
    // хотя в DOM остался HTML другого документа — поэтому сверяем и HTML.
    let lastHtml = "";

    function applyRender(res: RenderResult, source: string): void {
      if (res.changed || res.html !== lastHtml) {
        previewEl.innerHTML = res.html;
        lastHtml = res.html;
        try {
          tables.enhance();
        } catch (e) {
          // Украшение таблиц упало — оставляем читаемый HTML без улучшений (P1.2).
          ctx.status(`Таблицы: ${String(e)}`);
        }
      }
      // Локальные картинки: относительные src → asset-URL (идемпотентно).
      resolveLocalImages(previewEl, ctx.document()?.path ?? null);
      // Индекс привязан к тексту рендера (`source`) — переиндексация нужна всегда.
      index.set(source, res.rev, previewEl);
      o.onIndexed();
    }

    // Copy HTML: вместо копирования innerHTML (классы приложения) собираем
    // автономный фрагмент со встроенными стилями. Копирование — ВСЕГДА светлая
    // тема, независимо от темы приложения (ADR-0037). Один набор правил — и для
    // Markdown-превью, и для плагинных view; frontmatter копируется как есть.
    const COPY_CSS = `
      h1 { font-size: 30px; font-weight: 700; color: #0f1115; margin: 0 0 14px; border-bottom: 1px solid #b8bfcc; padding-bottom: 0.2em; }
      h2 { font-size: 21px; font-weight: 700; color: #0f1115; margin: 24px 0 10px; border-bottom: 1px solid #d0d5e0; padding-bottom: 0.2em; }
      h3 { font-size: 17px; font-weight: 700; color: #0f1115; margin: 18px 0 8px; }
      h4, h5, h6 { font-size: 14px; font-weight: 700; color: #0f1115; margin: 16px 0 6px; }
      p { margin: 0 0 10px; color: #2d3140; line-height: 1.7; }
      a { color: #1d4ed8; text-decoration: underline; }
      strong { font-weight: 700; color: #1a1d24; }
      em { font-style: italic; }
      code { font-family: ui-monospace, "Cascadia Code", Consolas, monospace; background: #d4d9e3; color: #6d28d9; padding: 2px 6px; border-radius: 4px; font-size: 12.5px; }
      pre { background: #f0f2f6; border: 1px solid #d0d5e0; border-radius: 8px; padding: 14px 18px; margin: 12px 0 18px; overflow-x: auto; }
      pre code { background: transparent; padding: 0; color: #4a5068; }
      blockquote { margin: 14px 0; padding: 4px 14px; color: #4a5068; background: rgba(85, 70, 208, 0.12); border-left: 3px solid #5546d0; border-radius: 0 6px 6px 0; }
      ul, ol { padding-left: 22px; margin: 8px 0 14px; color: #2d3140; }
      li { margin: 4px 0; }
      hr { border: none; border-top: 1px solid #b8bfcc; margin: 22px 0; }
      img { max-width: 100%; }
      table { border-collapse: collapse; margin: 1em 0; width: auto; max-width: 100%; font-size: 12.5px; }
      th, td { padding: 6px 13px; text-align: left; border: 1px solid #d0d5e0; color: #2d3140; }
      th { background: #e3e7ee; font-weight: 600; }
      input[type="checkbox"] { margin-right: 6px; }
    `.trim();
    const buildCopyHtml = (source: HTMLElement): string => {
      // Чистим клон, не трогая живой DOM: markdown-структура (теги) сохраняется.
      const clone = source.cloneNode(true) as HTMLElement;
      clone.removeAttribute("class");
      clone.removeAttribute("style");
      clone.removeAttribute("id");
      clone.removeAttribute("data-md");
      // У потомков тоже сбрасываем классы/инлайн-стили/интерактивные атрибуты:
      // копируем документ, а не рабочий view.
      for (const el of clone.querySelectorAll<HTMLElement>("*[class], *[style], *[data-md]")) {
        el.removeAttribute("class");
        el.removeAttribute("style");
        el.removeAttribute("data-md");
      }
      // UI таблиц — элементы приложения, а не содержимое документа.
      for (const sel of [".table-tools", ".table-count", ".col-filter-menu", ".col-filter-btn", ".col-filter-list", ".col-filter-item", ".col-filter-empty", ".col-filter-footer", ".table-scroll"]) {
        for (const el of clone.querySelectorAll<HTMLElement>(sel)) el.remove();
      }
      return [
        '<div style="background: #fafbfc; color: #1a1d24; font-family: -apple-system, \'Segoe UI\', system-ui, Roboto, Arial, sans-serif; line-height: 1.7; padding: 16px;">',
        `<style>${COPY_CSS}</style>`,
        clone.innerHTML,
        "</div>",
      ].join("\n");
    };
    const copyHtmlBtn = document.getElementById("btn-copy-html") as HTMLButtonElement | null;
    const handleCopyHtml = async (): Promise<void> => {
      const activeEl = previewEl.hidden ? (document.getElementById("plugin-view") as HTMLElement | null) : previewEl;
      if (!activeEl) return;
      const html = buildCopyHtml(activeEl);
      // text/plain = HTML-исходник (как writeText в idea4): вставка в текстовое
      // поле даёт разметку. text/html = rich-фрагмент со встроенными стилями.
      try {
        await navigator.clipboard.write([
          new ClipboardItem({
            "text/html": new Blob([html], { type: "text/html" }),
            "text/plain": new Blob([html], { type: "text/plain" }),
          }),
        ]);
        ctx.status("HTML скопирован в буфер обмена");
        return;
      } catch {
        // WebView2 может не поддерживать ClipboardItem с text/html — fallback ниже.
      }
      // Fallback для WebView2: временный div + execCommand копирует выделение
      // как rich text (text/html).
      const tempDiv = document.createElement("div");
      tempDiv.innerHTML = html;
      tempDiv.style.position = "fixed";
      tempDiv.style.left = "-9999px";
      tempDiv.style.top = "0";
      document.body.appendChild(tempDiv);
      const range = document.createRange();
      range.selectNodeContents(tempDiv);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      try {
        document.execCommand("copy");
        ctx.status("HTML скопирован в буфер обмена");
      } catch (err) {
        ctx.status(`Не удалось скопировать: ${String(err)}`);
      }
      selection?.removeAllRanges();
      document.body.removeChild(tempDiv);
    };
    if (copyHtmlBtn) copyHtmlBtn.addEventListener("click", () => void handleCopyHtml());

    const view: PreviewView = {
      applyRender,

      invalidate(): void {
        index.clear();
        o.onIndexed();
      },

      setMapped(value: boolean): void {
        mapped = value;
      },

      refreshImages(): void {
        resolveLocalImages(previewEl, ctx.document()?.path ?? null);
      },

      // Тир-1 путь: документ → HTML (через ctx.render) → применение.
      async render(doc: DocumentSnapshot): Promise<void> {
        const res = await ctx.render(doc.text, mapped);
        applyRender(res, doc.text);
      },

      dispose(): void {
        previewEl.removeEventListener("click", handleClick);
        tables.dispose();
        index.clear();
      },
    };
    return view;
  },
};
