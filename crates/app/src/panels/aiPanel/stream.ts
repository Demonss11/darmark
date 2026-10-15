// Отправка запроса и его жизненный цикл: `send` → `runChat` → `complete`/`fail`.
//
// Защита от гонок: каждый запрос получает монотонный `token`; ответ IPC и
// события ai:done/ai:error принимаются только если `pending.token` совпадает и
// пузырь не финализирован. Смена запроса (новый send, retry, очистка чата)
// двигает `requestToken`, поэтому «протухший» ответ молча отбрасывается.

import { aiChat, aiHistoryClear, errorMessage } from "../../core/tauri";
import type { AiUsage } from "../../core/tauri";
import type { ChatApi } from "./chat";
import type { ConsentApi } from "./consent";
import { errorKind, errorTitle, type ErrorAction, type ErrorKind } from "./errors";
import type { HistoryApi } from "./history";
import type { IndicatorApi } from "./indicator";
import type { PluginCardApi } from "./pluginCard";
import type { AiPanelState, Pending } from "./state";

export interface StreamApi {
  send(display: string, content: string): void;
  retry(): void;
  submitInput(): void;
  clearChat(): void;
  // Финализаторы: вызываются из подписок ai:done/ai:error, которые живут в
  // index.ts и работают с тем же объектом `pending`.
  complete(p: Pending, usage: AiUsage | null): void;
  fail(p: Pending, code: string, message: string): void;
}

export function createStream(
  state: AiPanelState,
  deps: {
    chat: ChatApi;
    indicator: IndicatorApi;
    history: HistoryApi;
    plugin: PluginCardApi;
    consent: ConsentApi;
    openSettings(): void;
  }
): StreamApi {
  const { dom } = state;

  function errorActions(kind: ErrorKind, code: string): ErrorAction[] {
    const c = code.toLowerCase();
    if (kind === "auth") return [{ label: "Открыть настройки", run: deps.openSettings }];
    if (kind === "context" || c === "ai_no_consent" || c === "ai_disabled") return [];
    return [{ label: "Повторить", run: retry }];
  }

  function send(display: string, content: string): void {
    if (!state.settings?.enabled || state.busy) return;
    if (!state.settings.consent_document_send || state.consentDenied) {
      deps.consent.askConsent(() => send(display, content));
      return;
    }
    state.lastRequest = { display, content };
    dom.input.value = "";
    dom.input.style.height = "";
    deps.chat.appendUser(display);
    state.history = state.history.concat({ role: "user", content });
    const { row, bubble, col } = deps.chat.appendAssistant();
    const token = ++state.requestToken;
    state.pending = { token, row, bubble, col, content: "", messageId: null, usage: null, finishReason: null, finalized: false };
    deps.indicator.setBusy(true);
    state.stick = true;
    deps.indicator.scrollToBottom();
    void runChat(token);
  }

  async function runChat(token: number): Promise<void> {
    try {
      const res = await aiChat(state.history);
      if (state.disposed) return;
      const p = state.pending;
      if (!p || p.token !== token || p.finalized) return;
      if (!p.content) p.content = res.message.content;
      p.finishReason = res.finish_reason;
      complete(p, res.usage ?? p.usage);
    } catch (e) {
      if (state.disposed) return;
      const p = state.pending;
      if (!p || p.token !== token || p.finalized) return;
      fail(p, "", errorMessage(e));
    }
  }

  function complete(p: Pending, usage: AiUsage | null): void {
    p.finalized = true;
    state.pending = null;
    deps.indicator.setBusy(false);
    const isEmpty = p.content === "";
    const truncated = p.finishReason === "length";
    const content = isEmpty ? (truncated ? "" : "Пустой ответ") : p.content;
    p.bubble.textContent = content;
    if (content) void deps.chat.renderBubble(p.bubble, content);
    if (usage) deps.chat.appendUsage(p.col, usage);
    if (!isEmpty || !truncated) {
      state.history = state.history.concat({ role: "assistant", content });
    }
    if (!isEmpty || !truncated) {
      deps.plugin.appendAssistantActions(p.col, content);
    }
    if (truncated) {
      deps.chat.appendNotice(
        p.col,
        "Ответ оборван лимитом токенов — увеличьте max_tokens или повторите запрос.",
        "error"
      );
    }
    state.authError = false;
    state.lastError = false;
    state.log(
      "info",
      "system",
      `AI: ответ получен${usage ? ` (prompt ${usage.prompt_tokens}, completion ${usage.completion_tokens})` : ""}`
    );
    deps.indicator.applyState();
    deps.history.saveHistory();
    if (state.stick) deps.indicator.scrollToBottom();
  }

  function fail(p: Pending, code: string, message: string): void {
    p.finalized = true;
    state.pending = null;
    deps.indicator.setBusy(false);
    p.row.remove();
    const c = code.toLowerCase();
    const kind = errorKind(code, message);
    if (c === "ai_no_consent") {
      state.consentDenied = true;
      state.authError = false;
      state.lastError = false;
    } else if (kind === "auth") {
      state.authError = true;
      state.lastError = false;
    } else {
      state.lastError = true;
    }
    deps.chat.appendErrorCard(errorTitle(kind, c), message, errorActions(kind, c));
    state.log("error", "system", `AI: ${errorTitle(kind, c)} — ${message}`);
    deps.indicator.applyState();
    deps.history.saveHistory();
  }

  function retry(): void {
    if (state.busy || !state.settings?.enabled || !state.lastRequest) return;
    const { row, bubble, col } = deps.chat.appendAssistant();
    const token = ++state.requestToken;
    state.pending = { token, row, bubble, col, content: "", messageId: null, usage: null, finishReason: null, finalized: false };
    deps.indicator.setBusy(true);
    state.stick = true;
    deps.indicator.scrollToBottom();
    void runChat(token);
  }

  function submitInput(): void {
    const text = dom.input.value.trim();
    if (!text) return;
    send(text, text);
  }

  function clearChat(): void {
    state.requestToken++;
    if (state.pending) {
      state.pending.finalized = true;
      state.pending = null;
    }
    deps.indicator.setBusy(false);
    dom.chat.textContent = "";
    state.history = [];
    state.lastRequest = null;
    dom.chat.appendChild(state.emptyHint);
    deps.indicator.updateEmpty();
    deps.history.hideSessionHeader();
    void aiHistoryClear().catch((e) => {
      if (state.disposed) return;
      state.log("warn", "system", `AI: история не очищена (${errorMessage(e)})`);
    });
  }

  return { send, retry, submitInput, clearChat, complete, fail };
}