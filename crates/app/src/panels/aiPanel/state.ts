// Общее изменяемое состояние панели AI-ассистента: DOM-ссылки, настройки,
// флаги и текущий запрос. Единственный владелец всех полей — этот объект
// (ADR-0030, ADR-0034).
//
// Зачем отдельный модуль: остальные кластеры (индикатор, навыки, чат, стриминг,
// настройки) читают и пишут одни и те же поля (`settings`, `busy`, `stick`,
// `pending`, …). Если бы каждое замыкание жило внутри `createAiPanel`, разбиение
// на модули породило бы либо циклические импорты, либо копии состояния. Здесь
// состояние собрано явно и передаётся фабрикам параметром.

import type { UnlistenFn } from "@tauri-apps/api/event";
import type { DocumentId } from "../../core/ids";
import type { AiMessage, AiSettingsView, AiSkill, AiUsage } from "../../core/tauri";
import type { LogCategory, LogLevel } from "../logPanel";
import { byId, must } from "./dom";
import type { PluginCardState } from "./pluginProposal";

export type AiLog = (
  level: LogLevel,
  category: LogCategory,
  message: string,
  detail?: string
) => void;

/** Активный документ — нужен карточкам правки и плагина (aiDiff/aiApplyEdit). */
type ActiveDocument = { id: DocumentId; rev: number };

/** DOM-узлы панели: собираются один раз при монтировании и не пересоздаются. */
interface AiDom {
  offStub: HTMLElement;
  enableBtn: HTMLButtonElement;
  skillsRow: HTMLElement;
  chat: HTMLElement;
  input: HTMLTextAreaElement;
  sendBtn: HTMLButtonElement;
  clearBtn: HTMLButtonElement;
  dot: HTMLElement;
  statusDot: HTMLElement | null;
  statusText: HTMLElement;
  settingsBtn: HTMLButtonElement;
  sessionHeader: HTMLElement | null;
  popover: HTMLElement;
  settingsClose: HTMLButtonElement;
  enabledInput: HTMLInputElement;
  enabledSub: HTMLElement;
  config: HTMLElement;
  providerInput: HTMLSelectElement;
  baseUrlInput: HTMLInputElement;
  modelInput: HTMLInputElement;
  keyInput: HTMLInputElement;
  keySave: HTMLButtonElement;
  keyClear: HTMLButtonElement;
  keyHint: HTMLElement;
  testBtn: HTMLButtonElement;
  testResult: HTMLElement;
  modelLabel: HTMLElement | null;
  inputRow: HTMLElement;
}

/** Незавершённый запрос: пузырь ассистента и накопленный текст ответа. */
export interface Pending {
  token: number;
  row: HTMLElement;
  bubble: HTMLElement;
  col: HTMLElement;
  content: string;
  messageId: string | null;
  usage: AiUsage | null;
  finishReason: string | null;
  finalized: boolean;
}

export interface AiPanelState {
  log: AiLog;
  getDocumentText(): string;
  getActiveDocument(): ActiveDocument | null;
  dom: AiDom;
  settings: AiSettingsView | null;
  skills: readonly AiSkill[];
  skillsLoaded: boolean;
  skillsLoading: boolean;
  consentDenied: boolean;
  authError: boolean;
  lastError: boolean;
  busy: boolean;
  stick: boolean;
  popoverOpen: boolean;
  wasAiExpanded: boolean;
  history: AiMessage[];
  lastRequest: { display: string; content: string } | null;
  disposed: boolean;
  pending: Pending | null;
  requestToken: number;
  cardSeq: number;
  pluginCards: Set<PluginCardState>;
  emptyHint: HTMLElement;
  unlisteners: UnlistenFn[];
}

export interface AiStateOptions {
  root: HTMLElement;
  log: AiLog;
  getDocumentText(): string;
  getActiveDocument(): ActiveDocument | null;
}

/**
 * Собирает DOM-ссылки и начальное состояние панели. Единственное место, где
 * создаётся `emptyHint` — он должен существовать до первого `updateEmpty()`.
 */
export function createAiState(opts: AiStateOptions): AiPanelState {
  const { root } = opts;
  const input = byId<HTMLTextAreaElement>("ai-input");

  const dom: AiDom = {
    offStub: byId<HTMLElement>("ai-off"),
    enableBtn: byId<HTMLButtonElement>("ai-enable"),
    skillsRow: byId<HTMLElement>("ai-skills"),
    chat: byId<HTMLElement>("ai-chat"),
    input,
    sendBtn: byId<HTMLButtonElement>("ai-send"),
    clearBtn: byId<HTMLButtonElement>("ai-clear"),
    dot: byId<HTMLElement>("ai-dot"),
    statusDot: document.getElementById("ai-status-dot"),
    statusText: byId<HTMLElement>("ai-status-text"),
    settingsBtn: byId<HTMLButtonElement>("ai-settings"),
    sessionHeader: document.getElementById("ai-session"),
    popover: byId<HTMLElement>("ai-settings-pop"),
    settingsClose: byId<HTMLButtonElement>("ai-settings-close"),
    enabledInput: byId<HTMLInputElement>("ai-enabled"),
    enabledSub: byId<HTMLElement>("ai-enabled-sub"),
    config: byId<HTMLElement>("ai-config"),
    providerInput: byId<HTMLSelectElement>("ai-provider"),
    baseUrlInput: byId<HTMLInputElement>("ai-base-url"),
    modelInput: byId<HTMLInputElement>("ai-model"),
    keyInput: byId<HTMLInputElement>("ai-key"),
    keySave: byId<HTMLButtonElement>("ai-key-save"),
    keyClear: byId<HTMLButtonElement>("ai-key-clear"),
    keyHint: byId<HTMLElement>("ai-key-hint"),
    testBtn: byId<HTMLButtonElement>("ai-test"),
    testResult: byId<HTMLElement>("ai-test-result"),
    modelLabel:
      root.closest<HTMLElement>("#logs-panel")?.querySelector<HTMLElement>(".ai-model") ?? null,
    inputRow: must(input.closest<HTMLElement>(".ai-input"), "контейнер .ai-input"),
  };

  const state: AiPanelState = {
    log: opts.log,
    getDocumentText: opts.getDocumentText,
    getActiveDocument: opts.getActiveDocument,
    dom,
    settings: null,
    skills: [],
    skillsLoaded: false,
    skillsLoading: false,
    consentDenied: false,
    authError: false,
    lastError: false,
    busy: false,
    stick: true,
    popoverOpen: false,
    wasAiExpanded: false,
    history: [],
    lastRequest: null,
    disposed: false,
    pending: null,
    requestToken: 0,
    cardSeq: 0,
    pluginCards: new Set<PluginCardState>(),
    emptyHint: document.createElement("div"),
    unlisteners: [],
  };
  state.emptyHint.className = "ai-chat-empty";
  state.emptyHint.setAttribute("aria-hidden", "true");
  state.emptyHint.textContent = "Выберите скилл выше или напишите запрос.";
  dom.chat.appendChild(state.emptyHint);

  return state;
}