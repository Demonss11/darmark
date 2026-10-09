// pluginManager.ts — менеджер плагинов в панели `#panel-plugins` (H2, Фаза 5).
//
// Контроллер показывает снимок плагинов из Rust-хоста (команда `list_plugins`):
// статус, тумблер вкл/выкл, «Перезагрузить», инлайн-согласие на permissions
// (§11.1 п.4), notices (F38) и кнопки команд плагина. Тело панели собирается
// DOM-узлами и `textContent` — строки из манифеста/хоста не проходят через
// `innerHTML` (защита от инъекции).
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
  setPluginPermissions,
  type PluginInfo,
  type PluginStatus,
} from "./tauri";
import { ownerPluginId } from "./pluginTarget";
import { pluginColor } from "./pluginColor";

/** Публичный фасад контроллера менеджера плагинов. */
export interface PluginManager {
  /** Перечитать список плагинов и перерисовать панель. */
  refresh(): Promise<void>;
  /**
   * `Ctrl+R`: перезагрузить плагин-владельца открытого `.lua`-документа, иначе —
   * все включённые плагины (fallback). Нет включённых — no-op с сообщением.
   * Ошибки IPC пробрасываются наружу (обрабатывает вызывающий).
   */
  reloadForDocument(path: string | null): Promise<void>;
  /** Снять слушатель события `plugins-changed`. */
  dispose(): void;
}

export interface PluginManagerOptions {
  /** Контейнер менеджера (`#plugin-manager` внутри `.pl-panel`). */
  root: HTMLElement;
  /** Сообщение в статусбар (ошибки IPC/действий). */
  status(msg: string): void;
  /** Свежий снимок плагинов после успешного `list_plugins` (для статусбара). */
  onPlugins?(list: PluginInfo[]): void;
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
  const { root, status, onPlugins } = opts;
  let disposed = false;
  // Защита от гонок: ответ устаревшего `list_plugins` не перетирает свежий.
  let refreshSeq = 0;
  // Последний успешный снимок — источник списка включённых для `Ctrl+R`.
  let lastList: PluginInfo[] = [];
  // Идущая перезагрузка: конкурентные Ctrl+R переиспользуют её (см. reloadForDocument).
  let reloading: Promise<void> | null = null;

  function renderBadge(info: PluginInfo): HTMLElement {
    const badge = el("span", `pl-badge ${info.status.state}`);
    badge.textContent = STATUS_LABELS[info.status.state];
    const message = statusMessage(info.status);
    if (message) badge.title = message;
    return badge;
  }

  /**
   * Инлайн-секция согласия: по чекбоксу на каждое запрошенное право. `checked` —
   * право реально выдано (`granted_permissions`), состояние дублируется словом
   * (не только цветом). Согласие — реальный гейт эффективных прав
   * (`manifest ∩ granted`, deny-by-default, §11.1 п.4). `null`, если плагин
   * ничего не запрашивает.
   */
  function renderConsent(info: PluginInfo): HTMLElement | null {
    const requested = info.permissions;
    if (requested.length === 0) return null;
    const granted = new Set(info.granted_permissions);
    const grantedCount = requested.filter((permission) => granted.has(permission)).length;

    const section = el("div", "pl-perms");

    const head = el("div", "pl-perms-head");
    const title = el("span", "pl-perms-title");
    title.textContent = `Доступ: ${grantedCount} из ${requested.length}`;

    const bulk = el("div", "pl-perms-bulk");
    const grantAll = document.createElement("button");
    grantAll.type = "button";
    grantAll.className = "pl-grant-all";
    grantAll.dataset.grant = "all";
    grantAll.textContent = "Выдать все";
    grantAll.disabled = grantedCount === requested.length;
    const grantNone = document.createElement("button");
    grantNone.type = "button";
    grantNone.className = "pl-grant-none";
    grantNone.dataset.grant = "none";
    grantNone.textContent = "Снять все";
    grantNone.disabled = grantedCount === 0;
    bulk.append(grantAll, grantNone);
    head.append(title, bulk);
    section.append(head);

    for (const permission of requested) {
      const row = el("label", "pl-perm");
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.className = "pl-grant";
      checkbox.dataset.permission = permission;
      checkbox.checked = granted.has(permission);
      checkbox.setAttribute(
        "aria-label",
        `${checkbox.checked ? "Снять" : "Выдать"} разрешение ${permission}`
      );
      const name = el("span", "pl-perm-name");
      name.textContent = permission;
      const state = el("span", "pl-perm-state");
      state.textContent = checkbox.checked ? "выдано" : "отклонено";
      row.append(checkbox, name, state);
      section.append(row);
    }

    // Подсказка, когда часть запрошенных прав отклонена (deny-by-default не очевиден).
    const missing = requested.filter((permission) => !granted.has(permission));
    if (missing.length > 0) {
      const hint = el("div", "pl-consent-hint");
      hint.textContent = `Не выдано: ${missing.join(", ")}. Плагин не сможет их использовать.`;
      section.append(hint);
    }

    return section;
  }

