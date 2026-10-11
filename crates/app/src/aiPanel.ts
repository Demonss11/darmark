// aiPanel.ts — тело режима «AI-ассистент» нижнего дока (ADR-0030; сеть и
// провайдер — ADR-0032/0033; рантайм — ADR-0034). Модуль-оболочка (как
// logPanel/resourceMonitor), НЕ view
// и НЕ pane: монтируется logPanel'ом в контейнер #logs-panel (режим ai).
//
// Инварианты: сетевые вызовы — только на Rust-стороне (CSP `default-src 'self'`);
// ответ модели рендерится ИСКЛЮЧИТЕЛЬНО через textContent (MVP; сырой HTML/ответ
// LLM в innerHTML запрещён). История чата — в памяти на сессию (без персист.).
// Стриминг: события ai:chunk/ai:done/ai:error дописывают/финализируют текущий
// пузырь ассистента; отписки сохраняются и снимаются (без утечек).
//
// Фаза 2 (harness, ADR-0034): под ответом ассистента — «Применить как правку» (дифф)
// и карточка плагина из ```json-блока. Правки документа идут через ai_apply_edit
// (+ undo_document); плагины — через ai_*_plugin. Карточки собираются DOM API и
// textContent; сырой вывод модели в innerHTML не попадает.

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { openDialog, isModalOpen, type Dialog } from "./dialog";
import {
  aiApplyEdit,
  aiChat,
  aiCreatePlugin,
  aiDeletePlugin,
  aiDiff,
  aiGetSettings,
  aiListSkills,
  aiSetApiKey,
  aiSetSettings,
  aiTestConnection,
  aiUpdatePlugin,
  aiValidatePlugin,
  errorMessage,
  reloadPlugin,
  undoDocument,
} from "./tauri";
import type {
  AiDiff,
  AiMessage,
  AiSettingsInput,
  AiSettingsView,
  AiSkill,
  AiUsage,
  PluginStatus,
} from "./tauri";
import { buildUnitMaps, bytesToUnits } from "./mapping";
import type { DocumentId } from "./ids";
import type { Json } from "./viewRegistry";
import type { DockMode, DockState, LogCategory, LogLevel } from "./logPanel";

/** Лимит текста документа за запрос (ADR-0032). */
const MAX_DOC_BYTES = 100 * 1024;

/** Допуск «чат прижат к низу», px — автопрокрутка с заморозкой. */
const STICK_THRESHOLD = 24;

/** Состояние индикатора #ai-dot (ADR-0030/0034). */
type AiDotState = "off" | "warn" | "ready" | "error";

/** Классификация ошибки для карточки и действий. */
type ErrorKind = "auth" | "offline" | "rate" | "context" | "generic";

/** Действие в карточке ошибки. */
interface ErrorAction {
  label: string;
  run(): void;
}

/** Встроенные скиллы-фолбэк, если `ai_list_skills` недоступен (ADR-0032). */
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
    // Синхронизирован с Rust (`ai/skills.rs`, `CREATE_PLUGIN_PROMPT`): модель обязана
    // вернуть РОВНО один fenced ```json с { id, name, version, permissions, source },
    // иначе карточка плагина не парсится (`extractPluginProposal`).
    prompt_template: `Ты — генератор Lua-плагинов для darmark. По описанию: {{input}}
Верни РОВНО один fenced-блок \`\`\`json с одним объектом:
{"id": "…", "name": "…", "version": "1.0.0", "permissions": [], "source": "полный код main.lua"}
Правила: id — только строчные латинские буквы, цифры, '-' и '_'; permissions — только из набора
darmark (document:read, document:write, view:create, view:modify, ui:statusbar); source — полный
код main.lua с функцией on_activate и host-API песочницы darmark. Никакого текста вне блока.`,
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
  /** Активный документ для harness-правок (id + rev) или `null`, если открытого нет. */
  getActiveDocument(): { id: DocumentId; rev: number } | null;
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

