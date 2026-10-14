// safeHtml.ts — общий хелпер санитизированного рендера HTML на фронте (ADR-0038).
//
// Место, где HTML ответ модели присваивается элементу. Рядом в проекте есть ещё
// две точки `innerHTML`: `previewView.ts` (сырой результат `render_markdown`, HTML
// уже прошёл `md-core` на хосте) и `pluginViews.ts` (HTML вкладки плагина,
// санитизирован хостом в `plugins/views.rs:80`). Список исчерпывающий
// (ADR-0038 §2.1): любая новая точка — ревью-блокер, проверяется сторожевым
// поиском `innerHTML` по `crates/app/src` на этапе ревью.
//
// Инвариант ADR-0038 §2.1: весь HTML из внешнего/модельного источника, показываемый
// пользователю, обязан идти через `setSanitizedHtml`.
//
// Санитизация — на хосте (DESIGN_DOC §11.1), здесь стоит **дублирующая** проверка-инвариант:
// она не заменяет санитайзер, а при нарушении уводит рендер в `textContent` (fail-safe
// к тексту) вместо присваивания HTML. Хелпер ничего не логирует: ответ модели может
// содержать данные документа.

import { aiRenderMarkdown } from "./tauri";

const FORBIDDEN_TAGS = /<\s*(script|iframe|object|embed)/i;

const EVENT_HANDLER_ATTR = /\son[a-z]+\s*=/i;

const DANGEROUS_SCHEMES = /javascript:|vbscript:|data:text\/html/i;

function isSanitizedAiHtml(html: string): boolean {
  return (
    !FORBIDDEN_TAGS.test(html) &&
    !EVENT_HANDLER_ATTR.test(html) &&
    !DANGEROUS_SCHEMES.test(html)
  );
}

export function setSanitizedHtml(el: HTMLElement, html: string, fallbackText: string): void {
  if (isSanitizedAiHtml(html)) {
    el.innerHTML = html;
    return;
  }
  el.textContent = fallbackText;
}

export function renderAiMarkdown(text: string): Promise<string> {
  return aiRenderMarkdown(text);
}
