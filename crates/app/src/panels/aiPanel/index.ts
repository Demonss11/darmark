// aiPanel/index.ts — тело режима «AI-ассистент» нижнего дока (ADR-0030; сеть и
// провайдер — ADR-0032/0033; рантайм — ADR-0034). Модуль-оболочка (как
// logPanel/resourceMonitor), НЕ view и НЕ pane: монтируется logPanel'ом в
// контейнер #logs-panel (режим ai).
//
// Инварианты: сетевые вызовы — только на Rust-стороне (CSP `default-src 'self'`).
// Рендер ответа ассистента — только через `safeHtml.setSanitizedHtml` (ADR-0038
// §2.1/§2.2): HTML ответа модели проходит Rust-санитайзер `md-core`, прямых
// присваиваний `innerHTML` в этом модуле нет. Всё, что не проходит этот хелпер —
// стриминг, код плагина, дифф, ошибки — только `textContent`.
// История чата — в памяти на сессию (без персист.) и хранит ТЕКСТ ответа, не HTML.
// Стриминг: события ai:chunk/ai:done/ai:error дописывают/финализируют текущий
// пузырь ассистента; отписки сохраняются и снимаются.
//
// Фаза 2 (harness, ADR-0034): под ответом ассистента — «Применить как правку»
// (дифф через ai_apply_edit + undo_document) и карточка плагина из ```json-блока
// (через ai_*_plugin).

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { openDialog, isModalOpen, type Dialog } from "../../ui/dialog";
import {
  aiApplyEdit,
  aiChat,
  aiCreatePlugin,
  aiDeletePlugin,
  aiDiff,
  aiGetSettings,
  aiHistoryClear,
  aiHistoryLoad,
  aiHistorySave,
  aiListSkills,
  aiSetApiKey,
  aiSetSettings,
  aiTestConnection,
  aiUpdatePlugin,
  aiValidatePlugin,
  errorMessage,
  reloadPlugin,
  undoDocument,
} from "../../core/tauri";
import type {
  AiDiff,
  AiMessage,
  AiSettingsInput,
  AiSettingsView,
  AiSkill,
  AiUsage,
} from "../../core/tauri";
import { renderAiMarkdown, setSanitizedHtml } from "../../core/safeHtml";
import type { DocumentId } from "../../core/ids";
import type { DockMode, DockState, LogCategory, LogLevel } from "../logPanel";
import { byId, must, nowTime, shortModel, el, actionButton } from "./dom";
import { MAX_DOC_BYTES, STICK_THRESHOLD, type AiDotState } from "./constants";
import { errorTitle, errorKind, errorCode, type ErrorKind, type ErrorAction } from "./errors";
import { renderDiffBody } from "./diff";
import { FALLBACK_SKILLS } from "./skills";
import {
  PLUGIN_STATUS_LABELS,
  extractPluginProposal,
  confirmDeletePlugin,
  type AiPluginProposal,
  type PluginCardState,
} from "./pluginProposal";


interface AiPanelOptions {
  root: HTMLElement;
  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
  getDocumentText(): string;
  getActiveDocument(): { id: DocumentId; rev: number } | null;
}

export interface AiPanel {
  onDockState(mode: DockMode, state: DockState): void;
  dispose(): void;
}

