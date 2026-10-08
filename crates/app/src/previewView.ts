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
