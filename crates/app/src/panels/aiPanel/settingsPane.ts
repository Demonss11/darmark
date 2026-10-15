// Менеджер провайдеров LLM в поповере настроек: список/выбор активного провайдера и
// модели, добавление/удаление/правка провайдера, ключ на провайдера и проверка
// соединения. Все изменения идут через `persist`, который при необходимости
// перечитывает настройки с Rust-стороны (там секреты и сброс согласия).
//
// `persist` вызывается и из других кластеров (согласие, переключатель в панели),
// поэтому он выставлен в API наружу.

import {
  aiGetSettings,
  aiSetApiKey,
  aiSetSettings,
  aiTestConnection,
  errorMessage,
} from "../../core/tauri";
import type {
  AiSettingsInput,
  AiSettingsView,
  ModelInput,
  ProviderInput,
  ProviderView,
} from "../../core/tauri";
import type { IndicatorApi } from "./indicator";
import { el } from "./dom";
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
  selectProvider(id: string): Promise<void>;
  selectModel(id: string): Promise<void>;
  addProvider(): Promise<void>;
  removeProvider(): Promise<void>;
  applyProviderEdits(): Promise<void>;
  renderConfig(): void;
}

export function createSettingsPane(
  state: AiPanelState,
  deps: { ensureSkills(): void; indicator: IndicatorApi }
): SettingsApi {
  const { dom } = state;

  /** Проекция провайдера `View` → `Input` (секрет не переносится — его хранит Rust). */
  function providerViewToInput(provider: ProviderView): ProviderInput {
    return {
      id: provider.id,
      name: provider.name,
      env: [...provider.env],
      settings: {
        base_url: provider.settings.base_url,
        timeout: provider.settings.timeout,
      },
      headers: { ...provider.headers },
      body: { ...provider.body },
      models: provider.models.map((model) => ({
        id: model.id,
        name: model.name,
        model_id: model.model_id,
        disabled: model.disabled,
      })),
    };
  }

  /** Проекция настроек `View` → `Input`. */
  function viewToInput(view: AiSettingsView): AiSettingsInput {
    return {
      enabled: view.enabled,
      active_provider: view.active_provider,
      active_model: view.active_model,
      max_tokens: view.max_tokens,
      temperature: view.temperature,
      consent_document_send: view.consent_document_send,
      providers: view.providers.map(providerViewToInput),
    };
  }

  /** Активный провайдер из текущих настроек. */
  function activeProvider(): ProviderView | undefined {
    const settings = state.settings;
    if (!settings) return undefined;
    return settings.providers.find((provider) => provider.id === settings.active_provider);
  }

  function option(value: string, label: string): HTMLOptionElement {
    const node = el("option");
    node.value = value;
    node.textContent = label;
    return node;
  }

  /** Разбирает `headers`/`body`: пусто — `{}`; иначе объект JSON или `null` (ошибка в логе). */
  function parseObject(raw: string, label: string): Record<string, unknown> | null {
    const text = raw.trim();
    if (!text) return {};
    try {
      const value = JSON.parse(text) as unknown;
      if (value && typeof value === "object" && !Array.isArray(value)) {
        return value as Record<string, unknown>;
      }
    } catch {
      /* обработка ниже */
    }
    state.log("error", "system", `AI: ${label} — некорректный JSON`);
    return null;
  }

  /** Заголовки: значения приводим к строкам (Rust ждёт `map<string,string>`). */
  function headersFrom(raw: Record<string, unknown>): Record<string, string> {
    const headers: Record<string, string> = {};
    for (const [key, value] of Object.entries(raw)) {
      headers[key] = typeof value === "string" ? value : JSON.stringify(value);
    }
    return headers;
  }

  /** Разбирает модели: по строке `id` либо `id = Имя`; пусто — пустой список. */
  function parseModels(raw: string): ModelInput[] | null {
    const models: ModelInput[] = [];
    for (const line of raw.split("\n")) {
      const text = line.trim();
      if (!text) continue;
      const eq = text.indexOf("=");
      const id = (eq === -1 ? text : text.slice(0, eq)).trim();
      if (!id) {
        state.log("error", "system", "AI: у модели пустой id");
        return null;
      }
      const name = (eq === -1 ? id : text.slice(eq + 1)).trim() || id;
      models.push({ id, name, model_id: null, disabled: null });
    }
    return models;
  }

  /** Разбирает список env-переменных (запятые/пробелы/переводы строк). */
  function parseEnv(raw: string): string[] {
    return raw
      .split(/[\s,]+/)
      .map((name) => name.trim())
      .filter((name) => name.length > 0);
  }

  /** Разбирает `timeout` в мс: пусто — `null`; иначе неотрицательное число или `undefined`. */
  function parseTimeout(raw: string): number | null | undefined {
    const text = raw.trim();
    if (!text) return null;
    const value = Number(text);
    if (!Number.isFinite(value) || value < 0) {
      state.log("error", "system", "AI: таймаут должен быть неотрицательным числом");
      return undefined;
    }
    return Math.trunc(value);
  }

  /** Отрисовывает список провайдеров, выбор модели и поля активного провайдера. */
  function renderConfig(): void {
    const settings = state.settings;
    if (!settings) return;

    dom.providersSelect.replaceChildren(
      ...settings.providers.map((provider) => option(provider.id, provider.name || provider.id))
    );
    dom.providersSelect.value = settings.active_provider;

    const provider = activeProvider();
    const models = provider?.models ?? [];
    dom.modelSelect.replaceChildren(...models.map((model) => option(model.id, model.id)));
    if (models.some((model) => model.id === settings.active_model)) {
      dom.modelSelect.value = settings.active_model;
    }

    if (provider) {
      dom.providerName.value = provider.name;
      dom.baseUrlInput.value = provider.settings.base_url;
      dom.providerEnv.value = provider.env.join(", ");
      dom.providerTimeout.value =
        provider.settings.timeout != null ? String(provider.settings.timeout) : "";
      dom.providerHeaders.value = JSON.stringify(provider.headers ?? {});
      dom.providerBody.value = JSON.stringify(provider.body ?? {});
      dom.providerModels.value = provider.models
        .map((model) => (model.name && model.name !== model.id ? `${model.id} = ${model.name}` : model.id))
        .join("\n");
    }
    dom.providerRemove.disabled = (settings.enabled ?? false) === false || settings.providers.length <= 1;
  }

  /** Сохраняет готовый `Input` и перечитывает настройки с Rust-стороны. */
  async function persistInput(input: AiSettingsInput): Promise<void> {
    try {
      await aiSetSettings(input);
    } catch (e) {
      state.log("error", "system", `AI: настройки не сохранены (${errorMessage(e)})`);
      return;
    }
    await refresh();
  }

  /**
   * Очередь записей в IPC: persist/persistInput выполняются строго последовательно.
   * Без неё быстрые правки затирают друг друга (Дефект 2, PROVIDERS_REVIEW.md).
   */
  let writeChain: Promise<void> = Promise.resolve();

  function enqueueWrite(input: AiSettingsInput): Promise<void> {
    const run = writeChain.then(() => persistInput(input));
    writeChain = run.catch(() => undefined);
    return run;
  }

  /**
   * Применяет патч к настройкам. Input формируется внутри очереди — после предыдущей
   * записи (persistInput → refresh). Иначе патч может лечь в устаревшее
   * state.settings и затереть чужую правку (Дефект 2, PROVIDERS_REVIEW.md).
   */
  function persist(patch: Partial<AiSettingsInput>): Promise<void> {
    const run = writeChain.then(async () => {
      if (!state.settings) return;
      return persistInput({ ...viewToInput(state.settings), ...patch });
    });
    writeChain = run.catch(() => undefined);
    return run;
  }

  /** Перечитывает настройки и перерисовывает панель (секреты живут только на Rust-стороне). */
  async function refresh(): Promise<void> {
    try {
      state.settings = await aiGetSettings();
    } catch (e) {
      state.log("error", "system", `AI: настройки недоступны (${errorMessage(e)})`);
    }
    renderConfig();
    deps.indicator.applyState();
  }

  async function selectProvider(id: string): Promise<void> {
    const provider = state.settings?.providers.find((item) => item.id === id);
    const model = provider?.models[0]?.id ?? state.settings?.active_model ?? "";
    await persist({ active_provider: id, active_model: model });
  }

  async function selectModel(id: string): Promise<void> {
    await persist({ active_model: id });
  }

  async function addProvider(): Promise<void> {
    const settings = state.settings;
    if (!settings) return;
    let sequence = 1;
    while (settings.providers.some((provider) => provider.id === `provider-${sequence}`)) {
      sequence += 1;
    }
    const id = `provider-${sequence}`;
    const input = viewToInput(settings);
    input.providers.push({
      id,
      name: `Провайдер ${sequence}`,
      env: [],
      settings: { base_url: "https://api.example.com/v1", timeout: null },
      headers: {},
      body: {},
      models: [{ id: "model-1", name: "model-1", model_id: null, disabled: null }],
    });
    input.active_provider = id;
    input.active_model = "model-1";
    await enqueueWrite(input);
  }

  async function removeProvider(): Promise<void> {
    const settings = state.settings;
    if (!settings) return;
    if (settings.providers.length <= 1) {
      state.log("warn", "system", "AI: нельзя удалить единственного провайдера");
      return;
    }
    const input = viewToInput(settings);
    input.providers = input.providers.filter((provider) => provider.id !== input.active_provider);
    if (!input.providers.some((provider) => provider.id === input.active_provider)) {
      const first = input.providers[0];
      input.active_provider = first.id;
      input.active_model = first.models[0]?.id ?? "";
    }
    await enqueueWrite(input);
  }

  async function applyProviderEdits(): Promise<void> {
    const settings = state.settings;
    if (!settings) return;
    const id = settings.active_provider;
    const parsedHeaders = parseObject(dom.providerHeaders.value, "headers");
    const parsedBody = parseObject(dom.providerBody.value, "body");
    const models = parseModels(dom.providerModels.value);
    const timeout = parseTimeout(dom.providerTimeout.value);
    if (parsedHeaders === null || parsedBody === null || models === null || timeout === undefined) {
      return;
    }
    const input = viewToInput(settings);
    const index = input.providers.findIndex((provider) => provider.id === id);
    if (index === -1) return;
    input.providers[index] = {
      id,
      name: dom.providerName.value.trim() || id,
      env: parseEnv(dom.providerEnv.value),
      settings: { base_url: dom.baseUrlInput.value.trim(), timeout },
      headers: headersFrom(parsedHeaders),
      body: parsedBody,
      models,
    };
    if (!models.some((model) => model.id === input.active_model)) {
      input.active_model = models[0]?.id ?? input.active_model;
    }
    await enqueueWrite(input);
  }

  async function saveKey(): Promise<void> {
    const providerId = state.settings?.active_provider;
    if (!providerId) return;
    const key = dom.keyInput.value.trim();
    try {
      await aiSetApiKey(providerId, key);
      dom.keyInput.value = "";
      state.authError = false;
      state.lastError = false;
      await refresh();
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
        if (activeProvider()?.settings.has_key) state.lastError = true;
        dom.testResult.textContent = res.error ? `Ошибка: ${res.error}` : "Не удалось соединиться";
        state.log(
          "warn",
          "system",
          `AI: проверка соединения не удалась${res.error ? ` (${res.error})` : ""}`
        );
      }
      deps.indicator.applyState();
    } catch (e) {
      if (activeProvider()?.settings.has_key) state.lastError = true;
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
        renderConfig();
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

  state.renderConfig = renderConfig;

  return {
    persist,
    saveKey,
    testConnection,
    openSettings,
    closeSettings,
    toggleSettings,
    enableFromStub,
    loadSettings,
    selectProvider,
    selectModel,
    addProvider,
    removeProvider,
    applyProviderEdits,
    renderConfig,
  };
}
