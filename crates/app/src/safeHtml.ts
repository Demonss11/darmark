// safeHtml.ts — общий хелпер санитизированного рендера HTML на фронте (ADR-0038).
//
// Второе (и последнее) место, где HTML присваивается элементу, КРОМЕ
// `previewView.ts`: там — сырой результат `render_markdown` (HTML уже прошёл
// `md-core` на хосте), здесь — HTML, прошедший тот же санитайзер в команде
// `ai_render_markdown` (ответ AI-ассистента).
//
// Инвариант ADR-0038 §2.1: весь HTML из внешнего/модельного источника, показываемый
// пользователю, обязан идти через `setSanitizedHtml`. Третья точка `innerHTML` во
// фронте — ревью-блокер; проверяется сторожевым поиском по `crates/app/src`:
// допустимы только `previewView.ts` и этот файл.
//
// Санитизация — на хосте (DESIGN_DOC §11.1), здесь стоит **дублирующая** проверка-инвариант:
// она не заменяет санитайзер, а при нарушении уводит рендер в `textContent` (fail-safe
// к тексту) вместо присваивания HTML. Хелпер ничего не логирует: ответ модели может
// содержать данные документа.

import { aiRenderMarkdown } from "./tauri";

/** Теги с исполняемым/встраиваемым содержимым — санитайзер на хосте вырезает их с содержимым. */
const FORBIDDEN_TAGS = /<\s*(script|iframe|object|embed)/i;

/** Инлайн-обработчики (`onclick=` и т.п.) — вне allowlist атрибутов `md-core`. */
const EVENT_HANDLER_ATTR = /\son[a-z]+\s*=/i;

/** Опасные URL-схемы: `data:` для картинок допустим, для HTML-документа — нет. */
const DANGEROUS_SCHEMES = /javascript:|vbscript:|data:text\/html/i;

/**
 * Защитная проверка-инвариант результата `ai_render_markdown` (ADR-0038 §2.2).
 * `false`, если строка содержит запрещённый тег, атрибут-обработчик или опасную
 * схему; иначе `true`. Настоящая санитизация — на Rust-стороне, это второй барьер.
 */
export function isSanitizedAiHtml(html: string): boolean {
  return (
    !FORBIDDEN_TAGS.test(html) &&
    !EVENT_HANDLER_ATTR.test(html) &&
    !DANGEROUS_SCHEMES.test(html)
  );
}

/**
 * Единственная разрешённая точка `innerHTML` кроме превью. `html` обязан быть
 * строкой от `renderAiMarkdown` (происхождение — контракт, не тип); при срабатывании
 * проверки показывается `fallbackText` как текст, HTML не присваивается вовсе.
 */
export function setSanitizedHtml(el: HTMLElement, html: string, fallbackText: string): void {
  if (isSanitizedAiHtml(html)) {
    el.innerHTML = html;
    return;
  }
  el.textContent = fallbackText;
}

/** Тонкая обёртка над командой `ai_render_markdown`: текст ответа → санитизированный HTML. */
export function renderAiMarkdown(text: string): Promise<string> {
  return aiRenderMarkdown(text);
}