// aiPanel.ts — тело режима «AI-ассистент» нижнего дока (TZ-AI-ASSISTANT §8;
// ADR-0030/0032/0034). Модуль-оболочка (как logPanel/resourceMonitor), НЕ view
// и НЕ pane: монтируется logPanel'ом в контейнер #logs-panel (режим ai).
//
// Инварианты: сетевые вызовы — только на Rust-стороне (CSP `default-src 'self'`);
// ответ модели рендерится ИСКЛЮЧИТЕЛЬНО через textContent (MVP; сырой HTML/ответ
// LLM в innerHTML запрещён). История чата — в памяти на сессию (без персист.).
// Стриминг: события ai:chunk/ai:done/ai:error дописывают/финализируют текущий
// пузырь ассистента; отписки сохраняются и снимаются (без утечек).

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { openDialog, isModalOpen, type Dialog } from "./dialog";
import {
  aiChat,
  aiGetSettings,
  aiListSkills,
  aiSetApiKey,
  aiSetSettings,
  aiTestConnection,
  errorMessage,
} from "./tauri";
import type { AiMessage, AiSettingsInput, AiSettingsView, AiSkill, AiUsage } from "./tauri";
import type { DockMode, DockState, LogCategory, LogLevel } from "./logPanel";

/** Лимит текста документа за запрос (§4.1, §10.4). */
const MAX_DOC_BYTES = 100 * 1024;

/** Допуск «чат прижат к низу», px — автопрокрутка с заморозкой. */
const STICK_THRESHOLD = 24;

/** Состояние индикатора #ai-dot (§8.6). */
type AiDotState = "off" | "warn" | "ready" | "error";

/** Классификация ошибки для карточки и действий. */
type ErrorKind = "auth" | "offline" | "rate" | "context" | "generic";

/** Действие в карточке ошибки. */
interface ErrorAction {
  label: string;
  run(): void;
}

/** Встроенные скиллы-фолбэк, если `ai_list_skills` недоступен (TZ §7.2). */
const FALLBACK_SKILLS: readonly AiSkill[] = [
  {
    id: "explain",
    title: "Объяснить файл",
    description: "Разобрать и объяснить текущий документ",
    prompt_template:
      "Объясни этот документ простыми словами: о чём он, какова структура, что делает разметка. Документ:\n{{doc}}",
    category: "document",
  },
  {
    id: "create-plugin",
    title: "Создать плагин",
    description: "Сгенерировать новый Lua-плагин по описанию",
    prompt_template:
      "Сгенерируй Lua-плагин для darmark по описанию: {{input}}. Верни main.lua и plugin.json (host-API и permissions — по песочнице darmark).",
    category: "plugins",
  },
];

const ERROR_TITLES: Record<ErrorKind, string> = {
  auth: "Неверный API-ключ",
  offline: "Нет соединения",
  rate: "Превышен лимит запросов",
  context: "Документ слишком большой",
  generic: "Ошибка AI",
};

/** Заголовок карточки: уточняем по коду Rust (`ai_no_key`/`ai_no_consent`/…). */
function errorTitle(kind: ErrorKind, code: string): string {
  if (code === "ai_no_key") return "Ключ routerai.ru не задан";
  if (code === "ai_no_consent") return "Нужно согласие на отправку";
  if (code === "ai_disabled") return "AI выключен";
  return ERROR_TITLES[kind];
}

export interface AiPanelOptions {
  /** Контейнер тела режима AI (#ai-mode). */
  root: HTMLElement;
  /** Шов записи в журнал: владелец ленты — logPanel (без своей подписки). */
  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
  /** Текущий текст активного документа (для подстановки {{doc}}). */
  getDocumentText(): string;
}

export interface AiPanel {
  /** Реакция на смену режима/состояния дока: фокус ввода, закрытие поповера. */
  onDockState(mode: DockMode, state: DockState): void;
  dispose(): void;
}

