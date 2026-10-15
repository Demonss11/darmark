// Пузыри чата: сообщения пользователя и ассистента, рендер ответа, счётчик
// токенов, карточки ошибок и служебные плашки.
//
// Инвариант ADR-0038 §2.1/§2.2: HTML ответа модели рисуется только через
// `setSanitizedHtml` с результатом `renderAiMarkdown`; всё остальное (текст
// ошибок, плашки, расход токенов) — `textContent`.

import { renderAiMarkdown, setSanitizedHtml } from "../../core/safeHtml";
import { errorMessage } from "../../core/tauri";
import type { AiUsage } from "../../core/tauri";
import { el, nowTime } from "./dom";
import type { IndicatorApi } from "./indicator";
import type { ErrorAction } from "./errors";
import type { AiPanelState } from "./state";

interface AssistantRow {
  row: HTMLElement;
  bubble: HTMLElement;
  col: HTMLElement;
}

export interface ChatApi {
  appendUser(text: string): void;
  appendAssistant(): AssistantRow;
  appendUsage(col: HTMLElement, usage: AiUsage): void;
  appendErrorCard(title: string, text: string, actions: ErrorAction[]): void;
  appendNotice(col: HTMLElement, text: string, kind: "info" | "error"): void;
  renderBubble(bubble: HTMLElement, content: string): Promise<void>;
}

export function createChat(
  state: AiPanelState,
  indicator: IndicatorApi
): ChatApi {
  const { dom } = state;

  function appendUser(text: string): void {
    const row = document.createElement("div");
    row.className = "msg user";
    row.setAttribute("role", "article");
    row.setAttribute("aria-label", "Сообщение пользователя");
    const col = document.createElement("div");
    col.className = "msg-col";
    const bubble = document.createElement("div");
    bubble.className = "bubble";
    bubble.textContent = text;
    const time = document.createElement("span");
    time.className = "msg-time";
    time.textContent = nowTime();
    col.append(bubble, time);
    row.appendChild(col);
    dom.chat.appendChild(row);
    indicator.updateEmpty();
  }

  function appendAssistant(): AssistantRow {
    const row = document.createElement("div");
    row.className = "msg assistant";
    row.setAttribute("role", "article");
    row.setAttribute("aria-label", "Сообщение ассистента");
    const avatar = document.createElement("span");
    avatar.className = "avatar";
    avatar.setAttribute("aria-hidden", "true");
    avatar.textContent = "AI";
    const col = document.createElement("div");
    col.className = "msg-col";
    const bubble = document.createElement("div");
    bubble.className = "bubble";
    const time = document.createElement("span");
    time.className = "msg-time";
    time.textContent = nowTime();
    col.append(bubble, time);
    row.append(avatar, col);
    dom.chat.appendChild(row);
    indicator.updateEmpty();
    return { row, bubble, col };
  }

  function appendUsage(col: HTMLElement, usage: AiUsage): void {
    const el = document.createElement("span");
    el.className = "msg-usage";
    el.textContent = `токены: ${usage.prompt_tokens} + ${usage.completion_tokens}`;
    col.appendChild(el);
  }

  function appendErrorCard(title: string, text: string, actions: ErrorAction[]): void {
    const row = document.createElement("div");
    row.className = "msg assistant";
    const card = document.createElement("div");
    card.className = "error-card";
    card.setAttribute("role", "alert");
    const icon = document.createElement("span");
    icon.className = "error-icon";
    icon.setAttribute("aria-hidden", "true");
    icon.textContent = "⚠";
    const body = document.createElement("div");
    body.className = "error-body";
    const t = document.createElement("div");
    t.className = "error-title";
    t.textContent = title;
    const x = document.createElement("div");
    x.className = "error-text";
    x.textContent = text;
    body.append(t, x);
    if (actions.length > 0) {
      const foot = document.createElement("div");
      foot.className = "card-foot";
      for (const action of actions) {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "btn ghost";
        b.textContent = action.label;
        b.addEventListener("click", action.run);
        foot.appendChild(b);
      }
      body.appendChild(foot);
    }
    card.append(icon, body);
    row.appendChild(card);
    dom.chat.appendChild(row);
    indicator.updateEmpty();
    if (state.stick) indicator.scrollToBottom();
  }

  function appendNotice(col: HTMLElement, text: string, kind: "info" | "error"): void {
    const note = el("div", kind === "error" ? "ai-note ai-note-error" : "ai-note");
    note.setAttribute("role", kind === "error" ? "alert" : "status");
    note.textContent = text;
    col.appendChild(note);
    if (state.stick) indicator.scrollToBottom();
  }

  /**
   * Единственное место, где ответ модели превращается в HTML: санитайзер на
   * Rust-стороне отдаёт уже безопасную разметку, клиент лишь подставляет её.
   */
  async function renderBubble(bubble: HTMLElement, content: string): Promise<void> {
    try {
      const html = await renderAiMarkdown(content);
      if (state.disposed || !bubble.isConnected) return;
      setSanitizedHtml(bubble, html, content);
      if (state.stick) indicator.scrollToBottom();
    } catch (e) {
      if (state.disposed || !bubble.isConnected) return;
      bubble.textContent = content;
      state.log(
        "warn",
        "system",
        `AI: Markdown-рендер ответа недоступен — показан текст (${errorMessage(e)})`
      );
    }
  }

  return { appendUser, appendAssistant, appendUsage, appendErrorCard, appendNotice, renderBubble };
}