// aiPanel/index.ts — тело режима «AI-ассистент» нижнего дока (ADR-0030; сеть и
// провайдер — ADR-0032/0033; рантайм — ADR-0034). Модуль-оболочка (как
// logPanel/resourceMonitor), НЕ view и НЕ pane: монтируется logPanel'ом в
// контейнер #logs-panel (режим ai).
//
// Инварианты: сетевые вызовы — только на Rust-стороне (CSP `default-src 'self'`).
// Рендер ответа ассистента — только через `safeHtml.setSanitizedHtml` (ADR-0038
// §2.1/§2.2): HTML ответа модели проходит Rust-санитайзер `md-core`, прямых
// присваиваний `innerHTML` в пакете нет. Всё, что не проходит этот хелпер —
// стриминг, код плагина, дифф, ошибки — только `textContent`.
// История чата — в памяти на сессию (без персист.) и хранит ТЕКСТ ответа, не HTML.
// Стриминг: события ai:chunk/ai:done/ai:error дописывают/финализируют текущий
// пузырь ассистента; отписки сохраняются и снимаются.
//
// Фаза 2 (harness, ADR-0034): под ответом ассистента — «Применить как правку»
// (дифф через ai_apply_edit + undo_document) и карточка плагина из ```json-блока
// (через ai_*_plugin).
//
// Этот файл — композитор: собирает фабрики кластеров, держит ВСЕ подписки
// (DOM на элементах панели и на `document`, Tauri-события) и отвечает за
// `dispose`. Логика самих кластеров живёт рядом: state.ts (общее состояние и
// DOM-ссылки), indicator.ts, skillBar.ts, chat.ts, diffCard.ts, pluginCard.ts,
// history.ts, settingsPane.ts, consent.ts, stream.ts.

import { listen, type Event } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { DocumentId } from "../../core/ids";
import { errorMessage } from "../../core/tauri";
import type { AiToolCallEvent, AiToolResultEvent, AiUsage } from "../../core/tauri";
import { isModalOpen } from "../../ui/dialog";
import type { DockMode, DockState } from "../logPanel";
import { createChat } from "./chat";
import { createConsent } from "./consent";
import { STICK_THRESHOLD } from "./constants";
import { createDiffCard } from "./diffCard";
import { createHistory } from "./history";
import { createIndicator } from "./indicator";
import { createPluginCard } from "./pluginCard";
import { createSettingsPane } from "./settingsPane";
import { createSkillBar } from "./skillBar";
import { createAiState, type AiLog } from "./state";
import { createStream } from "./stream";
import { createToolCard } from "./toolCard";

interface AiPanelOptions {
  root: HTMLElement;
  log: AiLog;
  getDocumentText(): string;
  getActiveDocument(): { id: DocumentId; rev: number } | null;
}

export interface AiPanel {
  onDockState(mode: DockMode, state: DockState): void;
  dispose(): void;
}

