











import { listen } from "@tauri-apps/api/event";
import {
  errorMessage,
  listPlugins,
  reloadPlugin,
  removePlugin,
  runPluginCommand,
  setPluginEnabled,
  setPluginPermissions,
  setPluginSetting,
  type PluginInfo,
  type PluginSettingProperty,
} from "../../core/tauri";
import { openDialog, type Dialog } from "../../ui/dialog";
import { ownerPluginId } from "../pluginTarget";
import { pluginColor } from "../pluginColor";
import type { Json } from "../../views/viewRegistry";
import {
  STATUS_LABELS,
  permissionLabel,
  statusMessage,
  stateHint,
  documentBoundaryNotice,
} from "./labels";
import { el, bodyId } from "./dom";
import { shieldIcon } from "./icons";
import { settingValue, renderSettingRow } from "./settings";
import {
  renderIdentity,
  renderInfoButton,
  renderInfoPopover,
  renderExpandButton,
} from "./render";


interface PluginManager {
  refresh(): Promise<void>;
  reloadForDocument(path: string | null): Promise<void>;
  dispose(): void;
}

interface PluginManagerOptions {
  root: HTMLElement;
  status(msg: string): void;
  onPlugins?(list: PluginInfo[]): void;
}

export function createPluginManager(opts: PluginManagerOptions): PluginManager {
  const { root, status, onPlugins } = opts;
  let disposed = false;
  let refreshSeq = 0;
  let lastList: PluginInfo[] = [];
  let reloading: Promise<void> | null = null;
  let openInfoButton: HTMLButtonElement | null = null;
  const infoPopovers = new WeakMap<HTMLButtonElement, HTMLElement>();
  const settingEnums = new WeakMap<HTMLSelectElement, Json[]>();
  const expanded = new Set<string>();

  function renderBadge(info: PluginInfo): HTMLElement {
    const badge = el("span", `pl-badge ${info.status.state}`);
    badge.textContent = STATUS_LABELS[info.status.state];
    badge.title = statusMessage(info.status) ?? STATUS_LABELS[info.status.state];
    return badge;
  }

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
        `${checkbox.checked ? "Снять" : "Выдать"} разрешение ${permissionLabel(permission)}`
      );
      const name = el("span", "pl-perm-name");
      name.textContent = permissionLabel(permission);
      const token = el("span", "pl-perm-token");
      token.textContent = permission;
      token.title = permission;
      const state = el("span", "pl-perm-state");
      state.textContent = checkbox.checked ? "выдано" : "отклонено";
      row.append(checkbox, name, token, state);
      section.append(row);
    }

    const missing = requested.filter((permission) => !granted.has(permission));
    if (missing.length > 0) {
      const hint = el("div", "pl-consent-hint");
      hint.textContent = `Не выдано: ${missing.join(", ")}. Плагин не сможет их использовать.`;
      section.append(hint);
    }

    const boundaryNotice = documentBoundaryNotice(info);
    if (boundaryNotice) {
      const boundary = el("div", "pl-boundary");
      const icon = el("span", "ic");
      icon.append(shieldIcon());
      const text = el("span", "pl-boundary-text");
      text.textContent = boundaryNotice;
      boundary.append(icon, text);
      section.append(boundary);
    }

    return section;
  }

  function settingControl(
    prop: PluginSettingProperty,
    current: unknown
  ): HTMLInputElement | HTMLSelectElement | null {
    const options = prop.enum;
    if (Array.isArray(options) && options.length > 0) {
      const select = document.createElement("select");
      select.className = "pl-setting-control";
      options.forEach((value, index) => {
        const option = document.createElement("option");
        option.value = String(index);
        option.textContent = String(value);
        if (value === current) option.selected = true;
        select.append(option);
      });
      settingEnums.set(select, options);
      return select;
    }

    const input = document.createElement("input");
    input.className = "pl-setting-control";
    if (prop.type === "boolean") {
      input.type = "checkbox";
      input.checked = current === true;
    } else if (prop.type === "integer" || prop.type === "number") {
      input.type = "number";
      if (prop.type === "integer") input.step = "1";
      if (typeof current === "number") input.value = String(current);
    } else if (prop.type === "string") {
      input.type = "text";
      if (typeof current === "string") input.value = current;
    } else {
      return null;
    }
    return input;
  }

  function renderSettings(info: PluginInfo): HTMLElement | null {
    const properties = info.settings_schema?.properties;
    if (!properties) return null;

    const rows: HTMLElement[] = [];
    for (const [key, prop] of Object.entries(properties)) {
      const control = settingControl(prop, settingValue(info, key, prop));
      if (control) rows.push(renderSettingRow(key, prop, control));
    }
    if (rows.length === 0) return null;

    const section = el("div", "pl-settings");
    const title = el("div", "pl-settings-title");
    title.textContent = "Настройки";
    section.append(title, ...rows);
    return section;
  }

  function setSettingsBusy(item: HTMLElement, busy: boolean): void {
    for (const control of item.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
      ".pl-setting-control"
    )) {
      control.disabled = busy;
    }
  }

  function readSettingValue(
    control: HTMLInputElement | HTMLSelectElement
  ): Json | undefined {
    if (control instanceof HTMLSelectElement) {
      const options = settingEnums.get(control);
      const index = Number(control.value);
      if (!options || !Number.isInteger(index) || index < 0 || index >= options.length) {
        return undefined;
      }
      return options[index];
    }
    if (control.type === "checkbox") return control.checked;
    if (control.type === "number") {
      const value = Number(control.value);
      return Number.isFinite(value) ? value : undefined;
    }
    return control.value;
  }

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

    const open = expanded.has(info.id);

    head.append(toggleLabel, renderIdentity(info));
    const infoPopover = info.notices.length > 0 ? renderInfoPopover(info) : null;
    if (infoPopover) {
      const infoButton = renderInfoButton(info);
      infoPopovers.set(infoButton, infoPopover);
      head.append(infoButton);
    }
    head.append(renderBadge(info), renderExpandButton(info, open));
    if (infoPopover) head.append(infoPopover);

    const body = el("div", "pl-item-body");
    body.id = bodyId(info.id);

    const failMessage = statusMessage(info.status);
    if (failMessage) {
      const node = el("div", "pl-notice");
      node.textContent = failMessage;
      body.append(node);
    }

    const hintText = stateHint(info.status);
    if (hintText) {
      const hint = el("div", "pl-state-hint");
      hint.textContent = hintText;
      body.append(hint);
    }

    const actions = el("div", "pl-actions");
    const reload = document.createElement("button");
    reload.type = "button";
    reload.className = "pl-reload";
    reload.textContent = "Перезагрузить";
    reload.disabled = !info.enabled;
    actions.append(reload);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "pl-remove";
    remove.textContent = "Удалить";
    actions.append(remove);

    for (const command of info.commands) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "pl-cmd";
      btn.dataset.command = command.id;
      btn.textContent = command.title;
      btn.title = command.keybinding
        ? `${command.title} (${command.keybinding})`
        : command.title;
      btn.setAttribute("aria-label", `Выполнить: ${command.title}`);
      actions.append(btn);
    }
    body.append(actions);

    const consent = renderConsent(info);
    if (consent) body.append(consent);

    const settings = renderSettings(info);
    if (settings) body.append(settings);

    item.classList.toggle("open", open);
    item.append(head, body);
    return item;
  }

  function toggleInfoPopover(button: HTMLButtonElement): void {
    const popover = infoPopovers.get(button);
    if (!popover) return;
    const wasOpen = openInfoButton === button;
    closeAllInfoPopovers();
    if (wasOpen) return;
    popover.hidden = false;
    popover.setAttribute("aria-hidden", "false");
    button.setAttribute("aria-expanded", "true");
    openInfoButton = button;
  }

  function closeAllInfoPopovers(): HTMLButtonElement | null {
    const button = openInfoButton;
    if (!button) return null;
    const popover = infoPopovers.get(button);
    if (popover) {
      popover.hidden = true;
      popover.setAttribute("aria-hidden", "true");
    }
    button.setAttribute("aria-expanded", "false");
    openInfoButton = null;
    return button;
  }

  function toggleExpand(btn: HTMLButtonElement): void {
    const id = btn.dataset.expand;
    const item = btn.closest<HTMLElement>(".pl-item");
    if (!id || !item) return;
    const open = !expanded.has(id);
    if (open) expanded.add(id);
    else expanded.delete(id);
    item.classList.toggle("open", open);
    btn.setAttribute("aria-expanded", String(open));
    btn.setAttribute("aria-label", `${open ? "Свернуть" : "Развернуть"} карточку ${id}`);
  }

  function render(list: PluginInfo[]): void {
    closeAllInfoPopovers();
    const ids = new Set(list.map((info) => info.id));
    for (const id of expanded) {
      if (!ids.has(id)) expanded.delete(id);
    }
    if (list.length === 0) {
      const empty = el("div", "side-empty pl-empty");
      const title = el("div", "pl-empty-title");
      title.textContent = "Плагины не найдены";
      const hint = el("div", "pl-empty-hint");
      hint.append("Положите плагин в ");
      const code = document.createElement("code");
      code.textContent = "%APPDATA%/darmark/plugins/<id>/";
      hint.append(code);
      hint.append(" (plugin.json + main.lua) и перезапустите ПЕНАЛ.");
      empty.append(title, hint);
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
    if (disposed || seq !== refreshSeq) return;
    lastList = list;
    render(list);
    onPlugins?.(list);
  }

  async function reloadForDocument(path: string | null): Promise<void> {
    if (reloading) return reloading;
    reloading = doReload(path).finally(() => {
      reloading = null;
    });
    return reloading;
  }

  async function doReload(path: string | null): Promise<void> {
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
      for (const id of ids) await reloadPlugin(id);
      status(`Плагины: перезагружено (${ids.length})`);
    }
    await refresh();
  }

  async function handleToggle(input: HTMLInputElement): Promise<void> {
    const id = input.closest<HTMLElement>(".pl-item")?.dataset.plugin;
    if (!id) return;
    input.disabled = true;
    try {
      await setPluginEnabled(id, input.checked);
    } catch (e) {
      status(`Плагин ${id}: ${errorMessage(e)}`);
    }
    await refresh();
  }

  function setConsentBusy(item: HTMLElement, busy: boolean): void {
    for (const cb of item.querySelectorAll<HTMLInputElement>("input.pl-grant")) {
      cb.disabled = busy;
    }
    for (const btn of item.querySelectorAll<HTMLButtonElement>(".pl-perms-bulk button")) {
      btn.disabled = busy;
    }
  }

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

  async function handleSettingChange(
    control: HTMLInputElement | HTMLSelectElement
  ): Promise<void> {
    const item = control.closest<HTMLElement>(".pl-item");
    const id = item?.dataset.plugin;
    const key = control.dataset.settingKey;
    if (!item || !id || !key) return;
    const value = readSettingValue(control);
    if (value === undefined) return;
    setSettingsBusy(item, true);
    try {
      await setPluginSetting(id, key, value);
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

  function confirmRemovePlugin(id: string): Promise<boolean> {
    return new Promise((resolve) => {
      const content = el("div", "pl-confirm-remove");
      const text = el("p", "pl-confirm-text");
      text.textContent = `Плагин «${id}» будет удалён безвозвратно. Его файлы и настройки будут стёрты.`;
      content.append(text);

      const buttons = el("div", "pl-confirm-buttons");
      const cancelBtn = document.createElement("button");
      cancelBtn.type = "button";
      cancelBtn.className = "pl-confirm-cancel";
      cancelBtn.textContent = "Отмена";
      const confirmBtn = document.createElement("button");
      confirmBtn.type = "button";
      confirmBtn.className = "pl-confirm-ok";
      confirmBtn.textContent = "Удалить";
      buttons.append(cancelBtn, confirmBtn);
      content.append(buttons);

      let dialog: Dialog | null = null;
      const cleanup = (result: boolean) => {
        dialog?.close();
        resolve(result);
      };

      cancelBtn.addEventListener("click", () => cleanup(false));
      confirmBtn.addEventListener("click", () => cleanup(true));

      dialog = openDialog({
        title: "Удаление плагина",
        content,
        variant: "center",
        initialFocus: cancelBtn,
        onClose: () => resolve(false),
      });
    });
  }

  async function handleRemove(btn: HTMLButtonElement): Promise<void> {
    const id = btn.closest<HTMLElement>(".pl-item")?.dataset.plugin;
    if (!id) return;
    const confirmed = await confirmRemovePlugin(id);
    if (!confirmed) return;
    btn.disabled = true;
    try {
      await removePlugin(id);
      status(`Плагин ${id}: удалён`);
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
    const target = e.target as HTMLElement | null;
    if (!target) return;
    if (target.classList.contains("pl-enabled")) {
      void handleToggle(target as HTMLInputElement);
      return;
    }
    if (target.classList.contains("pl-grant")) {
      void handleGrant(target as HTMLInputElement);
      return;
    }
    if (target.classList.contains("pl-setting-control")) {
      void handleSettingChange(target as HTMLInputElement | HTMLSelectElement);
    }
  }

  function onClick(e: MouseEvent): void {
    const target = e.target as HTMLElement | null;
    const info = target?.closest<HTMLButtonElement>(".pl-info");
    if (info) {
      toggleInfoPopover(info);
      return;
    }
    const expand = target?.closest<HTMLButtonElement>(".pl-item-expand");
    if (expand) {
      toggleExpand(expand);
      return;
    }
    const reload = target?.closest<HTMLButtonElement>(".pl-reload");
    if (reload) {
      void handleReload(reload);
      return;
    }
    const remove = target?.closest<HTMLButtonElement>(".pl-remove");
    if (remove) {
      void handleRemove(remove);
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

  function onDocumentClick(e: MouseEvent): void {
    const button = openInfoButton;
    if (!button) return;
    const popover = infoPopovers.get(button);
    const target = e.target as Node | null;
    if (target && (button.contains(target) || popover?.contains(target))) return;
    closeAllInfoPopovers();
  }

  function onDocumentKeydown(e: KeyboardEvent): void {
    if (e.key !== "Escape" || !openInfoButton) return;
    e.preventDefault();
    e.stopPropagation();
    closeAllInfoPopovers()?.focus();
  }

  root.addEventListener("change", onChange);
  root.addEventListener("click", onClick);
  document.addEventListener("click", onDocumentClick);
  document.addEventListener("keydown", onDocumentKeydown);

  const unlistenPromise = listen("plugins-changed", () => {
    void refresh();
  }).catch(() => null);

  return {
    refresh,
    reloadForDocument,

    dispose(): void {
      disposed = true;
      expanded.clear();
      closeAllInfoPopovers();
      root.removeEventListener("change", onChange);
      root.removeEventListener("click", onClick);
      document.removeEventListener("click", onDocumentClick);
      document.removeEventListener("keydown", onDocumentKeydown);
      void unlistenPromise.then((fn) => fn?.());
    },
  };
}
