// Индикатор состояния AI: точка в логе, текст статуса, доступность элементов
// панели и «липкая» прокрутка чата.
//
// Зачем отдельный модуль: `applyState` — единственный читатель всех флагов
// (`settings`, `authError`, `lastError`, `consentDenied`) для отрисовки, и его
// вызывают почти все остальные кластеры. Держим его в фабрике без
// зависимостей от остальных, чтобы не было цикла «состояние → панель → состояние».

import type { AiDotState } from "./constants";
import { shortModel } from "./dom";
import type { AiPanelState } from "./state";

export interface IndicatorApi {
  applyState(): void;
  updateEmpty(): void;
  scrollToBottom(): void;
  setBusy(next: boolean): void;
}

export function createIndicator(state: AiPanelState): IndicatorApi {
  const { dom } = state;

  function paintDot(el: HTMLElement | null, st: AiDotState): void {
    if (!el) return;
    for (const s of ["off", "warn", "ready", "error"] as const) {
      el.classList.toggle(`state-${s}`, s === st);
    }
  }

  function currentDotState(): AiDotState {
    if (!state.settings || !state.settings.enabled) return "off";
    if (state.lastError) return "error";
    if (
      state.authError ||
      !state.settings.has_key ||
      !state.settings.consent_document_send ||
      state.consentDenied
    ) {
      return "warn";
    }
    return "ready";
  }

  function stateLabel(st: AiDotState): string {
    if (st === "off") return "выключен";
    if (st === "error") return "ошибка · нет соединения";
    if (st === "ready") return "доступен · встроен";
    if (state.authError) return "неверный ключ";
    if (state.settings && !state.settings.has_key) return "нет ключа";
    return "нет согласия на отправку";
  }

  function applyState(): void {
    const st = currentDotState();
    const label = stateLabel(st);
    paintDot(dom.dot, st);
    paintDot(dom.statusDot, st);
    dom.dot.title = `AI: ${label}`;
    dom.statusText.textContent = label;

    const enabled = state.settings?.enabled ?? false;
    dom.offStub.hidden = enabled;
    dom.skillsRow.hidden = !enabled;
    dom.chat.hidden = !enabled;
    dom.inputRow.hidden = !enabled;
    if (!enabled && dom.sessionHeader) dom.sessionHeader.hidden = true;
    dom.input.disabled = !enabled;
    dom.sendBtn.disabled = !enabled || state.busy;

    dom.enabledInput.checked = enabled;
    dom.enabledSub.textContent = enabled ? "включён" : "выключен";
    dom.config.classList.toggle("is-disabled", !enabled);
    dom.providerInput.disabled = !enabled;
    dom.modelInput.disabled = !enabled;
    dom.keyInput.disabled = !enabled;
    dom.keySave.disabled = !enabled;
    dom.keyClear.disabled = !enabled || !(state.settings?.has_key ?? false);
    dom.testBtn.disabled = !enabled;

    if (state.settings) {
      if (document.activeElement !== dom.providerInput) dom.providerInput.value = state.settings.provider;
      dom.baseUrlInput.value = state.settings.base_url;
      if (document.activeElement !== dom.modelInput) dom.modelInput.value = state.settings.model;
      if (dom.modelLabel) dom.modelLabel.textContent = `routerai · ${shortModel(state.settings.model)}`;
    }
    dom.keyHint.textContent = state.settings?.has_key ? "Ключ сохранён" : "Ключ не задан";
  }

  function updateEmpty(): void {
    state.emptyHint.hidden = dom.chat.querySelector(".msg") !== null;
  }

  function scrollToBottom(): void {
    dom.chat.scrollTop = dom.chat.scrollHeight;
  }

  function setBusy(next: boolean): void {
    state.busy = next;
    dom.sendBtn.disabled = !(state.settings?.enabled ?? false) || state.busy;
    dom.dot.classList.toggle("busy", state.busy);
  }

  return { applyState, updateEmpty, scrollToBottom, setBusy };
}