/** Лимит строк диффа в карточке: защита от огромных правок (ADR-0032, 10 000 строк). */
const MAX_DIFF_LINES = 500;

/** Человекочитаемая подпись статуса плагина для бейджа карточки. */
const PLUGIN_STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "активен",
  failed: "ошибка",
  quarantined: "карантин",
  stopped: "остановлен",
};

/** Кодовый блок ```json в ответе модели — предложение плагина (скилл create-plugin, ADR-0034/ADR-0026). */
const PLUGIN_FENCE = /```json[^\n]*\n([\s\S]*?)```/gi;

/** Предложение плагина, извлечённое из ответа ассистента. */
interface AiPluginProposal {
  id: string;
  name: string;
  version: string;
  source: string;
  /** Собранный `plugin.json` (без `source`) для `ai_validate_plugin`/`ai_create_plugin`. */
  manifest: Json;
}

/** Карточка плагина, зарегистрированная для реакции на события `ai:plugin_*`. */
interface PluginCardState {
  id: string;
  /** Установлен ли плагин карточки (идемпотентная реакция на события). */
  isInstalled(): boolean;
  installed(label: string, stateClass: string): void;
  deleted(): void;
}

/** Код IPC-ошибки из `CommandError { code, message }` (или `undefined`). */
function errorCode(e: unknown): string | undefined {
  if (e && typeof e === "object" && "code" in e) {
    const code = (e as { code?: unknown }).code;
    if (typeof code === "string") return code;
  }
  return undefined;
}

/** Создаёт элемент с классом: карточки собираются только DOM API (без innerHTML). */
function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
}

/** Кнопка карточки: тип, подпись и доступное имя (для скринридеров). */
function actionButton(label: string, className: string, ariaLabel?: string): HTMLButtonElement {
  const button = el("button", className);
  button.type = "button";
  button.textContent = label;
  if (ariaLabel) button.setAttribute("aria-label", ariaLabel);
  return button;
}

/** Разбивает текст на строки; пустой ввод — пустой список (пустую строку не рисуем). */
function splitLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.split("\n");
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/** Строка диффа: `+`/`-` + текст (только textContent); перенос даёт `display:block`. */
function appendDiffLine(pre: HTMLElement, kind: "add" | "del", line: string): void {
  const span = el("span", kind === "add" ? "ai-diff-add" : "ai-diff-del");
  span.textContent = `${kind === "add" ? "+" : "-"} ${line}`;
  pre.appendChild(span);
}

/**
 * Рисует тело диффа по `ops`: удалённые фрагменты — красным, добавленные — зелёным.
 * Байтовые (UTF-8) смещения переводятся в UTF-16 едиными картами `mapping.ts`.
 */
function renderDiffBody(pre: HTMLElement, diff: AiDiff): void {
  const maps = buildUnitMaps(diff.original);
  const ops = [...diff.ops].sort((a, b) => a.start - b.start);
  let shown = 0;
  let truncated = false;
  for (const op of ops) {
    const removed = diff.original.slice(bytesToUnits(maps, op.start), bytesToUnits(maps, op.end));
    for (const line of splitLines(removed)) {
      if (shown >= MAX_DIFF_LINES) {
        truncated = true;
        break;
      }
      appendDiffLine(pre, "del", line);
      shown++;
    }
    if (truncated) break;
    for (const line of splitLines(op.text)) {
      if (shown >= MAX_DIFF_LINES) {
        truncated = true;
        break;
      }
      appendDiffLine(pre, "add", line);
      shown++;
    }
    if (truncated) break;
  }
  if (truncated) {
    const note = el("span", "ai-diff-more");
    note.textContent = `… дифф сокращён (лимит ${MAX_DIFF_LINES} строк)`;
    pre.appendChild(note);
  }
}

