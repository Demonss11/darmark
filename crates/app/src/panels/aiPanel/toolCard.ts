// Карточки вызовов инструментов MCP в чате ассистента (TZ-MCP, AC-MCP-5/7/8/13).
//
// Вызов приходит двумя аддитивными событиями Task 4: `ai:tool-call` рисует карточку в
// режиме «в процессе», `ai:tool-result` переводит её в итог по `call_id`. Наполнение —
// только `textContent` (ADR-0038 §2.3 п.9): аргументы вызова и результат инструмента
// могут нести путь и содержимое документа, поэтому в DOM попадают лишь имя инструмента,
// статус и длительность. Реестра карточек нет — владелец DOM это `p.col`, а очистка чата
// чистит и карточки. Поиск карточки — сравнением `data-call-id`, а не селектором:
// `call_id` приходит от провайдера, и подстановка его в `querySelector` дала бы битый
// селектор (исключение прямо в обработчике события).

import { el } from "./dom";
import type { AiPanelState } from "./state";

export interface ToolCardApi {
  /** Рисует карточку вызова в режиме «в процессе» (`ai:tool-call`). */
  appendCall(col: HTMLElement, callId: string, tool: string): void;
  /** Переводит карточку по `call_id` в итог: успех/ошибка, длительность, статус. */
  finishCall(
    col: HTMLElement,
    callId: string,
    ok: boolean,
    durationMs: number,
    summary: string
  ): void;
}

export function createToolCard(
  state: AiPanelState,
  deps: { scrollToBottom(): void }
): ToolCardApi {
  function findCard(col: HTMLElement, callId: string): HTMLElement | null {
    for (const card of col.querySelectorAll<HTMLElement>(".ai-tool-card")) {
      if (card.dataset.callId === callId) return card;
    }
    return null;
  }

  function appendCall(col: HTMLElement, callId: string, tool: string): void {
    const card = el("div", "ai-tool-card is-running");
    card.setAttribute("data-call-id", callId);
    card.setAttribute("data-tool", tool);
    card.setAttribute("role", "group");
    card.setAttribute("aria-label", `Вызов инструмента ${tool}`);

    const header = el("div", "ai-tool-header");
    const icon = el("span", "ai-tool-icon");
    icon.setAttribute("aria-hidden", "true");
    icon.textContent = "⚙";
    const name = el("span", "ai-tool-name");
    name.textContent = tool;
    const badge = el("span", "ai-tool-badge");
    badge.textContent = "выполняется";
    header.append(icon, name, badge);

    const status = el("div", "ai-tool-status");
    status.setAttribute("role", "status");
    status.textContent = "Вызов инструмента…";

    card.append(header, status);
    col.appendChild(card);
    if (state.stick) deps.scrollToBottom();
  }

  function finishCall(
    col: HTMLElement,
    callId: string,
    ok: boolean,
    durationMs: number,
    summary: string
  ): void {
    const card = findCard(col, callId);
    if (!card) return;
    const stateClass = ok ? "is-ok" : "is-error";
    card.classList.remove("is-running");
    card.classList.add(stateClass);
    const icon = card.querySelector<HTMLElement>(".ai-tool-icon");
    const badge = card.querySelector<HTMLElement>(".ai-tool-badge");
    const status = card.querySelector<HTMLElement>(".ai-tool-status");
    if (icon) icon.textContent = ok ? "✓" : "✗";
    if (badge) {
      badge.className = `ai-tool-badge ${stateClass}`;
      badge.textContent = summary;
    }
    if (status) status.textContent = `${card.dataset.tool ?? ""}: ${summary} (${durationMs} мс)`;
    if (state.stick) deps.scrollToBottom();
  }

  return { appendCall, finishCall };
}