export function createAiPanel(opts: AiPanelOptions): AiPanel {
  const { root, log, getDocumentText, getActiveDocument } = opts;

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
  const sessionHeader = document.getElementById("ai-session");
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

  let settings: AiSettingsView | null = null;
  let skills: readonly AiSkill[] = [];
  let skillsLoaded = false;
  let skillsLoading = false;
  let consentDenied = false;
  let authError = false;
  let lastError = false;
  let busy = false;
  let stick = true;
  let popoverOpen = false;
  let wasAiExpanded = false;
  let history: AiMessage[] = [];
  let lastRequest: { display: string; content: string } | null = null;
  let disposed = false;
  const unlisteners: UnlistenFn[] = [];

  interface Pending {
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
  let pending: Pending | null = null;
  let requestToken = 0;

  let cardSeq = 0;
  const pluginCards = new Set<PluginCardState>();

  const emptyHint = document.createElement("div");
  emptyHint.className = "ai-chat-empty";
  emptyHint.setAttribute("aria-hidden", "true");
  emptyHint.textContent = "Выберите скилл выше или напишите запрос.";
  chat.appendChild(emptyHint);


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
    if (!enabled && sessionHeader) sessionHeader.hidden = true;
    input.disabled = !enabled;
    sendBtn.disabled = !enabled || busy;

    enabledInput.checked = enabled;
    enabledSub.textContent = enabled ? "включён" : "выключен";
    config.classList.toggle("is-disabled", !enabled);
    providerInput.disabled = !enabled;
    modelInput.disabled = !enabled;
    keyInput.disabled = !enabled;
    keySave.disabled = !enabled;
    keyClear.disabled = !enabled || !(settings?.has_key ?? false);
    testBtn.disabled = !enabled;

    if (settings) {
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


  const PLUGIN_SKILL_IDS: readonly string[] = ["create-plugin", "debug-plugin", "add-view"];

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
      icon.textContent = PLUGIN_SKILL_IDS.includes(skill.id) ? "🧩" : "▤";
      const title = document.createElement("span");
      title.textContent = skill.title;
      btn.append(icon, title);
      btn.addEventListener("click", () => onSkillClick(skill));
      skillsRow.appendChild(btn);
    }
  }

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

  function truncateDoc(text: string): string {
    const bytes = new TextEncoder().encode(text);
    if (bytes.length <= MAX_DOC_BYTES) return text;
    const cut = new TextDecoder().decode(bytes.subarray(0, MAX_DOC_BYTES));
    log("warn", "system", `AI: документ обрезан до ${Math.round(MAX_DOC_BYTES / 1024)} КБ`);
    return cut.replace(/\uFFFD+$/u, "");
  }


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


  function appendNotice(col: HTMLElement, text: string, kind: "info" | "error"): void {
    const note = el("div", kind === "error" ? "ai-note ai-note-error" : "ai-note");
    note.setAttribute("role", kind === "error" ? "alert" : "status");
    note.textContent = text;
    col.appendChild(note);
    if (stick) scrollToBottom();
  }

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

  async function requestDiff(
    col: HTMLElement,
    content: string,
    button: HTMLButtonElement
  ): Promise<void> {
    button.disabled = true;
    const ok = await buildDiff(col, content);
    if (!disposed && !ok) button.disabled = false;
  }

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
    code.textContent = proposal.source;

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
    reload.disabled = true;
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


  function formatSessionDate(unixSecs: number): string {
    const d = new Date(unixSecs * 1000);
    const p = (n: number): string => String(n).padStart(2, "0");
    return `${p(d.getDate())}.${p(d.getMonth() + 1)}, ${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  function showSessionHeader(updatedAt: number): void {
    if (!sessionHeader) return;
    sessionHeader.textContent = `сессия от ${formatSessionDate(updatedAt)}`;
    sessionHeader.hidden = false;
  }

  function hideSessionHeader(): void {
    if (!sessionHeader) return;
    sessionHeader.hidden = true;
    sessionHeader.textContent = "";
  }

  async function restoreHistory(): Promise<void> {
    try {
      const view = await aiHistoryLoad();
      if (disposed) return;
      if (view.messages.length === 0) {
        hideSessionHeader();
        return;
      }
      for (const msg of view.messages) {
        if (msg.role === "user") {
          appendUser(msg.content);
          history = history.concat({ role: "user", content: msg.content });
        } else if (msg.role === "assistant") {
          const { bubble, col } = appendAssistant();
          history = history.concat({ role: "assistant", content: msg.content });
          if (msg.content) void renderBubble(bubble, msg.content);
          appendAssistantActions(col, msg.content);
        }
      }
      showSessionHeader(view.updated_at);
      if (view.warning) {
        log("warn", "system", `AI: ${view.warning}`);
      }
      if (stick) scrollToBottom();
    } catch (e) {
      if (disposed) return;
      log("warn", "system", `AI: история недоступна (${errorMessage(e)})`);
    }
  }

  function saveHistory(): void {
    void aiHistorySave(history)
      .then(() => {
        if (disposed) return;
        if (history.length > 0) {
          showSessionHeader(Math.floor(Date.now() / 1000));
        } else {
          hideSessionHeader();
        }
      })
      .catch((e) => {
        if (disposed) return;
        log("warn", "system", `AI: история не сохранена (${errorMessage(e)})`);
      });
  }


  function errorActions(kind: ErrorKind, code: string): ErrorAction[] {
    const c = code.toLowerCase();
    if (kind === "auth") return [{ label: "Открыть настройки", run: openSettings }];
    if (kind === "context" || c === "ai_no_consent" || c === "ai_disabled") return [];
    return [{ label: "Повторить", run: retry }];
  }

  function send(display: string, content: string): void {
    if (!settings?.enabled || busy) return;
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
    pending = { token, row, bubble, col, content: "", messageId: null, usage: null, finishReason: null, finalized: false };
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
      p.finishReason = res.finish_reason;
      complete(p, res.usage ?? p.usage);
    } catch (e) {
      if (disposed) return;
      const p = pending;
      if (!p || p.token !== token || p.finalized) return;
      fail(p, "", errorMessage(e));
    }
  }

  async function renderBubble(bubble: HTMLElement, content: string): Promise<void> {
    try {
      const html = await renderAiMarkdown(content);
      if (disposed || !bubble.isConnected) return;
      setSanitizedHtml(bubble, html, content);
      if (stick) scrollToBottom();
    } catch (e) {
      if (disposed || !bubble.isConnected) return;
      bubble.textContent = content;
      log("warn", "system", `AI: Markdown-рендер ответа недоступен — показан текст (${errorMessage(e)})`);
    }
  }

  function complete(p: Pending, usage: AiUsage | null): void {
    p.finalized = true;
    pending = null;
    setBusy(false);
    const isEmpty = p.content === "";
    const truncated = p.finishReason === "length";
    const content = isEmpty ? (truncated ? "" : "Пустой ответ") : p.content;
    p.bubble.textContent = content;
    if (content) void renderBubble(p.bubble, content);
    if (usage) appendUsage(p.col, usage);
    if (!isEmpty || !truncated) {
      history = history.concat({ role: "assistant", content });
    }
    if (!isEmpty || !truncated) {
      appendAssistantActions(p.col, content);
    }
    if (truncated) {
      appendNotice(p.col, "Ответ оборван лимитом токенов — увеличьте max_tokens или повторите запрос.", "error");
    }
    authError = false;
    lastError = false;
    log(
      "info",
      "system",
      `AI: ответ получен${usage ? ` (prompt ${usage.prompt_tokens}, completion ${usage.completion_tokens})` : ""}`
    );
    applyState();
    saveHistory();
    if (stick) scrollToBottom();
  }

  function fail(p: Pending, code: string, message: string): void {
    p.finalized = true;
    pending = null;
    setBusy(false);
    p.row.remove();
    const c = code.toLowerCase();
    const kind = errorKind(code, message);
    if (c === "ai_no_consent") {
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
    saveHistory();
  }

  function retry(): void {
    if (busy || !settings?.enabled || !lastRequest) return;
    const { row, bubble, col } = appendAssistant();
    const token = ++requestToken;
    pending = { token, row, bubble, col, content: "", messageId: null, usage: null, finishReason: null, finalized: false };
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
    hideSessionHeader();
    void aiHistoryClear().catch((e) => {
      if (disposed) return;
      log("warn", "system", `AI: история не очищена (${errorMessage(e)})`);
    });
  }


  function askConsent(onAllowed: () => void): void {
    let accepted = false;
    const content = document.createElement("div");
    content.className = "ai-consent";
    const p = document.createElement("p");
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
      log("warn", "system", `AI: ссылка не открыта — ${errorMessage(err)}`);
    });
  };

  const onSettingsClick = (): void => toggleSettings();
  const onSettingsClose = (): void => closeSettings();
  const onKeySave = (): void => void saveKey();
  const onKeyClear = (): void => {
    keyInput.value = "";
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
  chat.addEventListener("click", onChatClick);
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
    if (p.messageId !== null && p.messageId !== message_id) return;
    p.messageId = message_id;
    p.content += delta;
    p.bubble.textContent = p.content;
    updateEmpty();
    if (stick) scrollToBottom();
  });

  subscribe<{ message_id: string; usage: AiUsage | null; finish_reason: string | null }>("ai:done", (event) => {
    const p = pending;
    if (!p) return;
    const { message_id, usage, finish_reason } = event.payload;
    if (p.messageId !== null && p.messageId !== message_id) return;
    p.messageId = message_id;
    p.usage = usage;
    p.finishReason = finish_reason ?? null;
    if (p.content) complete(p, usage);
  });

  subscribe<{ code: string; message: string }>("ai:error", (event) => {
    const p = pending;
    if (!p) return;
    fail(p, event.payload.code, event.payload.message);
  });

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


  function onDockState(mode: DockMode, state: DockState): void {
    const active = mode === "ai" && state === "expanded";
    if (active && !wasAiExpanded && settings?.enabled) {
      ensureSkills();
      input.focus();
    }
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
    chat.removeEventListener("click", onChatClick);
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
  void restoreHistory().then(() => {
    if (disposed) return;
    applyState();
  });

  renderSkills();
  applyState();
  updateEmpty();

  return { onDockState, dispose };
}
