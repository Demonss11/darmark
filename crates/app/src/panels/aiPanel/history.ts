





import { aiHistoryLoad, aiHistorySave, errorMessage } from "../../core/tauri";
import type { ChatApi } from "./chat";
import type { PluginCardApi } from "./pluginCard";
import type { AiPanelState } from "./state";

export interface HistoryApi {
  restoreHistory(): Promise<void>;
  saveHistory(): void;
  hideSessionHeader(): void;
}

export function createHistory(
  state: AiPanelState,
  deps: {
    chat: ChatApi;
    plugin: PluginCardApi;
    scrollToBottom(): void;
  }
): HistoryApi {
  const { dom } = state;

  function formatSessionDate(unixSecs: number): string {
    const d = new Date(unixSecs * 1000);
    const p = (n: number): string => String(n).padStart(2, "0");
    return `${p(d.getDate())}.${p(d.getMonth() + 1)}, ${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  function showSessionHeader(updatedAt: number): void {
    if (!dom.sessionHeader) return;
    dom.sessionHeader.textContent = `сессия от ${formatSessionDate(updatedAt)}`;
    dom.sessionHeader.hidden = false;
  }

  function hideSessionHeader(): void {
    if (!dom.sessionHeader) return;
    dom.sessionHeader.hidden = true;
    dom.sessionHeader.textContent = "";
  }

  async function restoreHistory(): Promise<void> {
    try {
      const view = await aiHistoryLoad();
      if (state.disposed) return;
      if (view.messages.length === 0) {
        hideSessionHeader();
        return;
      }
      for (const msg of view.messages) {
        if (msg.role === "user") {
          deps.chat.appendUser(msg.content);
          state.history = state.history.concat({ role: "user", content: msg.content });
        } else if (msg.role === "assistant") {
          const { bubble, col } = deps.chat.appendAssistant();
          state.history = state.history.concat({ role: "assistant", content: msg.content });
          if (msg.content) void deps.chat.renderBubble(bubble, msg.content);
          deps.plugin.appendAssistantActions(col, msg.content);
        }
      }
      showSessionHeader(view.updated_at);
      if (view.warning) {
        state.log("warn", "system", `AI: ${view.warning}`);
      }
      if (state.stick) deps.scrollToBottom();
    } catch (e) {
      if (state.disposed) return;
      state.log("warn", "system", `AI: история недоступна (${errorMessage(e)})`);
    }
  }

  function saveHistory(): void {
    void aiHistorySave(state.history)
      .then(() => {
        if (state.disposed) return;
        if (state.history.length > 0) {
          showSessionHeader(Math.floor(Date.now() / 1000));
        } else {
          hideSessionHeader();
        }
      })
      .catch((e) => {
        if (state.disposed) return;
        state.log("warn", "system", `AI: история не сохранена (${errorMessage(e)})`);
      });
  }

  return { restoreHistory, saveHistory, hideSessionHeader };
}