/** Показ ошибки в чате (карточка с действиями). */
function errorKind(code: string, message: string): ErrorKind {
  const c = code.toLowerCase();
  const m = message.toLowerCase();
  if (c.includes("key") || c.includes("auth") || c.includes("401") || m.includes("401") || m.includes("ключ")) {
    return "auth";
  }
  if (
    c.includes("offline") ||
    c.includes("network") ||
    c.includes("timeout") ||
    m.includes("соедин") ||
    m.includes("network") ||
    m.includes("timeout") ||
    m.includes("офлайн")
  ) {
    return "offline";
  }
  if (c.includes("rate") || m.includes("429") || m.includes("лимит")) return "rate";
  if (c.includes("context") || m.includes("context length") || m.includes("слишком большой")) {
    return "context";
  }
  return "generic";
}

/** Элемент по id с проверкой: разметка AI-панели — контракт, падение — сразу. */
function byId<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`aiPanel: не найден элемент #${id}`);
  return el as T;
}

/** Значение с проверкой на null (для `querySelector`/`closest`). */
function must<T>(value: T | null, what: string): T {
  if (!value) throw new Error(`aiPanel: не найден ${what}`);
  return value;
}

/** «14:31:02» — время сообщения. */
function nowTime(): string {
  const d = new Date();
  const p = (n: number): string => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** Короткое имя модели для строки в шапке: «routerai · deepseek-v4.1-flash». */
function shortModel(model: string): string {
  const parts = model.split("/");
  return parts[parts.length - 1] || model;
}

export function createAiPanel(opts: AiPanelOptions): AiPanel {
  const { root, log, getDocumentText } = opts;

  // --- DOM ---
  const offStub = byId<HTMLElement>("ai-off");
  const enableBtn = byId<HTMLButtonElement>("ai-enable");
  const skillsRow = byId<HTMLElement>("ai-skills");
  const chat = byId<HTMLElement>("ai-chat");
  const input = byId<HTMLTextAreaElement>("ai-input");
  const sendBtn = byId<HTMLButtonElement>("ai-send");
  const clearBtn = byId<HTMLButtonElement>("ai-clear");
  const dot = byId<HTMLElement>("ai-dot");
  const statusDot = document.getElementById("ai-status-dot");
  const statusText = byId<HTMLElement>("ai-status-text");
  const settingsBtn = byId<HTMLButtonElement>("ai-settings");
  const popover = byId<HTMLElement>("ai-settings-pop");
  const settingsClose = byId<HTMLButtonElement>("ai-settings-close");
  const enabledInput = byId<HTMLInputElement>("ai-enabled");
  const enabledSub = byId<HTMLElement>("ai-enabled-sub");
  const config = byId<HTMLElement>("ai-config");
  const providerInput = byId<HTMLSelectElement>("ai-provider");
  const baseUrlInput = byId<HTMLInputElement>("ai-base-url");
  const modelInput = byId<HTMLInputElement>("ai-model");
  const keyInput = byId<HTMLInputElement>("ai-key");
  const keySave = byId<HTMLButtonElement>("ai-key-save");
  const keyHint = byId<HTMLElement>("ai-key-hint");
  const testBtn = byId<HTMLButtonElement>("ai-test");
  const testResult = byId<HTMLElement>("ai-test-result");
  const modelLabel = root.closest<HTMLElement>("#logs-panel")?.querySelector<HTMLElement>(".ai-model") ?? null;
  const inputRow = must(input.closest<HTMLElement>(".ai-input"), "контейнер .ai-input");

  // --- состояние ---
  let settings: AiSettingsView | null = null;
  let skills: readonly AiSkill[] = [];
  let skillsLoaded = false;
  let skillsLoading = false;
  let consentDenied = false; // пользователь отказал в отправке (жёлтый)
  let authError = false; // неверный ключ (401) — жёлтый «нет ключа»
  let lastError = false; // ошибка сети/LLM (красный)
  let busy = false;
  let stick = true;
  let popoverOpen = false;
  let wasAiExpanded = false;
  let history: AiMessage[] = [];
  let lastRequest: { display: string; content: string } | null = null;
  let disposed = false;
  const unlisteners: UnlistenFn[] = [];

  /** Текущий стриминговый запрос (один за раз). */
  interface Pending {
    token: number;
    row: HTMLElement;
    bubble: HTMLElement;
    col: HTMLElement;
    content: string;
    messageId: string | null;
    usage: AiUsage | null;
    finalized: boolean;
  }
  let pending: Pending | null = null;
  let requestToken = 0;

  // Подсказка пустого чата (внутри live-региона, но скрыта от скринридеров).
  const emptyHint = document.createElement("div");
  emptyHint.className = "ai-chat-empty";
  emptyHint.setAttribute("aria-hidden", "true");
  emptyHint.textContent = "Выберите скилл выше или напишите запрос.";
  chat.appendChild(emptyHint);

  // --- состояния и рендер UI ---

  function paintDot(el: HTMLElement | null, st: AiDotState): void {
    if (!el) return;
    for (const s of ["off", "warn", "ready", "error"] as const) {
      el.classList.toggle(`state-${s}`, s === st);
    }
  }

  function currentDotState(): AiDotState {
    if (!settings || !settings.enabled) return "off";
    if (lastError) return "error";
    if (authError || !settings.has_key || !settings.consent_document_send || consentDenied) return "warn";
    return "ready";
  }

  function stateLabel(st: AiDotState): string {
    if (st === "off") return "выключен";
    if (st === "error") return "ошибка · нет соединения";
    if (st === "ready") return "доступен · встроен";
    if (authError) return "неверный ключ";
    if (settings && !settings.has_key) return "нет ключа";
    return "нет согласия на отправку";
  }

  /** Единая точка приведения UI к состоянию: индикатор, заглушка, поля. */
  function applyState(): void {
    const st = currentDotState();
    const label = stateLabel(st);
    paintDot(dot, st);
    paintDot(statusDot, st);
    dot.title = `AI: ${label}`;
    statusText.textContent = label;

    const enabled = settings?.enabled ?? false;
    offStub.hidden = enabled;
    skillsRow.hidden = !enabled;
    chat.hidden = !enabled;
    inputRow.hidden = !enabled;
    input.disabled = !enabled;
    sendBtn.disabled = !enabled || busy;

    enabledInput.checked = enabled;
    enabledSub.textContent = enabled ? "включён" : "выключен";
    config.classList.toggle("is-disabled", !enabled);
    providerInput.disabled = !enabled;
    modelInput.disabled = !enabled;
    keyInput.disabled = !enabled;
    keySave.disabled = !enabled;
    testBtn.disabled = !enabled;

    if (settings) {
      // Не перезатираем поле, которое пользователь редактирует прямо сейчас.
      if (document.activeElement !== providerInput) providerInput.value = settings.provider;
      baseUrlInput.value = settings.base_url;
      if (document.activeElement !== modelInput) modelInput.value = settings.model;
      if (modelLabel) modelLabel.textContent = `routerai · ${shortModel(settings.model)}`;
    }
    keyHint.textContent = settings?.has_key ? "Ключ сохранён" : "Ключ не задан";
  }

  function updateEmpty(): void {
    emptyHint.hidden = chat.querySelector(".msg") !== null;
  }

  function scrollToBottom(): void {
    chat.scrollTop = chat.scrollHeight;
  }

  function setBusy(next: boolean): void {
    busy = next;
    sendBtn.disabled = !(settings?.enabled ?? false) || busy;
    dot.classList.toggle("busy", busy);
  }

  // --- скиллы ---

  function ensureSkills(): void {
    if (skillsLoaded || skillsLoading) return;
    skillsLoading = true;
    void aiListSkills()
      .then((list) => {
        skills = list.length > 0 ? list : FALLBACK_SKILLS;
      })
      .catch((e) => {
        skills = FALLBACK_SKILLS;
        log("warn", "system", `AI: скиллы недоступны (${errorMessage(e)}) — используются встроенные`);
      })
      .finally(() => {
        skillsLoading = false;
        skillsLoaded = true;
        renderSkills();
      });
  }

  function renderSkills(): void {
    skillsRow.textContent = "";
    for (const skill of skills) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "skill";
      btn.title = skill.description;
      btn.setAttribute("aria-label", `${skill.title}: ${skill.description}`);
      const icon = document.createElement("span");
      icon.className = "skill-icon";
      icon.setAttribute("aria-hidden", "true");
      icon.textContent = skill.id === "create-plugin" ? "🧩" : "▤";
      const title = document.createElement("span");
      title.textContent = skill.title;
      btn.append(icon, title);
      btn.addEventListener("click", () => onSkillClick(skill));
      skillsRow.appendChild(btn);
    }
    const soon = document.createElement("button");
    soon.type = "button";
    soon.className = "skill skill-soon";
    soon.disabled = true;
    soon.setAttribute("aria-disabled", "true");
    soon.setAttribute("aria-label", "Другие скиллы — скоро");
    soon.title = "Скоро: переструктурировать, конспект, отладить плагин, добавить view, объяснить синтаксис";
    const soonIcon = document.createElement("span");
    soonIcon.className = "skill-icon";
    soonIcon.setAttribute("aria-hidden", "true");
    soonIcon.textContent = "⏱";
    const soonTitle = document.createElement("span");
    soonTitle.textContent = "Ещё скиллы — скоро";
    soon.append(soonIcon, soonTitle);
    skillsRow.appendChild(soon);
  }

  /** Подстановка шаблона скилла: {{doc}} — из документа, {{input}} — из поля ввода. */
  function onSkillClick(skill: AiSkill): void {
    if (!settings?.enabled) return;
    const tpl = skill.prompt_template;
    if (tpl.includes("{{doc}}")) {
      send(skill.title, tpl.replace("{{doc}}", truncateDoc(getDocumentText())));
      return;
    }
    if (tpl.includes("{{input}}")) {
      const desc = input.value.trim();
      if (!desc) {
        input.focus();
        log("info", "system", "AI: для скилла «Создать плагин» нужно описание в поле ввода");
        return;
      }
      send(desc, tpl.replace("{{input}}", desc));
      return;
    }
    send(skill.title, tpl);
  }

  /** Обрезка текста документа до лимита байт UTF-8 (без разрыва символа). */
  function truncateDoc(text: string): string {
    const bytes = new TextEncoder().encode(text);
    if (bytes.length <= MAX_DOC_BYTES) return text;
    const cut = new TextDecoder().decode(bytes.subarray(0, MAX_DOC_BYTES));
    log("warn", "system", `AI: документ обрезан до ${Math.round(MAX_DOC_BYTES / 1024)} КБ`);
    return cut.replace(/\uFFFD+$/u, "");
  }

  // --- сообщения чата (только textContent) ---

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
    chat.appendChild(row);
    updateEmpty();
  }

  function appendAssistant(): { row: HTMLElement; bubble: HTMLElement; col: HTMLElement } {
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
    chat.appendChild(row);
    updateEmpty();
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
    chat.appendChild(row);
    updateEmpty();
    if (stick) scrollToBottom();
  }

  // --- отправка ---

  function errorActions(kind: ErrorKind, code: string): ErrorAction[] {
    const c = code.toLowerCase();
    if (kind === "auth") return [{ label: "Открыть настройки", run: openSettings }];
    // Контекст/согласие/выключенность повторять бессмысленно — только сообщение.
    if (kind === "context" || c === "ai_no_consent" || c === "ai_disabled") return [];
    return [{ label: "Повторить", run: retry }];
  }

  function send(display: string, content: string): void {
    if (!settings?.enabled || busy) return;
    // Согласие на отправку содержимого в облако — явный диалог перед первым запросом.
    if (!settings.consent_document_send || consentDenied) {
      askConsent(() => send(display, content));
      return;
    }
    lastRequest = { display, content };
    input.value = "";
    input.style.height = "";
    appendUser(display);
    history = history.concat({ role: "user", content });
    const { row, bubble, col } = appendAssistant();
    const token = ++requestToken;
    pending = { token, row, bubble, col, content: "", messageId: null, usage: null, finalized: false };
    setBusy(true);
    stick = true;
    scrollToBottom();
    void runChat(token);
  }

  async function runChat(token: number): Promise<void> {
    try {
      const res = await aiChat(history);
      if (disposed) return;
      const p = pending;
      if (!p || p.token !== token || p.finalized) return;
      if (!p.content) p.content = res.message.content;
      complete(p, res.usage ?? p.usage);
    } catch (e) {
      if (disposed) return;
      const p = pending;
      if (!p || p.token !== token || p.finalized) return;
      fail(p, "", errorMessage(e));
    }
  }

  /** Успешное завершение: финализировать пузырь, показать usage, обновить историю. */
  function complete(p: Pending, usage: AiUsage | null): void {
    p.finalized = true;
    pending = null;
    setBusy(false);
    const content = p.content || "Пустой ответ";
    p.bubble.textContent = content;
    if (usage) appendUsage(p.col, usage);
    history = history.concat({ role: "assistant", content });
    authError = false;
    lastError = false;
    log(
      "info",
      "system",
      `AI: ответ получен${usage ? ` (prompt ${usage.prompt_tokens}, completion ${usage.completion_tokens})` : ""}`
    );
    applyState();
    if (stick) scrollToBottom();
  }

  /** Ошибка запроса: убрать пустой пузырь, показать карточку ошибки с действиями. */
  function fail(p: Pending, code: string, message: string): void {
    p.finalized = true;
    pending = null;
    setBusy(false);
    p.row.remove();
    const c = code.toLowerCase();
    const kind = errorKind(code, message);
    if (c === "ai_no_consent") {
      // Согласие не дано — жёлтое состояние, без «повторить» (запрос не уйдёт).
      consentDenied = true;
      authError = false;
      lastError = false;
    } else if (kind === "auth") {
      authError = true;
      lastError = false;
    } else {
      lastError = true;
    }
    appendErrorCard(errorTitle(kind, c), message, errorActions(kind, c));
    log("error", "system", `AI: ${errorTitle(kind, c)} — ${message}`);
    applyState();
  }

  /** Повтор последнего неудачного запроса (история уже содержит user-сообщение). */
  function retry(): void {
    if (busy || !settings?.enabled || !lastRequest) return;
    const { row, bubble, col } = appendAssistant();
    const token = ++requestToken;
    pending = { token, row, bubble, col, content: "", messageId: null, usage: null, finalized: false };
    setBusy(true);
    stick = true;
    scrollToBottom();
    void runChat(token);
  }

  function submitInput(): void {
    const text = input.value.trim();
    if (!text) return;
    send(text, text);
  }

  function clearChat(): void {
    chat.textContent = "";
    history = [];
    lastRequest = null;
    chat.appendChild(emptyHint);
    updateEmpty();
  }

  // --- согласие ---

  function askConsent(onAllowed: () => void): void {
    let accepted = false;
    const content = document.createElement("div");
    content.className = "ai-consent";
    const p = document.createElement("p");
    p.textContent =
      "AI-ассистент может отправлять содержимое документа и историю чата в облачный сервис routerai.ru. Разрешить?";
    const warn = document.createElement("div");
    warn.className = "ai-consent-warning";
    warn.textContent = "Содержимое уйдёт на внешний сервер. Не отправляйте конфиденциальные документы.";
    const actions = document.createElement("div");
    actions.className = "dialog-actions";
    const deny = document.createElement("button");
    deny.type = "button";
    deny.className = "btn ghost";
    deny.textContent = "Не разрешать";
    const allow = document.createElement("button");
    allow.type = "button";
    allow.className = "btn primary";
    allow.textContent = "Разрешить";
    actions.append(deny, allow);
    content.append(p, warn, actions);

    const dialog: Dialog = openDialog({
      title: "Отправка содержимого в routerai.ru",
      content,
      initialFocus: allow,
      onClose: () => {
        if (accepted) {
          void grantConsent().then(onAllowed);
        } else {
          consentDenied = true;
          applyState();
          log("info", "system", "AI: отправка отклонена (нет согласия)");
        }
      },
    });
    deny.addEventListener("click", () => dialog.close());
    allow.addEventListener("click", () => {
      accepted = true;
      dialog.close();
    });
  }

  async function grantConsent(): Promise<void> {
    await persist({ consent_document_send: true });
    if (settings?.consent_document_send) {
      consentDenied = false;
      applyState();
      log("info", "system", "AI: согласие на отправку получено");
    }
  }

  // --- настройки ---

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
    if (!settings) return;
    const input: AiSettingsInput = { ...toInput(settings), ...patch };
    try {
      await aiSetSettings(input);
    } catch (e) {
      log("error", "system", `AI: настройки не сохранены (${errorMessage(e)})`);
      return;
    }
    settings = mergeSettings(settings, input);
    applyState();
  }

  async function saveKey(): Promise<void> {
    const key = keyInput.value.trim();
    if (!key) {
      keyInput.focus();
      return;
    }
    try {
      await aiSetApiKey(key);
      keyInput.value = "";
      settings = await aiGetSettings();
      authError = false;
      lastError = false;
      applyState();
      log("info", "system", "AI: API-ключ сохранён");
    } catch (e) {
      log("error", "system", `AI: ключ не сохранён (${errorMessage(e)})`);
    }
  }

  async function testConnection(): Promise<void> {
    testBtn.disabled = true;
    testResult.textContent = "Проверка соединения…";
    try {
      const res = await aiTestConnection();
      if (res.ok) {
        authError = false;
        lastError = false;
        testResult.textContent = res.model ? `Соединение установлено · ${res.model}` : "Соединение установлено";
        log("info", "system", "AI: соединение установлено");
      } else {
        // «Нет ключа» остаётся жёлтым; реальный сбой соединения — красный.
        if (settings?.has_key) lastError = true;
        testResult.textContent = res.error ? `Ошибка: ${res.error}` : "Не удалось соединиться";
        log("warn", "system", `AI: проверка соединения не удалась${res.error ? ` (${res.error})` : ""}`);
      }
      applyState();
    } catch (e) {
      if (settings?.has_key) lastError = true;
      testResult.textContent = `Ошибка: ${errorMessage(e)}`;
      applyState();
    } finally {
      testBtn.disabled = !(settings?.enabled ?? false);
    }
  }

  function openSettings(): void {
    popover.hidden = false;
    popoverOpen = true;
  }

  function closeSettings(): void {
    popover.hidden = true;
    popoverOpen = false;
  }

  function toggleSettings(): void {
    if (popoverOpen) closeSettings();
    else openSettings();
  }

  async function enableFromStub(): Promise<void> {
    await persist({ enabled: true });
    if (settings?.enabled) {
      lastError = false;
      ensureSkills();
      applyState();
      openSettings();
      input.focus();
    }
  }

  // --- обработчики ---

  const onInputKeyDown = (e: KeyboardEvent): void => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submitInput();
    }
  };
  const onInput = (): void => {
    input.style.height = "auto";
    input.style.height = `${Math.min(input.scrollHeight, 76)}px`;
  };
  const onSendClick = (): void => submitInput();
  const onChatScroll = (): void => {
    stick = chat.scrollHeight - chat.scrollTop - chat.clientHeight <= STICK_THRESHOLD;
  };
  const onClearClick = (): void => clearChat();
  const onSettingsClick = (): void => toggleSettings();
  const onSettingsClose = (): void => closeSettings();
  const onKeySave = (): void => void saveKey();
  const onTest = (): void => void testConnection();
  const onEnableClick = (): void => void enableFromStub();
  const onEnabledChange = (): void => {
    const on = enabledInput.checked;
    void persist({ enabled: on }).then(() => {
      if (on) {
        lastError = false;
        ensureSkills();
      } else {
        closeSettings();
      }
      applyState();
    });
  };
  const onProviderChange = (): void => void persist({ provider: providerInput.value });
  const onModelChange = (): void => void persist({ model: modelInput.value.trim() });
  const onDocPointerDown = (e: PointerEvent): void => {
    if (!popoverOpen) return;
    const t = e.target;
    if (t instanceof Node && (popover.contains(t) || settingsBtn.contains(t))) return;
    closeSettings();
  };
  const onDocKeyDown = (e: KeyboardEvent): void => {
    if (e.key !== "Escape" || !popoverOpen || isModalOpen()) return;
    e.preventDefault();
    e.stopPropagation();
    closeSettings();
  };

  input.addEventListener("keydown", onInputKeyDown);
  input.addEventListener("input", onInput);
  sendBtn.addEventListener("click", onSendClick);
  chat.addEventListener("scroll", onChatScroll);
  clearBtn.addEventListener("click", onClearClick);
  settingsBtn.addEventListener("click", onSettingsClick);
  settingsClose.addEventListener("click", onSettingsClose);
  keySave.addEventListener("click", onKeySave);
  testBtn.addEventListener("click", onTest);
  enableBtn.addEventListener("click", onEnableClick);
  enabledInput.addEventListener("change", onEnabledChange);
  providerInput.addEventListener("change", onProviderChange);
  modelInput.addEventListener("change", onModelChange);
  document.addEventListener("pointerdown", onDocPointerDown);
  document.addEventListener("keydown", onDocKeyDown, true);

  // --- события стриминга (host → front) ---

  function subscribe<T>(event: string, handler: (e: Event<T>) => void): void {
    void listen<T>(event, handler)
      .then((un) => {
        if (disposed) un();
        else unlisteners.push(un);
      })
      .catch((err) => console.error("listen:", event, err));
  }

  subscribe<{ message_id: string; delta: string }>("ai:chunk", (event) => {
    const p = pending;
    if (!p) return;
    const { message_id, delta } = event.payload;
    if (p.messageId !== null && p.messageId !== message_id) return; // чанк чужого/устаревшего ответа
    p.messageId = message_id;
    p.content += delta;
    p.bubble.textContent = p.content; // только textContent (MVP)
    updateEmpty();
    if (stick) scrollToBottom();
  });

  subscribe<{ message_id: string; usage: AiUsage | null }>("ai:done", (event) => {
    const p = pending;
    if (!p) return;
    const { message_id, usage } = event.payload;
    if (p.messageId !== null && p.messageId !== message_id) return;
    p.messageId = message_id;
    p.usage = usage;
    // Пустой стрим (чанков не было) финализирует ответ команды `ai_chat`.
    if (p.content) complete(p, usage);
  });

  subscribe<{ code: string; message: string }>("ai:error", (event) => {
    const p = pending;
    if (!p) return;
    fail(p, event.payload.code, event.payload.message);
  });

  // --- публичный интерфейс ---

  function onDockState(mode: DockMode, state: DockState): void {
    const active = mode === "ai" && state === "expanded";
    if (active && !wasAiExpanded && settings?.enabled) {
      ensureSkills();
      input.focus();
    }
    if (mode !== "ai") closeSettings();
    wasAiExpanded = active;
  }

  function dispose(): void {
    disposed = true;
    for (const un of unlisteners) un();
    unlisteners.length = 0;
    input.removeEventListener("keydown", onInputKeyDown);
    input.removeEventListener("input", onInput);
    sendBtn.removeEventListener("click", onSendClick);
    chat.removeEventListener("scroll", onChatScroll);
    clearBtn.removeEventListener("click", onClearClick);
    settingsBtn.removeEventListener("click", onSettingsClick);
    settingsClose.removeEventListener("click", onSettingsClose);
    keySave.removeEventListener("click", onKeySave);
    testBtn.removeEventListener("click", onTest);
    enableBtn.removeEventListener("click", onEnableClick);
    enabledInput.removeEventListener("change", onEnabledChange);
    providerInput.removeEventListener("change", onProviderChange);
    modelInput.removeEventListener("change", onModelChange);
    document.removeEventListener("pointerdown", onDocPointerDown);
    document.removeEventListener("keydown", onDocKeyDown, true);
  }

  // Начальная загрузка настроек (локальное чтение config.json, не сеть).
  void aiGetSettings()
    .then((view) => {
      if (disposed) return;
      settings = view;
      applyState();
      if (view.enabled) ensureSkills();
    })
    .catch((e) => {
      if (disposed) return;
      settings = null;
      log("error", "system", `AI: настройки недоступны (${errorMessage(e)})`);
      applyState();
    });

  renderSkills(); // чип «скоро» виден сразу; боевые скиллы добавятся после загрузки
  applyState();
  updateEmpty();

  return { onDockState, dispose };
}
