// viewRegistry.ts — контракты двух тиров представлений и реестр провайдеров
// (DESIGN_DOC §5.3–§5.4).
//
// Тир 1 (`HtmlViewProvider`) — plugin-safe контракт: JSON на входе, HTML на
// выходе, никакого прямого DOM-доступа извне. Preview реализуется по нему, но
// встраивается на хосте (D2): завтра `createView` станет вызовом плагина без
// смены интерфейса.
//
// Тир 2 (`DomViewProvider`) — built-in only: редактор и будущие контроллеры,
// которым нужен полный DOM.
//
// `ViewContext` — единственная «песочница» view: никакого доступа к
// `document`/`window`/`fetch`/файловой системе, только фасад.

import type { PaneId, ViewId } from "./ids";
import type { DocumentSnapshot, RenderResult } from "./tauri";

export type ViewKind = string;

/** JSON-совместимое значение — язык будущего контракта с плагинами (§7). */
export type Json = string | number | boolean | null | Json[] | { [k: string]: Json };

/** Единственный фасад доступа view к хосту (§5.4). */
export interface ViewContext {
  readonly paneId: PaneId;
  readonly viewId: ViewId;
  /** Текущий документ или `null` (не создан). */
  document(): DocumentSnapshot | null;
  /** Правка текста документа (путь редактора: setText → debounce → Rust). */
  edit(text: string): void;
  /** Рендер текста (`render_document`), результат — `RenderResult`. */
  render(text: string, mapped: boolean): Promise<RenderResult>;
  /** Сообщение в статус-бар. */
  status(msg: string): void;
  /** Открыть внешнюю ссылку системным средством. */
  openExternal(url: string): void;
  /** Подписка на смену/правку документа; возвращает unsubscribe (§5.6). */
  onDocument(cb: (d: DocumentSnapshot) => void): () => void;
}

/** Тир 1 — получатель HTML; `preview.innerHTML` живёт только внутри реализации. */
export interface HtmlView {
  render(doc: DocumentSnapshot): Promise<void>;
  dispose(): void;
}

export interface HtmlViewProvider {
  readonly kind: ViewKind;
  readonly title: string;
  createView(ctx: ViewContext, opts: Json): HtmlView;
}

/** Тир 2 — полный DOM; activate/deactivate — жизненный цикл будущих панелей (§5.2). */
export interface DomView {
  activate(): void;
  deactivate(): void;
  dispose(): void;
}

export interface DomViewProvider {
  readonly kind: ViewKind;
  readonly title: string;
  createView(ctx: ViewContext, host: HTMLElement, opts: Json): DomView;
}

export interface ViewRegistry {
  registerHtml(provider: HtmlViewProvider): void;
  registerDom(provider: DomViewProvider): void;
  /** Провайдер тира 1 по kind или `null`. */
  htmlProvider(kind: ViewKind): HtmlViewProvider | null;
  /** Провайдер тира 2 по kind или `null`. */
  domProvider(kind: ViewKind): DomViewProvider | null;
  /**
   * Создать экземпляр тира 1. `T` — расширение контракта конкретным view
   * (например, `PreviewView`); `O` — тип `opts`. Для плагинов это `Json`, для
   * встроенного на хосте провайдера (D2) — живые зависимости (`HTMLElement`,
   * `RenderIndex`, колбэк).
   */
  createHtmlView<T extends HtmlView = HtmlView, O = Json>(
    kind: ViewKind,
    ctx: ViewContext,
    opts?: O
  ): T;
  /** Создать экземпляр тира 2. */
  createDomView<T extends DomView = DomView, O = Json>(
    kind: ViewKind,
    ctx: ViewContext,
    host: HTMLElement,
    opts?: O
  ): T;
}

export function createViewRegistry(): ViewRegistry {
  const html = new Map<ViewKind, HtmlViewProvider>();
  const dom = new Map<ViewKind, DomViewProvider>();
  return {
    registerHtml(provider) {
      html.set(provider.kind, provider);
    },
    registerDom(provider) {
      dom.set(provider.kind, provider);
    },
    htmlProvider: (kind) => html.get(kind) ?? null,
    domProvider: (kind) => dom.get(kind) ?? null,
    createHtmlView<T extends HtmlView, O>(kind: ViewKind, ctx: ViewContext, opts?: O): T {
      const provider = html.get(kind);
      if (!provider) throw new Error(`HtmlViewProvider не зарегистрирован: ${kind}`);
      // Провайдер получает строгий `Json`; встроенный кастует обратно (§2.3 ТЗ-H1-F4).
      return provider.createView(ctx, (opts ?? {}) as unknown as Json) as T;
    },
    createDomView<T extends DomView, O>(
      kind: ViewKind,
      ctx: ViewContext,
      host: HTMLElement,
      opts?: O
    ): T {
      const provider = dom.get(kind);
      if (!provider) throw new Error(`DomViewProvider не зарегистрирован: ${kind}`);
      return provider.createView(ctx, host, (opts ?? {}) as unknown as Json) as T;
    },
  };
}