export function createAiPanel(opts: AiPanelOptions): AiPanel {
  const state = createAiState(opts);
  const { dom } = state;

  // Кластеры создаются снизу вверх по зависимостям; каждый получает соседние
  // API параметром — так в пакете нет циклических импортов (см. комментарий
  // в skillBar.ts про единственную «мягкую» ссылку на stream).
  const indicator = createIndicator(state);
  const chat = createChat(state, indicator);
  const diffCard = createDiffCard(state, {
    appendNotice: chat.appendNotice,
    scrollToBottom: indicator.scrollToBottom,
  });
  const plugin = createPluginCard(state, {
    requestDiff: diffCard.requestDiff,
    scrollToBottom: indicator.scrollToBottom,
  });
  const toolCard = createToolCard(state, { scrollToBottom: indicator.scrollToBottom });
  const history = createHistory(state, { chat, plugin, scrollToBottom: indicator.scrollToBottom });
  const skillBar = createSkillBar(state, { send: (display, content) => stream.send(display, content) });
  const settingsPane = createSettingsPane(state, {
    ensureSkills: () => skillBar.ensureSkills(),
    indicator,
  });
  const consent = createConsent(state, { persist: settingsPane.persist, indicator });
  const stream = createStream(state, {
    chat,
    indicator,
    history,
    plugin,
    consent,
    openSettings: settingsPane.openSettings,
  });

  const onInputKeyDown = (e: KeyboardEvent): void => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      stream.submitInput();
    }
  };
  const onInput = (): void => {
    dom.input.style.height = "auto";
    dom.input.style.height = `${Math.min(dom.input.scrollHeight, 76)}px`;
  };
  const onSendClick = (): void => stream.submitInput();
  const onChatScroll = (): void => {
    state.stick = dom.chat.scrollHeight - dom.chat.scrollTop - dom.chat.clientHeight <= STICK_THRESHOLD;
  };
  const onClearClick = (): void => stream.clearChat();
  const onChatClick = (e: MouseEvent): void => {
    const anchor = e.target instanceof Element ? e.target.closest("a") : null;
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    if (!/^https?:/i.test(href)) {
      e.preventDefault();
      return;
    }
    e.preventDefault();
    void openUrl(href).catch((err) => {
      state.log("warn", "system", `AI: ссылка не открыта — ${errorMessage(err)}`);
    });
  };

  const onSettingsClick = (): void => settingsPane.toggleSettings();
  const onSettingsClose = (): void => settingsPane.closeSettings();
  const onKeySave = (): void => void settingsPane.saveKey();
  const onKeyClear = (): void => {
    dom.keyInput.value = "";
    void settingsPane.saveKey();
  };
  const onTest = (): void => void settingsPane.testConnection();
  const onEnableClick = (): void => void settingsPane.enableFromStub();
  const onEnabledChange = (): void => {
    const on = dom.enabledInput.checked;
    void settingsPane.persist({ enabled: on }).then(() => {
      if (on) {
        state.lastError = false;
        skillBar.ensureSkills();
      } else {
        settingsPane.closeSettings();
      }
      indicator.applyState();
    });
  };
  const onProviderSelect = (): void => void settingsPane.selectProvider(dom.providersSelect.value);
  const onModelSelect = (): void => void settingsPane.selectModel(dom.modelSelect.value);
  const onProviderAdd = (): void => void settingsPane.addProvider();
  const onProviderRemove = (): void => void settingsPane.removeProvider();
  const onProviderEdit = (): void => void settingsPane.applyProviderEdits();
  const onDocPointerDown = (e: PointerEvent): void => {
    if (!state.popoverOpen) return;
    const t = e.target;
    if (t instanceof Node && (dom.popover.contains(t) || dom.settingsBtn.contains(t))) return;
    settingsPane.closeSettings();
  };
  const onDocKeyDown = (e: KeyboardEvent): void => {
    if (e.key !== "Escape" || !state.popoverOpen || isModalOpen()) return;
    e.preventDefault();
    e.stopPropagation();
    settingsPane.closeSettings();
  };

  dom.input.addEventListener("keydown", onInputKeyDown);
  dom.input.addEventListener("input", onInput);
  dom.sendBtn.addEventListener("click", onSendClick);
  dom.chat.addEventListener("scroll", onChatScroll);
  dom.chat.addEventListener("click", onChatClick);
  dom.clearBtn.addEventListener("click", onClearClick);
  dom.settingsBtn.addEventListener("click", onSettingsClick);
  dom.settingsClose.addEventListener("click", onSettingsClose);
  dom.keySave.addEventListener("click", onKeySave);
  dom.keyClear.addEventListener("click", onKeyClear);
  dom.testBtn.addEventListener("click", onTest);
  dom.enableBtn.addEventListener("click", onEnableClick);
  dom.enabledInput.addEventListener("change", onEnabledChange);
  dom.providersSelect.addEventListener("change", onProviderSelect);
  dom.modelSelect.addEventListener("change", onModelSelect);
  dom.providerAdd.addEventListener("click", onProviderAdd);
  dom.providerRemove.addEventListener("click", onProviderRemove);
  dom.providerEdit.addEventListener("click", onProviderEdit);
  document.addEventListener("pointerdown", onDocPointerDown);
  document.addEventListener("keydown", onDocKeyDown, true);

  function subscribe<T>(event: string, handler: (e: Event<T>) => void): void {
    void listen<T>(event, handler)
      .then((un) => {
        if (state.disposed) un();
        else state.unlisteners.push(un);
      })
      .catch((err) => console.error("listen:", event, err));
  }

  subscribe<{ message_id: string; delta: string }>("ai:chunk", (event) => {
    const p = state.pending;
    if (!p) return;
    const { message_id, delta } = event.payload;
    if (p.messageId !== null && p.messageId !== message_id) return;
    p.messageId = message_id;
    p.content += delta;
    p.bubble.textContent = p.content;
    indicator.updateEmpty();
    if (state.stick) indicator.scrollToBottom();
  });

  subscribe<{ message_id: string; usage: AiUsage | null; finish_reason: string | null }>(
    "ai:done",
    (event) => {
      const p = state.pending;
      if (!p) return;
      const { message_id, usage, finish_reason } = event.payload;
      if (p.messageId !== null && p.messageId !== message_id) return;
      p.messageId = message_id;
      p.usage = usage;
      p.finishReason = finish_reason ?? null;
      if (p.content) stream.complete(p, usage);
    }
  );

  subscribe<{ code: string; message: string }>("ai:error", (event) => {
    const p = state.pending;
    if (!p) return;
    stream.fail(p, event.payload.code, event.payload.message);
  });

  subscribe<AiToolCallEvent>("ai:tool-call", (event) => {
    const p = state.pending;
    if (!p) return;
    const { message_id, call_id, tool } = event.payload;
    if (p.messageId !== null && p.messageId !== message_id) return;
    p.messageId = message_id;
    toolCard.appendCall(p.col, call_id, tool);
  });

  subscribe<AiToolResultEvent>("ai:tool-result", (event) => {
    const p = state.pending;
    if (!p) return;
    const { message_id, call_id, ok, duration_ms, summary } = event.payload;
    if (p.messageId !== null && p.messageId !== message_id) return;
    p.messageId = message_id;
    toolCard.finishCall(p.col, call_id, ok, duration_ms, summary);
  });

  subscribe<{ plugin_id: string }>("ai:plugin_created", (event) => {
    for (const card of state.pluginCards) {
      if (card.id === event.payload.plugin_id && !card.isInstalled()) {
        card.installed("установлен", "active");
      }
    }
  });

  subscribe<{ plugin_id: string }>("ai:plugin_updated", (event) => {
    for (const card of state.pluginCards) {
      if (card.id === event.payload.plugin_id && !card.isInstalled()) {
        card.installed("обновлён", "active");
      }
    }
  });

  subscribe<{ plugin_id: string }>("ai:plugin_deleted", (event) => {
    for (const card of state.pluginCards) {
      if (card.id === event.payload.plugin_id) card.deleted();
    }
  });

  function onDockState(mode: DockMode, dockState: DockState): void {
    const active = mode === "ai" && dockState === "expanded";
    if (active && !state.wasAiExpanded && state.settings?.enabled) {
      skillBar.ensureSkills();
      dom.input.focus();
    }
    if (!active) settingsPane.closeSettings();
    state.wasAiExpanded = active;
  }

  function dispose(): void {
    state.disposed = true;
    for (const un of state.unlisteners) un();
    state.unlisteners.length = 0;
    dom.input.removeEventListener("keydown", onInputKeyDown);
    dom.input.removeEventListener("input", onInput);
    dom.sendBtn.removeEventListener("click", onSendClick);
    dom.chat.removeEventListener("scroll", onChatScroll);
    dom.chat.removeEventListener("click", onChatClick);
    dom.clearBtn.removeEventListener("click", onClearClick);
    dom.settingsBtn.removeEventListener("click", onSettingsClick);
    dom.settingsClose.removeEventListener("click", onSettingsClose);
    dom.keySave.removeEventListener("click", onKeySave);
    dom.keyClear.removeEventListener("click", onKeyClear);
    dom.testBtn.removeEventListener("click", onTest);
    dom.enableBtn.removeEventListener("click", onEnableClick);
    dom.enabledInput.removeEventListener("change", onEnabledChange);
    dom.providersSelect.removeEventListener("change", onProviderSelect);
    dom.modelSelect.removeEventListener("change", onModelSelect);
    dom.providerAdd.removeEventListener("click", onProviderAdd);
    dom.providerRemove.removeEventListener("click", onProviderRemove);
    dom.providerEdit.removeEventListener("click", onProviderEdit);
    document.removeEventListener("pointerdown", onDocPointerDown);
    document.removeEventListener("keydown", onDocKeyDown, true);
  }

  settingsPane.loadSettings();
  void history.restoreHistory().then(() => {
    if (state.disposed) return;
    indicator.applyState();
  });

  skillBar.renderSkills();
  indicator.applyState();
  indicator.updateEmpty();

  return { onDockState, dispose };
}