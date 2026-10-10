// pluginManager.ts — менеджер плагинов в дровере `#drawer-body` (H2, Фаза 5).
//
// Контроллер показывает снимок плагинов из Rust-хоста (команда `list_plugins`):
// статус, тумблер вкл/выкл, «Перезагрузить», инлайн-согласие на permissions
// (§11.1 п.4) и кнопки команд плагина. Тело панели собирается
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
  /** Контейнер менеджера (`#plugin-manager` внутри `#drawer-body`). */
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

/** Уникальный id поповера информации — связывает кнопку (`aria-controls`) с содержимым. */
function infoPopoverId(pluginId: string): string {
  return `pl-info-pop-${pluginId}`;
}

/** Уникальный id тела карточки — связывает шеврон (`aria-controls`) с аккордеоном. */
function bodyId(pluginId: string): string {
  return `pl-body-${pluginId}`;
}

/**
 * Инфо-иконка (круг с «i») как inline SVG: CSP `default-src 'self'` разрешает
 * inline-контент, и, в отличие от Unicode `ⓘ`, рендер не зависит от системных шрифтов.
 */
function infoIcon(): SVGSVGElement {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("width", "14");
  svg.setAttribute("height", "14");
  svg.setAttribute("aria-hidden", "true");

  const circle = document.createElementNS(NS, "circle");
  circle.setAttribute("cx", "8");
  circle.setAttribute("cy", "8");
  circle.setAttribute("r", "7");
  circle.setAttribute("fill", "none");
  circle.setAttribute("stroke", "currentColor");
  circle.setAttribute("stroke-width", "1.5");

  const text = document.createElementNS(NS, "text");
  text.setAttribute("x", "8");
  text.setAttribute("y", "11.5");
  text.setAttribute("text-anchor", "middle");
  text.setAttribute("font-size", "10");
  text.setAttribute("fill", "currentColor");
  text.setAttribute("font-family", "inherit");
  text.textContent = "i";

  svg.append(circle, text);
  return svg;
}

/**
 * Шеврон раскрытия как inline SVG (CSP `default-src 'self'` без внешних ассетов;
 * Unicode-стрелка зависела бы от системного шрифта). Поворот задаётся CSS.
 */
function chevronIcon(): SVGSVGElement {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");

  const polyline = document.createElementNS(NS, "polyline");
  polyline.setAttribute("points", "9 6 15 12 9 18");

  svg.append(polyline);
  return svg;
}

/** Кнопка-иконка «инфо» о границах изоляции плагина (в `.pl-item-head`). */
function renderInfoButton(info: PluginInfo): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "pl-info";
  btn.setAttribute("aria-expanded", "false");
  btn.setAttribute("aria-controls", infoPopoverId(info.id));
  btn.setAttribute("aria-label", `Информация о границах изоляции плагина ${info.id}`);
  btn.append(infoIcon());
  return btn;
}

/**
 * Скрытый поповер с формулировками границы изоляции. Текст — только `textContent`
 * (строки приходят из Rust-ядра и не проходят через `innerHTML`).
 */
function renderInfoPopover(info: PluginInfo): HTMLElement {
  const pop = el("div", "pl-info-pop");
  pop.id = infoPopoverId(info.id);
  pop.setAttribute("role", "tooltip");
  pop.setAttribute("aria-hidden", "true");
  pop.hidden = true;
  for (const notice of info.notices) {
    const line = el("div", "pl-notice");
    line.textContent = notice;
    pop.append(line);
  }
  return pop;
}

