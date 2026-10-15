// Поповер настроек: включение AI, провайдер/модель, API-ключ и проверка
// соединения. Все изменения идут через `persist`, который после успешного
// `aiSetSettings` обновляет общее состояние и перерисовывает панель.
//
// `persist` вызывается и из других кластеров (согласие, переключатель в
// панели), поэтому он выставлен в API наружу.

import {
  aiGetSettings,
  aiSetApiKey,
  aiSetSettings,
  aiTestConnection,
  errorMessage,
} from "../../core/tauri";
import type { AiSettingsInput, AiSettingsView } from "../../core/tauri";
import type { IndicatorApi } from "./indicator";
import type { AiPanelState } from "./state";

export interface SettingsApi {
  persist(patch: Partial<AiSettingsInput>): Promise<void>;
  saveKey(): Promise<void>;
  testConnection(): Promise<void>;
  openSettings(): void;
  closeSettings(): void;
  toggleSettings(): void;
  enableFromStub(): Promise<void>;
  loadSettings(): void;
}

export function createSettingsPane(
  state: AiPanelState,
  deps: { ensureSkills(): void; indicator: IndicatorApi }
): SettingsApi {
  const { dom } = state;

  function toInput(s: AiSettingsView): AiSettingsInput {
    return {
      enabled: s.enabled,
      provider: s.provider,
      base_url: s.base_url,
      model: s.model,
      max_tokens: s.max_tokens,
      temperature: s.temperature,
      consent_document_send: s.consent_document_send,
    };
  }

  function mergeSettings(view: AiSettingsView, input: AiSettingsInput): AiSettingsView {
    return { ...input, has_key: view.has_key };
  }

  async function persist(patch: Partial<AiSettingsInput>): Promise<void> {
    if (!state.settings) return;
    const input: AiSettingsInput = { ...toInput(state.settings), ...patch };
    try {
      await aiSetSettings(input);
    } catch (e) {
      state.log("error", "system", `AI: настройки не сохранены (${errorMessage(e)})`);
      return;
    }
    state.settings = mergeSettings(state.settings, input);
    deps.indicator.applyState();
  }

  async function saveKey(): Promise<void> {
    const key = dom.keyInput.value.trim();
    try {
      await aiSetApiKey(key);
      dom.keyInput.value = "";
      state.settings = await aiGetSettings();
      state.authError = false;
      state.lastError = false;
      deps.indicator.applyState();
      state.log("info", "system", key ? "AI: API-ключ сохранён" : "AI: API-ключ удалён");
    } catch (e) {
      state.log("error", "system", `AI: ключ не сохранён (${errorMessage(e)})`);
    }
  }

  async function testConnection(): Promise<void> {
    dom.testBtn.disabled = true;
    dom.testResult.textContent = "Проверка соединения…";
    try {
      const res = await aiTestConnection();
      if (res.ok) {
        state.authError = false;
        state.lastError = false;
        dom.testResult.textContent = res.model
          ? `Соединение установлено · ${res.model}`
          : "Соединение установлено";
        state.log("info", "system", "AI: соединение установлено");
      } else {
        if (state.settings?.has_key) state.lastError = true;
        dom.testResult.textContent = res.error ? `Ошибка: ${res.error}` : "Не удалось соединиться";
        state.log(
          "warn",
          "system",
          `AI: проверка соединения не удалась${res.error ? ` (${res.error})` : ""}`
        );
      }
      deps.indicator.applyState();
    } catch (e) {
      if (state.settings?.has_key) state.lastError = true;
      dom.testResult.textContent = `Ошибка: ${errorMessage(e)}`;
      deps.indicator.applyState();
    } finally {
      dom.testBtn.disabled = !(state.settings?.enabled ?? false);
    }
  }

  function openSettings(): void {
    dom.popover.hidden = false;
    state.popoverOpen = true;
  }

  function closeSettings(): void {
    dom.popover.hidden = true;
    state.popoverOpen = false;
  }

  function toggleSettings(): void {
    if (state.popoverOpen) closeSettings();
    else openSettings();
  }

  async function enableFromStub(): Promise<void> {
    await persist({ enabled: true });
    if (state.settings?.enabled) {
      state.lastError = false;
      deps.ensureSkills();
      deps.indicator.applyState();
      openSettings();
      dom.input.focus();
    }
  }

  /** Стартовая загрузка настроек: до неё панель считается выключенной. */
  function loadSettings(): void {
    void aiGetSettings()
      .then((view) => {
        if (state.disposed) return;
        state.settings = view;
        deps.indicator.applyState();
        if (view.enabled) deps.ensureSkills();
      })
      .catch((e) => {
        if (state.disposed) return;
        state.settings = null;
        state.log("error", "system", `AI: настройки недоступны (${errorMessage(e)})`);
        deps.indicator.applyState();
      });
  }

  return {
    persist,
    saveKey,
    testConnection,
    openSettings,
    closeSettings,
    toggleSettings,
    enableFromStub,
    loadSettings,
  };
}