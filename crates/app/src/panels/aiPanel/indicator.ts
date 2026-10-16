







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

  

  function activeHasKey(): boolean {
    const settings = state.settings;
    if (!settings) return false;
    return (
      settings.providers.find((provider) => provider.id === settings.active_provider)?.settings
        .has_key ?? false
    );
  }

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
      !activeHasKey() ||
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
    if (state.settings && !activeHasKey()) return "нет ключа";
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

    const providers = state.settings?.providers ?? [];
    dom.providersSelect.disabled = !enabled;
    dom.modelSelect.disabled = !enabled;
    dom.providerAdd.disabled = !enabled;
    dom.providerEdit.disabled = !enabled;
    dom.providerRemove.disabled = !enabled || providers.length <= 1;
    dom.providerName.disabled = !enabled;
    dom.baseUrlInput.disabled = !enabled;
    dom.providerEnv.disabled = !enabled;
    dom.providerTimeout.disabled = !enabled;
    dom.providerHeaders.disabled = !enabled;
    dom.providerBody.disabled = !enabled;
    dom.providerModels.disabled = !enabled;
    dom.keyInput.disabled = !enabled;
    dom.keySave.disabled = !enabled;
    dom.keyClear.disabled = !enabled || !activeHasKey();
    dom.testBtn.disabled = !enabled;

    const active = providers.find((provider) => provider.id === state.settings?.active_provider);
    if (active && dom.modelLabel) {
      dom.modelLabel.textContent = `${active.name || active.id} · ${shortModel(
        state.settings?.active_model ?? ""
      )}`;
    }
    dom.keyHint.textContent = active?.settings.has_key ? "Ключ сохранён" : "Ключ не задан";
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
