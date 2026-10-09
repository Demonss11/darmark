// previewView.ts — предпросмотр по тир-1 контракту (HtmlView), встроен на хосте (D2).
//
// Единственное место, где выполняется `preview.innerHTML = html` (ТЗ-H1, Фаза 4).
// Сюда переехали из `main.ts:onRender`: применение HTML, украшение таблиц
// (`createTablesController`, per-view), `resolveLocalImages`, делегирование
// внешних ссылок и перестроение общего `RenderIndex`. Завтра тот же класс может быть заменён плагином: контракт
// `HtmlView.render(doc)` + `ViewContext` ничего хост-специфичного не знают.
//
// Конвейер: docStore (`onRender`) вызывает `applyRender(res, source)`;
// `source` — текст, ушедший в рендер (не текущий буфер редактора), по нему
// строится индекс, иначе `data-md` разъедется с HTML.

import { openUrl } from "@tauri-apps/plugin-opener";
import { createTablesController } from "./tables";
import { resolveLocalImages } from "./images";
import type { RenderIndex } from "./renderIndex";
import type { HtmlView, HtmlViewProvider, Json, ViewContext } from "./viewRegistry";
import type { DocumentSnapshot, RenderResult } from "./tauri";

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
 * Живые ссылки на DOM-зависимости. Формально `opts` контракта — `Json`
 * (§5.3), но встроенный на хосте провайдер (D2) вправе передавать больше;
 * плагин-провайдер получит строгий `Json`.
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
    // Таблицы: украшение + меню фильтров — состояние ЭКЗЕМПЛЯРА preview-вида
    // (§5.6): меню вешается в body, dispose() его убирает и снимает слушатель.
    const tables = createTablesController(previewEl);

    function applyRender(res: RenderResult, source: string): void {
      if (res.changed) {
        previewEl.innerHTML = res.html;
        try {
          tables.enhance(); // Excel-подобные сортировка/фильтры для всех <table>
        } catch (e) {
          // Украшение таблиц упало — оставляем читаемый HTML без улучшений (P1.2).
          ctx.status(`Таблицы: ${String(e)}`);
        }
      }
      // Локальные картинки: относительные src → asset-URL (идемпотентно).
      resolveLocalImages(previewEl, ctx.document()?.path ?? null);
      // Переиндексация нужна всегда: индекс привязан к тексту рендера (`source`).
      index.set(source, res.rev, previewEl);
      o.onIndexed();
    }

    // Copy HTML: автономный HTML-фрагмент для буфера обмена. Вместо копирования
    // текущего innerHTML (тёмная тема, классы приложения) собираем документ со
    // встроенными стилями: белый фон, чёрный текст. Один набор правил — и для
    // Markdown-превью, и для плагинных view. Frontmatter (YAML-шапка) копируется
    // как есть — таблицей.
    const COPY_CSS = `
      h1 { font-size: 26px; font-weight: 700; color: #000; margin: 0 0 14px; border-bottom: 1px solid #ddd; padding-bottom: 0.2em; }
      h2 { font-size: 19px; font-weight: 700; color: #000; margin: 24px 0 10px; border-bottom: 1px solid #ddd; padding-bottom: 0.2em; }
      h3 { font-size: 15px; font-weight: 700; color: #000; margin: 18px 0 8px; }
      h4, h5, h6 { font-size: 14px; font-weight: 700; color: #000; margin: 16px 0 6px; }
      p { margin: 0 0 10px; color: #000; line-height: 1.7; }
      a { color: #0066cc; text-decoration: underline; }
      strong { font-weight: 700; color: #000; }
      em { font-style: italic; }
      code { font-family: "Cascadia Code", Consolas, monospace; background: #f5f5f5; color: #333; padding: 2px 6px; border-radius: 4px; font-size: 12.5px; }
      pre { background: #f5f5f5; border: 1px solid #ddd; border-radius: 8px; padding: 14px 18px; margin: 12px 0 18px; overflow-x: auto; }
      pre code { background: transparent; padding: 0; color: #333; }
      blockquote { margin: 14px 0; padding: 4px 14px; color: #555; background: #f9f9f9; border-left: 3px solid #0066cc; border-radius: 0 6px 6px 0; }
      ul, ol { padding-left: 22px; margin: 8px 0 14px; color: #000; }
      li { margin: 4px 0; }
      hr { border: none; border-top: 1px solid #ddd; margin: 22px 0; }
      img { max-width: 100%; }
      table { border-collapse: collapse; margin: 1em 0; width: auto; max-width: 100%; font-size: 12.5px; }
      th, td { padding: 6px 13px; text-align: left; border: 1px solid #ddd; color: #000; }
      th { background: #f5f5f5; font-weight: 600; }
      input[type="checkbox"] { margin-right: 6px; }
    `.trim();
    const buildCopyHtml = (source: HTMLElement): string => {
      // Клонируем контейнер, чтобы вычистить классы/стили/атрибуты приложения,
      // не трогая живой DOM. Markdown-структура (теги) сохраняется.
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
      // Убираем UI таблиц (панель инструментов, счётчики, меню фильтров):
      // это интерактивные элементы приложения, а не содержимое документа.
      for (const sel of [".table-tools", ".table-count", ".col-filter-menu", ".col-filter-btn", ".col-filter-list", ".col-filter-item", ".col-filter-empty", ".col-filter-footer", ".table-scroll"]) {
        for (const el of clone.querySelectorAll<HTMLElement>(sel)) el.remove();
      }
      return [
        '<div style="background: #fff; color: #000; font-family: -apple-system, \'Segoe UI\', system-ui, Roboto, Arial, sans-serif; line-height: 1.7; padding: 16px;">',
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
      // Fallback для WebView2: временный div + execCommand('copy') с выделением.
      // execCommand копирует выделенный HTML как rich text (text/html) в буфер обмена.
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
      // В текущем конвейере главный вход — `applyRender` из docStore.
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
