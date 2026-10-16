






import {
  aiCreatePlugin,
  aiDeletePlugin,
  aiUpdatePlugin,
  aiValidatePlugin,
  errorMessage,
  reloadPlugin,
} from "../../core/tauri";
import { actionButton, el } from "./dom";
import { errorCode } from "./errors";
import {
  PLUGIN_STATUS_LABELS,
  confirmDeletePlugin,
  extractPluginProposal,
  type AiPluginProposal,
  type PluginCardState,
} from "./pluginProposal";
import type { AiPanelState } from "./state";

export interface PluginCardApi {
  appendAssistantActions(col: HTMLElement, content: string): void;
  

  clearCards(): void;
}

export function createPluginCard(
  state: AiPanelState,
  deps: {
    requestDiff(col: HTMLElement, content: string, button: HTMLButtonElement): Promise<void>;
    scrollToBottom(): void;
  }
): PluginCardApi {
  function renderPluginCard(col: HTMLElement, proposal: AiPluginProposal): void {
    const id = ++state.cardSeq;
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
    if (state.stick) deps.scrollToBottom();

    let exists = false;
    const cardState: PluginCardState = {
      id: proposal.id,
      isInstalled: () => exists,
      installed(label, stateClass) {
        markInstalled(label, stateClass);
      },
      deleted() {
        markDeleted();
      },
      remove() {
        card.remove();
        state.pluginCards.delete(cardState);
      },
    };
    state.pluginCards.add(cardState);

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
      state.pluginCards.delete(cardState);
    }

    async function validate(): Promise<void> {
      status.textContent = "Проверка…";
      try {
        const res = await aiValidatePlugin(proposal.id, proposal.manifest, proposal.source);
        if (state.disposed) return;
        status.classList.toggle("is-ok", res.ok);
        status.classList.toggle("is-error", !res.ok);
        status.textContent = res.ok
          ? "Проверка: ✓ пройдена"
          : `Проверка не пройдена: ${res.errors.join("; ") || "неизвестная ошибка"}`;
      } catch (e) {
        if (state.disposed) return;
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
        if (state.disposed) return;
        markInstalled(PLUGIN_STATUS_LABELS[info.status.state], info.status.state);
        status.classList.add("is-ok");
        status.textContent = created ? "Плагин создан." : "Плагин обновлён.";
      } catch (e) {
        if (state.disposed) return;
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
        if (state.disposed) return;
        status.classList.remove("is-error");
        status.textContent = "Плагин перезагружен.";
      } catch (e) {
        if (state.disposed) return;
        status.classList.add("is-error");
        status.textContent = `Не удалось перезагрузить: ${errorMessage(e)}`;
      } finally {
        if (!state.disposed) reload.disabled = false;
      }
    }

    async function removePluginCard(): Promise<void> {
      const confirmed = await confirmDeletePlugin(proposal.id);
      if (!confirmed || state.disposed) return;
      remove.disabled = true;
      try {
        await aiDeletePlugin(proposal.id);
        if (state.disposed) return;
        markDeleted();
      } catch (e) {
        if (state.disposed) return;
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
    apply.addEventListener("click", () => void deps.requestDiff(col, content, apply));
    actions.appendChild(apply);
    col.appendChild(actions);

    const proposal = extractPluginProposal(content);
    if (proposal) renderPluginCard(col, proposal);
  }

  function clearCards(): void {
    for (const card of Array.from(state.pluginCards)) {
      card.remove();
    }
  }

  return { appendAssistantActions, clearCards };
}