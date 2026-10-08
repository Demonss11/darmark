// palette.ts — палитра команд (Ctrl+K), построена на dialog.ts (variant "top").
//
// Провайдеры команд: core (синхронный список из main.ts) и плагинные
// (асинхронно, list_plugins/run_plugin_command). Палитра не парсит Markdown и не
// хранит доменное состояние; ошибка провайдера уходит в onError, палитра живёт.
// Ответы устаревших запросов отбрасываются по счётчику сессий (seq-гейт).

import { openDialog, type Dialog } from "./dialog";
import { pluginColor } from "./pluginColor";
import { errorMessage } from "./tauri";

export interface PaletteCommand {
  /** Уникальный id команды. */
  id: string;
  /** Заголовок пункта (по нему идёт фильтр). */
  title: string;
  /** Секция группировки (sentence case). */
  group: string;
  /** Источник: ядро или плагин (`plugin:<id>`). */
  origin: "core" | `plugin:${string}`;
  /** Хоткей для бейджа (только Ctrl-символы). */
  hotkey?: string;
  /** Действие команды. */
  run(): void | Promise<void>;
}

export interface CommandProvider {
  /** Команды провайдера: массив (синхронно) или промис (плагины). */
  list(): PaletteCommand[] | Promise<PaletteCommand[]>;
}

export interface PaletteOptions {
  providers: CommandProvider[];
  /** Ошибка провайдера/пустой результат — сообщение для тоста. */
  onError?(message: string): void;
}

export interface PaletteController {
  open(): void;
  close(): void;
  toggle(): void;
  isOpen(): boolean;
}

/** Подпись источника: `darmark` для ядра, id плагина — для плагина. */
function originLabel(origin: PaletteCommand["origin"]): string {
  return origin.startsWith("plugin:") ? origin.slice("plugin:".length) : "darmark";
}