/** Ищет первый валидный ```json-блок плагина в ответе ассистента (ADR-0034/ADR-0026). */
function extractPluginProposal(content: string): AiPluginProposal | null {
  PLUGIN_FENCE.lastIndex = 0; // глобальный regexp: сбрасываем позицию между вызовами
  let match: RegExpExecArray | null;
  while ((match = PLUGIN_FENCE.exec(content)) !== null) {
    const proposal = parsePluginBlock(match[1]);
    if (proposal) return proposal;
  }
  return null;
}

/** Разбирает JSON-блок плагина и собирает `plugin.json` (без `source`). */
function parsePluginBlock(raw: string): AiPluginProposal | null {
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!data || typeof data !== "object" || Array.isArray(data)) return null;
  const obj = data as Record<string, unknown>;
  const id = typeof obj.id === "string" ? obj.id.trim() : "";
  const source = typeof obj.source === "string" ? obj.source : "";
  if (!id || !source) return null;
  const name = typeof obj.name === "string" && obj.name.trim() ? obj.name.trim() : id;
  const version =
    typeof obj.version === "string" && obj.version.trim() ? obj.version.trim() : "0.1.0";
  const rawPermissions: unknown[] = Array.isArray(obj.permissions) ? obj.permissions : [];
  const permissions = rawPermissions.filter((p): p is string => typeof p === "string");
  // api_version/entry — значения по умолчанию шаблона `darmark-plugin init` (ADR-0034 §2.4).
  const manifest: Json = { id, name, version, api_version: 1, entry: "main.lua", permissions };
  return { id, name, version, source, manifest };
}

/** Confirm-диалог удаления плагина через `dialog.ts` (по образцу pluginManager). */
function confirmDeletePlugin(pluginId: string): Promise<boolean> {
  return new Promise((resolve) => {
    const content = el("div", "ai-confirm");
    const text = el("p", "ai-confirm-text");
    text.textContent = `Плагин «${pluginId}» будет удалён безвозвратно: файлы и настройки.`;
    content.appendChild(text);

    const buttons = el("div", "dialog-actions");
    const cancel = actionButton("Отмена", "btn ghost");
    const confirm = actionButton("Удалить", "btn primary");
    buttons.append(cancel, confirm);
    content.appendChild(buttons);

    let dialog: Dialog | null = null;
    const finish = (value: boolean): void => {
      // Сначала фиксируем результат: `close()` вызовет `onClose` (resolve(false)),
      // который после уже принятого resolve игнорируется.
      resolve(value);
      dialog?.close();
    };
    cancel.addEventListener("click", () => finish(false));
    confirm.addEventListener("click", () => finish(true));

    dialog = openDialog({
      title: "Удаление плагина",
      content,
      initialFocus: cancel,
      onClose: () => resolve(false),
    });
  });
}

