// pluginManager.ts — менеджер плагинов в панели `#panel-plugins` (H2, Фаза 5).
//
// Контроллер показывает снимок плагинов из Rust-хоста (команда `list_plugins`):
// статус, тумблер вкл/выкл, «Перезагрузить», разрешения + notices (F38) и кнопки
// команд плагина. Тело панели собирается DOM-узлами и `textContent` — строки из
// манифеста/хоста не проходят через `innerHTML` (защита от инъекции).
//
// Состав/статусы меняются и по инициативе хоста: событие `plugins-changed` (без
// payload) заставляет перечитать снимок. Обработчики — делегированные на корне,
// поэтому перерисовка списка не оставляет висячих слушателей.

import { listen } from "@tauri-apps/api/event";
import {
  errorMessage,
  listPlugins,
  reloadPlugin,
  runPluginCommand,
  setPluginEnabled,
  type PluginInfo,
  type PluginStatus,
} from "./tauri";

/** Публичный фасад контроллера менеджера плагинов. */
export interface PluginManager {
  /** Перечитать список плагинов и перерисовать панель. */
  refresh(): Promise<void>;
  /** Снять слушатель события `plugins-changed`. */
  dispose(): void;
}

export interface PluginManagerOptions {
  /** Контейнер менеджера (`#plugin-manager` внутри `.pl-panel`). */
  root: HTMLElement;
  /** Сообщение в статусбар (ошибки IPC/действий). */
  status(msg: string): void;
}

/** Человекочитаемая подпись статуса для бейджа. */
const STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "активен",
  failed: "ошибка",
  quarantined: "карантин",
  stopped: "остановлен",
};

/** Сообщение статуса, если оно есть (`failed`), иначе `null`. */
function statusMessage(status: PluginStatus): string | null {
  return status.state === "failed" ? status.message : null;
}

function el(tag: string, className?: string): HTMLElement {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
}

export function createPluginManager(opts: PluginManagerOptions): PluginManager {
  const { root, status } = opts;
  let disposed = false;
  // Защита от гонок: ответ устаревшего `list_plugins` не перетирает свежий.
  let refreshSeq = 0;

  function renderBadge(info: PluginInfo): HTMLElement {
    const badge = el("span", `pl-badge ${info.status.state}`);
    badge.textContent = STATUS_LABELS[info.status.state];
    const message = statusMessage(info.status);
    if (message) badge.title = message;
    return badge;
  }

  /** Один плагин: голова (тумблер/имя/бейдж), разрешения, notices, действия. */
  function renderItem(info: PluginInfo): HTMLElement {
    const item = el("div", "pl-item");
    item.dataset.plugin = info.id;

    const head = el("div", "pl-item-head");

    const toggleLabel = el("label", "pl-toggle");
    toggleLabel.title = info.enabled ? "Выключить плагин" : "Включить плагин";
    const toggle = document.createElement("input");
    toggle.type = "checkbox";
    toggle.className = "pl-enabled";
    toggle.checked = info.enabled;
    toggle.setAttribute("aria-label", `${info.enabled ? "Выключить" : "Включить"} плагин ${info.id}`);
    toggleLabel.append(toggle);

    const name = el("span", "pl-name");
    name.textContent = info.id;
    name.title = info.id;

    head.append(toggleLabel, name, renderBadge(info));

    const children: HTMLElement[] = [head];

    // Разрешения манифеста — chips.
    const permissions = info.permissions ?? [];
    if (permissions.length > 0) {
      const perms = el("div", "pl-perms");
      for (const permission of permissions) {
        const chip = el("span", "pl-perm");
        chip.textContent = permission;
        perms.append(chip);
      }
      children.push(perms);
    }

    // F38: формулировки границы изоляции/доступа к документу.
    const notices = info.notices ?? [];
    for (const notice of notices) {
      const node = el("div", "pl-notice");
      node.textContent = notice;
      children.push(node);
    }
    // Статус `failed` несёт сообщение — показываем его рядом с бейджем.
    const failMessage = statusMessage(info.status);
    if (failMessage) {
      const node = el("div", "pl-notice");
      node.textContent = failMessage;
      children.push(node);
    }

    const actions = el("div", "pl-actions");
    const reload = document.createElement("button");
    reload.type = "button";
    reload.className = "pl-reload";
    reload.textContent = "Перезагрузить";
    reload.disabled = !info.enabled;
    actions.append(reload);

    for (const command of info.commands ?? []) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "pl-cmd";
      btn.dataset.command = command.id;
      btn.textContent = "Выполнить";
      btn.title = command.keybinding
        ? `${command.title} (${command.keybinding})`
        : command.title;
      btn.setAttribute("aria-label", `Выполнить: ${command.title}`);
      actions.append(btn);
    }
    children.push(actions);

    item.append(...children);
    return item;
  }

  function render(list: PluginInfo[]): void {
    if (list.length === 0) {
      const empty = el("div", "side-empty pl-empty");
      empty.textContent = "Плагины не найдены";
      root.replaceChildren(empty);
      return;
    }
    root.replaceChildren(...list.map(renderItem));
  }

  async function refresh(): Promise<void> {
    const seq = ++refreshSeq;
    let list: PluginInfo[];
    try {
      list = await listPlugins();
    } catch (e) {
      if (!disposed && seq === refreshSeq) status(`Плагины: ${errorMessage(e)}`);
      return;
    }
    // Ответ устарел (пришёл refresh новее) или контроллер уже освобождён.
    if (disposed || seq !== refreshSeq) return;
    render(list);
  }

  async function handleToggle(input: HTMLInputElement): Promise<void> {
    const id = input.closest<HTMLElement>(".pl-item")?.dataset.plugin;
    if (!id) return;
    input.disabled = true; // защита от повторного клика до перерисовки
    try {
      await setPluginEnabled(id, input.checked);
    } catch (e) {
      status(`Плагин ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }

  async function handleReload(btn: HTMLButtonElement): Promise<void> {
    const id = btn.closest<HTMLElement>(".pl-item")?.dataset.plugin;
    if (!id) return;
    btn.disabled = true;
    try {
      await reloadPlugin(id);
      status(`Плагин ${id}: перезагружен`);
    } catch (e) {
      status(`Плагин ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }

  async function handleCommand(btn: HTMLButtonElement): Promise<void> {
    const commandId = btn.dataset.command;
    if (!commandId) return;
    btn.disabled = true;
    try {
      await runPluginCommand(commandId);
    } catch (e) {
      status(`Команда ${commandId}: ${errorMessage(e)}`);
    }
    await refresh();
  }

  function onChange(e: Event): void {
    const input = e.target as HTMLInputElement | null;
    if (!input || !input.classList.contains("pl-enabled")) return;
    void handleToggle(input);
  }

  function onClick(e: MouseEvent): void {
    const target = e.target as HTMLElement | null;
    const reload = target?.closest<HTMLButtonElement>(".pl-reload");
    if (reload) {
      void handleReload(reload);
      return;
    }
    const command = target?.closest<HTMLButtonElement>(".pl-cmd");
    if (command) void handleCommand(command);
  }

  root.addEventListener("change", onChange);
  root.addEventListener("click", onClick);

  // Ошибку установки слушателя глушим: в обычном (не плагинном) запуске событий не будет.
  const unlistenPromise = listen("plugins-changed", () => {
    void refresh();
  }).catch(() => null);

  return {
    refresh,

    dispose(): void {
      disposed = true;
      root.removeEventListener("change", onChange);
      root.removeEventListener("click", onClick);
      void unlistenPromise.then((fn) => fn?.());
    },
  };
}