export function createPalette(opts: PaletteOptions): PaletteController {
  let dialog: Dialog | null = null;
  let session = 0;
  let ac: AbortController | null = null;
  let commands: PaletteCommand[] = [];
  let filtered: PaletteCommand[] = [];
  let activeIndex = -1;

  let input!: HTMLInputElement;
  let list!: HTMLElement;

  function isOpen(): boolean {
    return dialog !== null;
  }

  /** Сделать пункт активным по индексу (с закольцовыванием). */
  function setActive(index: number): void {
    activeIndex = filtered.length === 0 ? -1 : (index + filtered.length) % filtered.length;
    renderSelection();
  }

  /** Обновить aria-selected/aria-activedescendant и дот-скроллить активный пункт. */
  function renderSelection(): void {
    const items = Array.from(list.querySelectorAll<HTMLElement>(".palette-item"));
    items.forEach((item, i) => {
      const selected = i === activeIndex;
      item.setAttribute("aria-selected", selected ? "true" : "false");
      if (selected) item.scrollIntoView({ block: "nearest" });
    });
    const active = activeIndex >= 0 ? items[activeIndex] : undefined;
    if (active) input.setAttribute("aria-activedescendant", active.id);
    else input.removeAttribute("aria-activedescendant");
  }

  function renderItem(command: PaletteCommand, index: number): HTMLElement {
    const item = document.createElement("div");
    item.className = "palette-item";
    item.id = `palette-item-${index}`;
    item.dataset.index = String(index);
    item.setAttribute("role", "option");
    item.setAttribute("aria-selected", index === activeIndex ? "true" : "false");
    if (command.origin.startsWith("plugin:")) {
      item.style.setProperty("--pc", pluginColor(originLabel(command.origin)));
    }

    const title = document.createElement("span");
    title.className = "palette-title";
    title.textContent = command.title;

    const origin = document.createElement("span");
    origin.className = "palette-origin";
    origin.textContent = originLabel(command.origin);
    origin.title = command.origin;

    item.append(title, origin);

    if (command.hotkey) {
      const key = document.createElement("span");
      key.className = "palette-key";
      key.textContent = command.hotkey;
      item.append(key);
    }
    return item;
  }

  /** Перестроить список по текущему фильтру (группы + счётчики). */
  function render(): void {
    const query = input.value.trim().toLowerCase();
    filtered = query
      ? commands.filter((c) => c.title.toLowerCase().includes(query))
      : commands.slice();

    if (filtered.length === 0) activeIndex = -1;
    else if (activeIndex < 0 || activeIndex >= filtered.length) activeIndex = 0;

    // Счётчики по группам считаем заранее, а список рендерим в порядке `filtered`,
    // вставляя заголовок при смене группы. Так DOM-порядок совпадает с `filtered`
    // (иначе индекс для Enter/клика разошёлся бы при неконтигуозных группах).
    const counts = new Map<string, number>();
    for (const command of filtered) {
      counts.set(command.group, (counts.get(command.group) ?? 0) + 1);
    }

    const frag = document.createDocumentFragment();
    let section: HTMLElement | null = null;
    let lastGroup = "";
    filtered.forEach((command, index) => {
      if (command.group !== lastGroup) {
        lastGroup = command.group;
        section = document.createElement("div");
        section.className = "palette-group";

        const head = document.createElement("div");
        head.className = "palette-group-title";
        const name = document.createElement("span");
        name.textContent = command.group;
        const count = document.createElement("span");
        count.className = "palette-count";
        count.textContent = String(counts.get(command.group) ?? 0);
        head.append(name, count);
        section.append(head);
        frag.append(section);
      }
      section?.append(renderItem(command, index));
    });

    if (filtered.length === 0) {
      const empty = document.createElement("div");
      empty.className = "palette-empty";
      const line1 = document.createElement("div");
      line1.textContent = "Ничего не найдено";
      const line2 = document.createElement("div");
      line2.textContent = "Попробуйте другой запрос.";
      empty.append(line1, line2);
      frag.append(empty);
    }

    list.replaceChildren(frag);
    renderSelection();
  }

  async function runCommand(command: PaletteCommand): Promise<void> {
    close();
    try {
      await command.run();
    } catch (e) {
      opts.onError?.(errorMessage(e));
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    // Ctrl+K — toggle палитры; Ctrl+Shift+K оставляем команде «Ссылка», поэтому
    // проверяем отсутствие Shift/Alt. shell.ts при открытом модале хоткеи глушит.
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k" && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      // Гасим всплытие: иначе после close() событие дойдёт до window-слушателя
      // shell.ts, где isModalOpen() уже false, и палитра переоткроется.
      e.stopPropagation();
      close();
      return;
    }
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setActive(activeIndex + 1);
        break;
      case "ArrowUp":
        e.preventDefault();
        setActive(activeIndex - 1);
        break;
      case "Home":
        e.preventDefault();
        setActive(0);
        break;
      case "End":
        e.preventDefault();
        setActive(filtered.length - 1);
        break;
      case "Enter": {
        e.preventDefault();
        const command = activeIndex >= 0 ? filtered[activeIndex] : undefined;
        if (!command) {
          opts.onError?.("Ничего не найдено");
          return;
        }
        void runCommand(command);
        break;
      }
      default:
        break;
    }
  }

  function onListClick(e: MouseEvent): void {
    const target = e.target as Element | null;
    const item = target?.closest<HTMLElement>(".palette-item");
    if (!item) return;
    const command = filtered[Number(item.dataset.index ?? "-1")];
    if (command) void runCommand(command);
  }

  function close(): void {
    dialog?.close();
  }

  function open(): void {
    if (dialog) return;
    commands = [];
    filtered = [];
    activeIndex = -1;

    const mySession = ++session;
    ac = new AbortController();
    const { signal } = ac;

    const content = document.createElement("div");
    content.className = "palette";

    input = document.createElement("input");
    input.type = "text";
    input.className = "palette-input";
    input.placeholder = "Введите команду…";
    input.setAttribute("role", "combobox");
    input.setAttribute("aria-expanded", "true");
    input.setAttribute("aria-controls", "palette-list");
    input.setAttribute("aria-autocomplete", "list");
    input.setAttribute("autocomplete", "off");
    input.setAttribute("aria-label", "Палитра команд");
    input.setAttribute("spellcheck", "false");

    list = document.createElement("div");
    list.className = "palette-list";
    list.id = "palette-list";
    list.setAttribute("role", "listbox");

    content.append(input, list);

    input.addEventListener(
      "input",
      () => {
        activeIndex = 0;
        render();
      },
      { signal }
    );
    // Слушаем клавиатуру на панели, а не только на input: если фокус окажется
    // на другом элементе палитры, Ctrl+K/стрелки всё равно отработают.
    content.addEventListener("keydown", onKeydown, { signal });
    // Наведение мыши переносит активный пункт: подсветка (`aria-selected`) следует
    // за курсором, и Enter выполняет наведённую команду, а не первую.
    list.addEventListener(
      "mouseover",
      (e) => {
        const item = (e.target as Element | null)?.closest<HTMLElement>(".palette-item");
        if (!item) return;
        const index = Number(item.dataset.index ?? "-1");
        if (index >= 0 && index !== activeIndex) setActive(index);
      },
      { signal }
    );
    list.addEventListener("click", onListClick, { signal });

    dialog = openDialog({
      title: "Палитра команд",
      content,
      initialFocus: input,
      variant: "top",
      closeOnBackdrop: true,
      onClose: () => {
        dialog = null;
        ac?.abort();
        ac = null;
        commands = [];
        filtered = [];
        activeIndex = -1;
      },
    });

    // Синхронные провайдеры — сразу, асинхронные — по готовности (seq-гейт).
    for (const provider of opts.providers) {
      let result: PaletteCommand[] | Promise<PaletteCommand[]>;
      try {
        result = provider.list();
      } catch (e) {
        opts.onError?.(errorMessage(e));
        continue;
      }
      if (Array.isArray(result)) {
        commands.push(...result);
      } else {
        void result
          .then((items) => {
            if (session !== mySession || !dialog) return;
            commands.push(...items);
            render();
          })
          .catch((e) => {
            if (session !== mySession || !dialog) return;
            opts.onError?.(errorMessage(e));
          });
      }
    }
    render();
  }

  function toggle(): void {
    if (dialog) close();
    else open();
  }

  return { open, close, toggle, isOpen };
}