/** Кнопка-шеврон раскрытия карточки (в конце `.pl-item-head`, перед поповером). */
function renderExpandButton(info: PluginInfo, open: boolean): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "pl-item-expand";
  btn.dataset.expand = info.id;
  btn.setAttribute("aria-expanded", String(open));
  btn.setAttribute("aria-controls", bodyId(info.id));
  btn.setAttribute("aria-label", `${open ? "Свернуть" : "Развернуть"} карточку ${info.id}`);
  btn.append(chevronIcon());
  return btn;
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
  // Кнопка открытого поповера информации — их не больше одного (см. toggleInfoPopover).
  let openInfoButton: HTMLButtonElement | null = null;
  // Связь кнопки с её поповером по DOM-узлу — без запросов к document и без утечек.
  const infoPopovers = new WeakMap<HTMLButtonElement, HTMLElement>();
  // Раскрытые карточки — вне DOM: `render` пересоздаёт узлы (`replaceChildren`),
  // а раскрытие должно переживать перерисовку после действий (toggle/grant/reload).
  const expanded = new Set<string>();

  function renderBadge(info: PluginInfo): HTMLElement {
    const badge = el("span", `pl-badge ${info.status.state}`);
    badge.textContent = STATUS_LABELS[info.status.state];
    // Тултип всегда: причина для `failed`, иначе — человекочитаемый статус.
    badge.title = statusMessage(info.status) ?? STATUS_LABELS[info.status.state];
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

  /** Один плагин: голова-строка (тумблер/имя/инфо/бейдж/шеврон) + тело-аккордеон. */
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

    const open = expanded.has(info.id);

    // Порядок головы: тумблер → имя → инфо (если notices) → бейдж → шеврон.
    head.append(toggleLabel, name);
    // Инфо-кнопка — только при непустых notices: пустой поповер бессмыслен.
    const infoPopover = info.notices.length > 0 ? renderInfoPopover(info) : null;
    if (infoPopover) {
      const infoButton = renderInfoButton(info);
      infoPopovers.set(infoButton, infoPopover);
      head.append(infoButton);
    }
    head.append(renderBadge(info), renderExpandButton(info, open));
    // Поповер — последним ребёнком головы: позиционируется абсолютно, порядок не важен.
    if (infoPopover) head.append(infoPopover);

    const body = el("div", "pl-item-body");
    body.id = bodyId(info.id);

    // Порядок тела: критичное выше рутины — fail-notice → actions → consent.
    // Статус `failed` несёт сообщение — показываем его первым.
    const failMessage = statusMessage(info.status);
    if (failMessage) {
      const node = el("div", "pl-notice");
      node.textContent = failMessage;
      body.append(node);
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
      btn.textContent = command.title;
      btn.title = command.keybinding
        ? `${command.title} (${command.keybinding})`
        : command.title;
      btn.setAttribute("aria-label", `Выполнить: ${command.title}`);
      actions.append(btn);
    }
    body.append(actions);

    // Инлайн-секция согласия: запросы манифеста + фактически выданные права.
    const consent = renderConsent(info);
    if (consent) body.append(consent);

    // Тело всегда в DOM (скрывается CSS `.pl-item:not(.open)`) — иначе e2e не
    // найдёт контролы свёрнутой карточки до её раскрытия.
    item.classList.toggle("open", open);
    item.append(head, body);
    return item;
  }

  /** Открывает поповер кнопки; повторный клик или открытие другого закрывает предыдущий. */
  function toggleInfoPopover(button: HTMLButtonElement): void {
    const popover = infoPopovers.get(button);
    if (!popover) return;
    // Повторный клик по той же кнопке закрывает; открытие другой — закрывает текущий.
    const wasOpen = openInfoButton === button;
    closeAllInfoPopovers();
    if (wasOpen) return;
    popover.hidden = false;
    popover.setAttribute("aria-hidden", "false");
    button.setAttribute("aria-expanded", "true");
    openInfoButton = button;
  }

  /** Закрывает открытый поповер (он один) и возвращает кнопку — для возврата фокуса. */
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

  /**
   * Раскрывает/сворачивает карточку на месте — без `refresh` и IPC: узлы не
   * пересоздаются, фокус остаётся на шевроне, лишних гонок и перерисовок нет.
   */
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
    // `replaceChildren` уничтожит узлы, включая открытый поповер, — сбрасываем ссылку.
    closeAllInfoPopovers();
    // Прунинг: выкидываем раскрытие исчезнувших плагинов, иначе `Set` растёт вечно.
    const ids = new Set(list.map((info) => info.id));
    for (const id of expanded) {
      if (!ids.has(id)) expanded.delete(id);
    }
    if (list.length === 0) {
      const empty = el("div", "side-empty pl-empty");
      const title = el("div", "pl-empty-title");
      title.textContent = "Плагины не найдены";
      const hint = el("div", "pl-empty-hint");
      // Статические строки (путь/состав папки) — textContent, без innerHTML.
      hint.append("Положите плагин в ");
      const code = document.createElement("code");
      code.textContent = "%APPDATA%/darmark/plugins/<id>/";
      hint.append(code);
      // Каталог пересканируется при старте (на лету H2 не подхватывает) — честно
      // просим перезапуск, а не несуществующую кнопку «Перезагрузить все».
      hint.append(" (plugin.json + main.lua) и перезапустите darmark.");
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
    const command = target?.closest<HTMLButtonElement>(".pl-cmd");
    if (command) {
      void handleCommand(command);
      return;
    }
    const bulk = target?.closest<HTMLButtonElement>(".pl-perms-bulk button");
    if (bulk) void handleGrantBulk(bulk);
  }

  /** Клик вне кнопки/поповера закрывает его (делегирование на `document`). */
  function onDocumentClick(e: MouseEvent): void {
    const button = openInfoButton;
    if (!button) return;
    const popover = infoPopovers.get(button);
    const target = e.target as Node | null;
    if (target && (button.contains(target) || popover?.contains(target))) return;
    closeAllInfoPopovers();
  }

  /** Escape закрывает поповер и возвращает фокус на кнопку. */
  function onDocumentKeydown(e: KeyboardEvent): void {
    if (e.key !== "Escape" || !openInfoButton) return;
    // Поповер — верхний слой: поглощаем Escape, чтобы он не закрыл дровер
    // (drawer.ts отдаёт событие вложенному слою) и не выключил инспектор.
    e.preventDefault();
    e.stopPropagation();
    closeAllInfoPopovers()?.focus();
  }

  root.addEventListener("change", onChange);
  root.addEventListener("click", onClick);
  document.addEventListener("click", onDocumentClick);
  document.addEventListener("keydown", onDocumentKeydown);

  // Ошибку установки слушателя глушим: в обычном (не плагинном) запуске событий не будет.
  const unlistenPromise = listen("plugins-changed", () => {
    void refresh();
  }).catch(() => null);

  return {
    refresh,
    reloadForDocument,

    dispose(): void {
      disposed = true;
      expanded.clear(); // симметрия с create: контроллер не удерживает id после уничтожения
      closeAllInfoPopovers();
      root.removeEventListener("change", onChange);
      root.removeEventListener("click", onClick);
      document.removeEventListener("click", onDocumentClick);
      document.removeEventListener("keydown", onDocumentKeydown);
      void unlistenPromise.then((fn) => fn?.());
    },
  };
}