export function createAiPanel(opts: AiPanelOptions): AiPanel {
  const { root, log, getDocumentText, getActiveDocument } = opts;

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
  const keyClear = byId<HTMLButtonElement>("ai-key-clear");
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

  // Фаза 2 (harness): карточки плагина, зарегистрированные для реакции на события
  // `ai:plugin_*`. Карточка диффа НЕ регистрируется: пометку «применено» ставит
  // синхронный ответ `ai_apply_edit` той карточки, на которой нажали «Применить»
  // (иначе событие пометило бы все карточки документа). Счётчик даёт уникальные id.
  let cardSeq = 0;
  const pluginCards = new Set<PluginCardState>();

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
    // «Удалить ключ» доступна только когда ключ реально задан.
    keyClear.disabled = !enabled || !(settings?.has_key ?? false);
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

  // --- Фаза 2: harness — карточка диффа и карточка плагина (ADR-0034) ---

  /** Короткая заметка под сообщением (info/error) — только textContent. */
  function appendNotice(col: HTMLElement, text: string, kind: "info" | "error"): void {
    const note = el("div", kind === "error" ? "ai-note ai-note-error" : "ai-note");
    note.setAttribute("role", kind === "error" ? "alert" : "status");
    note.textContent = text;
    col.appendChild(note);
    if (stick) scrollToBottom();
  }

  /**
   * Считает дифф по ответу ассистента и рисует карточку. `false` — правок нет,
   * нет активного документа или ошибка IPC (заметка уже показана).
   */
  async function buildDiff(col: HTMLElement, content: string): Promise<boolean> {
    const doc = getActiveDocument();
    if (!doc) {
      appendNotice(col, "Нет активного документа — правку применить некуда.", "error");
      return false;
    }
    try {
      const diff = await aiDiff(doc.id, content);
      if (disposed) return false;
      if (diff.ops.length === 0) {
        appendNotice(col, "Правок не требуется — ответ совпадает с документом.", "info");
        return false;
      }
      renderDiffCard(col, diff, doc.id, content);
      return true;
    } catch (e) {
      if (disposed) return false;
      appendNotice(col, `Не удалось подготовить правку: ${errorMessage(e)}`, "error");
      return false;
    }
  }

  /** Клик «Применить как правку»: кнопку гасим только при успехе (карточка построена). */
  async function requestDiff(
    col: HTMLElement,
    content: string,
    button: HTMLButtonElement
  ): Promise<void> {
    button.disabled = true;
    const ok = await buildDiff(col, content);
    if (!disposed && !ok) button.disabled = false;
  }

  /** Карточка диффа: предпросмотр по `ops` + «Применить»/«Отклонить»/«Отменить» (ADR-0034). */
  function renderDiffCard(
    col: HTMLElement,
    diff: AiDiff,
    docId: DocumentId,
    content: string
  ): void {
    const id = ++cardSeq;
    const card = el("div", "ai-diff");
    card.id = `ai-diff-${id}`;
    card.setAttribute("role", "group");
    card.setAttribute("aria-label", "Предлагаемая правка");

    const header = el("div", "ai-diff-header");
    header.textContent = "Предлагаемая правка";

    const body = el("pre", "ai-diff-body");
    renderDiffBody(body, diff);

    const status = el("div", "ai-diff-status");
    status.setAttribute("role", "status");
    status.textContent = `Изменений: ${diff.ops.length}.`;

    const actions = el("div", "ai-diff-actions");
    const apply = actionButton("Применить", "btn primary", "Применить правку к документу");
    apply.id = `ai-diff-apply-${id}`;
    const reject = actionButton("Отклонить", "btn ghost", "Отклонить правку");
    reject.id = `ai-diff-reject-${id}`;
    actions.append(apply, reject);

    card.append(header, body, status, actions);
    col.appendChild(card);
    if (stick) scrollToBottom();

    let applied = false;

    /** Правка применена (синхронный ответ `ai_apply_edit`): показываем «Отменить». */
    function markApplied(rev: number): void {
      if (applied) return;
      applied = true;
      actions.textContent = "";
      const undo = actionButton("Отменить", "btn ghost", "Отменить применённую правку");
      undo.id = `ai-diff-undo-${id}`;
      undo.addEventListener("click", () => void undoEdit(undo));
      actions.appendChild(undo);
      status.textContent = `Правка применена (rev ${rev}).`;
    }

    async function undoEdit(button: HTMLButtonElement): Promise<void> {
      button.disabled = true;
      try {
        await undoDocument(docId);
        if (disposed) return;
        status.textContent = "Правка отменена.";
        actions.textContent = "";
      } catch (e) {
        if (disposed) return;
        button.disabled = false;
        status.textContent = `Не удалось отменить: ${errorMessage(e)}`;
      }
    }

    async function applyEdit(): Promise<void> {
      apply.disabled = true;
      reject.disabled = true;
      status.textContent = "Применение…";
      try {
        const res = await aiApplyEdit(docId, diff.base_rev, diff.ops);
        if (disposed) return;
        markApplied(res.rev);
      } catch (e) {
        if (disposed) return;
        apply.disabled = false;
        reject.disabled = false;
        if (errorCode(e) === "ai_stale_edit") {
          // Документ изменился между diff и apply — предлагаем пересчитать (ADR-0034).
          status.textContent = "Документ изменился, пересмотрите правку.";
          const reconsider = actionButton(
            "Пересмотреть",
            "btn ghost",
            "Пересчитать правку по актуальному документу"
          );
          reconsider.addEventListener("click", () => {
            card.remove();
            void buildDiff(col, content);
          });
          actions.appendChild(reconsider);
        } else {
          status.textContent = `Не удалось применить: ${errorMessage(e)}`;
        }
      }
    }

    function rejectEdit(): void {
      card.remove();
    }

    apply.addEventListener("click", () => void applyEdit());
    reject.addEventListener("click", rejectEdit);
  }

  /** Карточка плагина из ```json-блока: предпросмотр, валидация, создание/удаление (ADR-0034/ADR-0026). */
  function renderPluginCard(col: HTMLElement, proposal: AiPluginProposal): void {
    const id = ++cardSeq;
    const card = el("div", "ai-plugin-card");
    card.id = `ai-plugin-${id}`;
    card.setAttribute("role", "group");
    card.setAttribute("aria-label", `Плагин ${proposal.name} версии ${proposal.version}`);

    const header = el("div", "ai-plugin-header");
    const icon = el("span", "ai-plugin-icon");
    icon.setAttribute("aria-hidden", "true");
    icon.textContent = "🧩";
    const name = el("span", "ai-plugin-name");
    name.textContent = proposal.name;
    const version = el("span", "ai-plugin-version");
    version.textContent = `v${proposal.version}`;
    const badge = el("span", "ai-plugin-badge");
    badge.textContent = "предлагается";
    header.append(icon, name, version, badge);

    const code = el("pre", "ai-plugin-code");
    code.textContent = proposal.source; // сырой вывод модели — только textContent

    const status = el("div", "ai-plugin-status");
    status.setAttribute("role", "status");
    status.textContent = "Проверка…";

    const actions = el("div", "ai-plugin-actions");
    const save = actionButton("Создать", "btn primary", `Создать плагин ${proposal.id}`);
    save.id = `ai-plugin-save-${id}`;
    const reload = actionButton(
      "Перезагрузить",
      "btn ghost",
      `Перезагрузить плагин ${proposal.id}`
    );
    reload.id = `ai-plugin-reload-${id}`;
    reload.disabled = true; // доступна только когда плагин уже установлен
    const remove = actionButton("Удалить", "btn ghost danger", `Удалить плагин ${proposal.id}`);
    remove.id = `ai-plugin-remove-${id}`;
    remove.disabled = true;
    actions.append(save, reload, remove);

    card.append(header, code, status, actions);
    col.appendChild(card);
    if (stick) scrollToBottom();

    let exists = false;
    const state: PluginCardState = {
      id: proposal.id,
      isInstalled: () => exists,
      installed(label, stateClass) {
        markInstalled(label, stateClass);
      },
      deleted() {
        markDeleted();
      },
    };
    pluginCards.add(state);

    /** Плагин установлен (создан/обновлён/уже существует): открываем reload/delete. */
    function markInstalled(label: string, stateClass: string): void {
      exists = true;
      badge.className = `ai-plugin-badge ${stateClass}`;
      badge.textContent = label;
      save.textContent = "Сохранить";
      save.setAttribute("aria-label", `Сохранить плагин ${proposal.id}`);
      save.disabled = false;
      reload.disabled = false;
      remove.disabled = false;
      status.classList.remove("is-error");
    }

    function markDeleted(): void {
      exists = false;
      badge.className = "ai-plugin-badge stopped";
      badge.textContent = "удалён";
      save.disabled = true;
      reload.disabled = true;
      remove.disabled = true;
      status.classList.remove("is-error", "is-ok");
      status.textContent = "Плагин удалён.";
      pluginCards.delete(state);
    }

    /** Валидация при появлении карточки (манифест + entry + песочница, ADR-0034/ADR-0026). */
    async function validate(): Promise<void> {
      status.textContent = "Проверка…";
      try {
        const res = await aiValidatePlugin(proposal.id, proposal.manifest, proposal.source);
        if (disposed) return;
        status.classList.toggle("is-ok", res.ok);
        status.classList.toggle("is-error", !res.ok);
        status.textContent = res.ok
          ? "Проверка: ✓ пройдена"
          : `Проверка не пройдена: ${res.errors.join("; ") || "неизвестная ошибка"}`;
      } catch (e) {
        if (disposed) return;
        status.classList.remove("is-ok");
        status.classList.add("is-error");
        status.textContent = `Проверка недоступна: ${errorMessage(e)}`;
      }
    }

    /** «Создать»/«Сохранить»: первый клик — create; при существующем плагине — update. */
    async function savePlugin(): Promise<void> {
      save.disabled = true;
      reload.disabled = true;
      remove.disabled = true;
      status.classList.remove("is-error", "is-ok");
      status.textContent = exists ? "Сохранение…" : "Создание…";
      try {
        const created = !exists;
        const info = exists
          ? await aiUpdatePlugin(proposal.id, proposal.manifest, proposal.source)
          : await aiCreatePlugin(proposal.id, proposal.manifest, proposal.source);
        if (disposed) return;
        markInstalled(PLUGIN_STATUS_LABELS[info.status.state], info.status.state);
        status.classList.add("is-ok");
        status.textContent = created ? "Плагин создан." : "Плагин обновлён.";
      } catch (e) {
        if (disposed) return;
        save.disabled = false;
        handleSaveError(e);
      }
    }

    function handleSaveError(e: unknown): void {
      const code = errorCode(e);
      status.classList.remove("is-ok");
      status.classList.add("is-error");
      if (code === "ai_plugin_exists") {
        // Плагин уже стоит: переключаем на обновление (update) и открываем reload/delete.
        exists = true;
        save.textContent = "Сохранить";
        save.setAttribute("aria-label", `Сохранить плагин ${proposal.id}`);
        reload.disabled = false;
        remove.disabled = false;
        status.textContent = "Плагин уже существует — нажмите «Сохранить», чтобы обновить.";
      } else if (code === "ai_plugin_invalid") {
        status.textContent = `Плагин не прошёл проверку: ${errorMessage(e)}`;
      } else {
        status.textContent = `Не удалось создать плагин: ${errorMessage(e)}`;
      }
    }

    async function reloadPluginCard(): Promise<void> {
      reload.disabled = true;
      try {
        await reloadPlugin(proposal.id);
        if (disposed) return;
        status.classList.remove("is-error");
        status.textContent = "Плагин перезагружен.";
      } catch (e) {
        if (disposed) return;
        status.classList.add("is-error");
        status.textContent = `Не удалось перезагрузить: ${errorMessage(e)}`;
      } finally {
        if (!disposed) reload.disabled = false;
      }
    }

    async function removePluginCard(): Promise<void> {
      const confirmed = await confirmDeletePlugin(proposal.id);
      if (!confirmed || disposed) return;
      remove.disabled = true;
      try {
        await aiDeletePlugin(proposal.id);
        if (disposed) return;
        markDeleted();
      } catch (e) {
        if (disposed) return;
        status.classList.add("is-error");
        status.textContent = `Не удалось удалить: ${errorMessage(e)}`;
        remove.disabled = false;
      }
    }

    save.addEventListener("click", () => void savePlugin());
    reload.addEventListener("click", () => void reloadPluginCard());
    remove.addEventListener("click", () => void removePluginCard());
    void validate();
  }

  /** Действия под ответом ассистента: «Применить как правку» + карточка плагина (ADR-0034). */
  function appendAssistantActions(col: HTMLElement, content: string): void {
    const actions = el("div", "ai-msg-actions");
    const apply = actionButton(
      "Применить как правку",
      "btn ghost",
      "Применить ответ ассистента как правку документа"
    );
    apply.addEventListener("click", () => void requestDiff(col, content, apply));
    actions.appendChild(apply);
    col.appendChild(actions);

    const proposal = extractPluginProposal(content);
    if (proposal) renderPluginCard(col, proposal);
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
    // Фаза 2: действия над готовым ответом (дифф-правка + карточка плагина).
    appendAssistantActions(p.col, content);
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
    // Инвалидируем текущий запрос: осиротевший ответ не должен дописаться в
    // удалённый пузырь и не попадёт в очищенную history.
    requestToken++;
    if (pending) {
      pending.finalized = true;
      pending = null;
    }
    setBusy(false);
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
    // Формулировка — канон ADR-0032: явно называем получателя и модель.
    p.textContent =
      "AI-ассистент может отправлять содержимое документа и историю чата в облачный сервис routerai.ru (для запроса к модели deepseek/deepseek-v4.1-flash). Разрешить?";
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
    // Пустое поле + «Сохранить»/«Удалить ключ» = очистка: Rust трактует "" как снятие ключа.
    try {
      await aiSetApiKey(key);
      keyInput.value = "";
      settings = await aiGetSettings();
      authError = false;
      lastError = false;
      applyState();
      log("info", "system", key ? "AI: API-ключ сохранён" : "AI: API-ключ удалён");
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
    // Enter и Ctrl+Enter — отправить; Shift+Enter — перенос строки (ADR-0030).
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
  const onKeyClear = (): void => {
    keyInput.value = ""; // явная очистка: пустой ключ → ai_set_api_key("")
    void saveKey();
  };
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
  keyClear.addEventListener("click", onKeyClear);
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

  // --- события harness Фазы 2 (host → front) ---
  // `ai:edit_applied` панель для карточек НЕ использует: применённую карточку
  // помечает синхронный ответ `ai_apply_edit` (иначе событие пометило бы все
  // карточки того же документа). Обновление редактора/предпросмотра идёт отдельным
  // путём `document-updated` → `store.applyHostUpdate` (main.ts).

  // Жизненный цикл плагина: обновляем бейдж/доступность кнопок в карточке.
  // Карточку, уже обновлённую ответом IPC, событие не перетирает (гейт isInstalled).
  subscribe<{ plugin_id: string }>("ai:plugin_created", (event) => {
    for (const card of pluginCards) {
      if (card.id === event.payload.plugin_id && !card.isInstalled()) {
        card.installed("установлен", "active");
      }
    }
  });

  subscribe<{ plugin_id: string }>("ai:plugin_updated", (event) => {
    for (const card of pluginCards) {
      if (card.id === event.payload.plugin_id && !card.isInstalled()) {
        card.installed("обновлён", "active");
      }
    }
  });

  subscribe<{ plugin_id: string }>("ai:plugin_deleted", (event) => {
    for (const card of pluginCards) {
      if (card.id === event.payload.plugin_id) card.deleted();
    }
  });

  // --- публичный интерфейс ---

  function onDockState(mode: DockMode, state: DockState): void {
    const active = mode === "ai" && state === "expanded";
    if (active && !wasAiExpanded && settings?.enabled) {
      ensureSkills();
      input.focus();
    }
    // Поповер настроек живёт только при активном раскрытии режима AI: при уходе в
    // другой режим, свёртке или скрытии дока он теряет якорь — закрываем.
    if (!active) closeSettings();
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
    keyClear.removeEventListener("click", onKeyClear);
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