  /** Один плагин: голова (тумблер/имя/бейдж), согласие, notices, действия. */
  function renderItem(info: PluginInfo): HTMLElement {
    const item = el("div", "pl-item");
    item.dataset.plugin = info.id;
    item.style.setProperty("--pc", pluginColor(info.id));

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

    // Инлайн-секция согласия: запросы манифеста + фактически выданные права.
    const consent = renderConsent(info);
    if (consent) children.push(consent);

    // F38: формулировки границы изоляции/доступа к документу.
    const notices = info.notices;
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

    for (const command of info.commands) {
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
    lastList = list;
    render(list);
    onPlugins?.(list);
  }

  /**
   * `Ctrl+R`: владелец открытого `.lua` (`<plugins>/<id>/<entry>.lua`) —
   * перезагружаем только его; иначе — все включённые (fallback). Reload не
   * разрушителен: карантин и согласие сохраняются.
   */
  async function reloadForDocument(path: string | null): Promise<void> {
    // Защита от конкурентных нажатий Ctrl+R: пока идёт перезагрузка, повторный
    // вызов переиспользует тот же промис (иначе child-процессы рестартуют дважды).
    if (reloading) return reloading;
    reloading = doReload(path).finally(() => {
      reloading = null;
    });
    return reloading;
  }

  async function doReload(path: string | null): Promise<void> {
    // Снимок может быть пуст — refresh ещё не прошёл (например, старт приложения).
    const source = lastList.length > 0 ? lastList : await listPlugins();
    const enabled = source.filter((info) => info.enabled);
    if (enabled.length === 0) {
      status("Плагины: нет включённых");
      return;
    }
    const ids = enabled.map((info) => info.id);
    const owner = ownerPluginId(path, ids);
    if (owner) {
      await reloadPlugin(owner);
      status(`Плагин ${owner}: перезагружен`);
    } else {
      // Последовательно: не плодим одновременные перезапуски child-процессов.
      for (const id of ids) await reloadPlugin(id);
      status(`Плагины: перезагружено (${ids.length})`);
    }
    await refresh();
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

  /** Дизейблит контролы согласия плагина на время IPC (защита от повторного клика). */
  function setConsentBusy(item: HTMLElement, busy: boolean): void {
    for (const cb of item.querySelectorAll<HTMLInputElement>("input.pl-grant")) {
      cb.disabled = busy;
    }
    for (const btn of item.querySelectorAll<HTMLButtonElement>(".pl-perms-bulk button")) {
      btn.disabled = busy;
    }
  }

  /** Собирает **весь** набор отмеченных прав `.pl-item` и фиксирует согласие. */
  async function handleGrant(input: HTMLInputElement): Promise<void> {
    const item = input.closest<HTMLElement>(".pl-item");
    const id = item?.dataset.plugin;
    if (!item || !id) return;
    const granted = Array.from(item.querySelectorAll<HTMLInputElement>("input.pl-grant"))
      .filter((cb) => cb.checked)
      .map((cb) => cb.dataset.permission)
      .filter((permission): permission is string => !!permission);
    setConsentBusy(item, true);
    try {
      await setPluginPermissions(id, granted);
    } catch (e) {
      status(`Плагин ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }

  /** Групповые кнопки «Выдать все»/«Снять все» для согласия плагина. */
  async function handleGrantBulk(btn: HTMLButtonElement): Promise<void> {
    const item = btn.closest<HTMLElement>(".pl-item");
    const id = item?.dataset.plugin;
    const mode = btn.dataset.grant;
    if (!item || !id || !mode) return;
    const granted =
      mode === "all"
        ? Array.from(item.querySelectorAll<HTMLInputElement>("input.pl-grant"))
            .map((cb) => cb.dataset.permission)
            .filter((permission): permission is string => !!permission)
        : [];
    setConsentBusy(item, true);
    try {
      await setPluginPermissions(id, granted);
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
    if (!input) return;
    if (input.classList.contains("pl-enabled")) {
      void handleToggle(input);
      return;
    }
    if (input.classList.contains("pl-grant")) void handleGrant(input);
  }

  function onClick(e: MouseEvent): void {
    const target = e.target as HTMLElement | null;
    const reload = target?.closest<HTMLButtonElement>(".pl-reload");
    if (reload) {
      void handleReload(reload);
      return;
    }
    const command = target?.closest<HTMLButtonElement>(".pl-cmd");
    if (command) {
      void handleCommand(command);
      return;
    }
    const bulk = target?.closest<HTMLButtonElement>(".pl-perms-bulk button");
    if (bulk) void handleGrantBulk(bulk);
  }

  root.addEventListener("change", onChange);
  root.addEventListener("click", onClick);

  // Ошибку установки слушателя глушим: в обычном (не плагинном) запуске событий не будет.
  const unlistenPromise = listen("plugins-changed", () => {
    void refresh();
  }).catch(() => null);

  return {
    refresh,
    reloadForDocument,

    dispose(): void {
      disposed = true;
      root.removeEventListener("change", onChange);
      root.removeEventListener("click", onClick);
      void unlistenPromise.then((fn) => fn?.());
    },
  };
}
