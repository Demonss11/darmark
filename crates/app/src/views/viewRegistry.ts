// viewRegistry.ts — контракты двух тиров представлений и реестр провайдеров
// (DESIGN_DOC §5.3–§5.4).
//
// Тир 1 (`HtmlViewProvider`) — plugin-safe контракт: JSON на входе, HTML на
// выходе, никакого прямого DOM-доступа извне. Preview реализуется по нему, но
// встраивается на хосте (D2): завтра `createView` станет вызовом плагина без
// смены интерфейса. Тир 2 (`DomViewProvider`) — built-in only: редактор и
// будущие контроллеры, которым нужен полный DOM.
//
// `ViewContext` — единственная «песочница» view: никакого доступа к
// `document`/`window`/`fetch`/файловой системе, только фасад.

import type { PaneId, ViewId } from "../core/ids";
import type { DocumentSnapshot, RenderResult } from "../core/tauri";

type ViewKind = string;

export type Json = string | number | boolean | null | Json[] | { [k: string]: Json };

export interface ViewContext {
  readonly paneId: PaneId;
  readonly viewId: ViewId;
  document(): DocumentSnapshot | null;
  edit(text: string): void;
  render(text: string, mapped: boolean): Promise<RenderResult>;
  status(msg: string): void;
  openExternal(url: string): void;
  onDocument(cb: (d: DocumentSnapshot) => void): () => void;
}

export interface HtmlView {
  render(doc: DocumentSnapshot): Promise<void>;
  dispose(): void;
}

export interface HtmlViewProvider {
  readonly kind: ViewKind;
  readonly title: string;
  createView(ctx: ViewContext, opts: Json): HtmlView;
}

interface DomView {
  activate(): void;
  deactivate(): void;
  dispose(): void;
}

interface DomViewProvider {
  readonly kind: ViewKind;
  readonly title: string;
  createView(ctx: ViewContext, host: HTMLElement, opts: Json): DomView;
}

export interface ViewRegistry {
  registerHtml(provider: HtmlViewProvider): void;
  registerDom(provider: DomViewProvider): void;
  unregisterHtml(kind: ViewKind): boolean;
  htmlProvider(kind: ViewKind): HtmlViewProvider | null;
  domProvider(kind: ViewKind): DomViewProvider | null;
  createHtmlView<T extends HtmlView = HtmlView, O = Json>(
    kind: ViewKind,
    ctx: ViewContext,
    opts?: O
  ): T;
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
    unregisterHtml: (kind) => html.delete(kind),
    htmlProvider: (kind) => html.get(kind) ?? null,
    domProvider: (kind) => dom.get(kind) ?? null,
    createHtmlView<T extends HtmlView, O>(kind: ViewKind, ctx: ViewContext, opts?: O): T {
      const provider = html.get(kind);
      if (!provider) throw new Error(`HtmlViewProvider не зарегистрирован: ${kind}`);
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
