// palette.ts вЂ” РїР°Р»РёС‚СЂР° РєРѕРјР°РЅРґ (Ctrl+K), РїРѕСЃС‚СЂРѕРµРЅР° РЅР° dialog.ts (variant "top").
//
// РџСЂРѕРІР°Р№РґРµСЂС‹ РєРѕРјР°РЅРґ: core (СЃРёРЅС…СЂРѕРЅРЅС‹Р№ СЃРїРёСЃРѕРє РёР· main.ts) Рё РїР»Р°РіРёРЅРЅС‹Рµ
// (Р°СЃРёРЅС…СЂРѕРЅРЅРѕ, list_plugins/run_plugin_command). РџР°Р»РёС‚СЂР° РЅРµ РїР°СЂСЃРёС‚ Markdown Рё РЅРµ
// С…СЂР°РЅРёС‚ РґРѕРјРµРЅРЅРѕРµ СЃРѕСЃС‚РѕСЏРЅРёРµ; РѕС€РёР±РєР° РїСЂРѕРІР°Р№РґРµСЂР° СѓС…РѕРґРёС‚ РІ onError. РћС‚РІРµС‚С‹
// СѓСЃС‚Р°СЂРµРІС€РёС… Р·Р°РїСЂРѕСЃРѕРІ РѕС‚Р±СЂР°СЃС‹РІР°СЋС‚СЃСЏ РїРѕ СЃС‡С‘С‚С‡РёРєСѓ СЃРµСЃСЃРёР№ (seq-РіРµР№С‚).

import { openDialog, type Dialog } from "./dialog";
import { pluginColor } from "../plugin/pluginColor";
import { errorMessage } from "../core/tauri";

export interface PaletteCommand {
  id: string;
  /** Р—Р°РіРѕР»РѕРІРѕРє РїСѓРЅРєС‚Р° (РїРѕ РЅРµРјСѓ РёРґС‘С‚ С„РёР»СЊС‚СЂ). */
  title: string;
  /** РЎРµРєС†РёСЏ РіСЂСѓРїРїРёСЂРѕРІРєРё (sentence case). */
  group: string;
  /** РСЃС‚РѕС‡РЅРёРє: СЏРґСЂРѕ РёР»Рё РїР»Р°РіРёРЅ (`plugin:<id>`). */
  origin: "core" | `plugin:${string}`;
  /** РҐРѕС‚РєРµР№ РґР»СЏ Р±РµР№РґР¶Р° (С‚РѕР»СЊРєРѕ Ctrl-СЃРёРјРІРѕР»С‹). */
  hotkey?: string;
  run(): void | Promise<void>;
}

export interface CommandProvider {
  /** РљРѕРјР°РЅРґС‹ РїСЂРѕРІР°Р№РґРµСЂР°: РјР°СЃСЃРёРІ (СЃРёРЅС…СЂРѕРЅРЅРѕ) РёР»Рё РїСЂРѕРјРёСЃ (РїР»Р°РіРёРЅС‹). */
  list(): PaletteCommand[] | Promise<PaletteCommand[]>;
}

export interface PaletteOptions {
  providers: CommandProvider[];
  /** РћС€РёР±РєР° РїСЂРѕРІР°Р№РґРµСЂР°/РїСѓСЃС‚РѕР№ СЂРµР·СѓР»СЊС‚Р°С‚ вЂ” СЃРѕРѕР±С‰РµРЅРёРµ РґР»СЏ С‚РѕСЃС‚Р°. */
  onError?(message: string): void;
}

export interface PaletteController {
  open(): void;
  close(): void;
  toggle(): void;
  isOpen(): boolean;
}

/** РџРѕРґРїРёСЃСЊ РёСЃС‚РѕС‡РЅРёРєР°: `РџР•РќРђР›` РґР»СЏ СЏРґСЂР°, id РїР»Р°РіРёРЅР° вЂ” РґР»СЏ РїР»Р°РіРёРЅР°. */
function originLabel(origin: PaletteCommand["origin"]): string {
  return origin.startsWith("plugin:") ? origin.slice("plugin:".length) : "РџР•РќРђР›";
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

  /** РЎРґРµР»Р°С‚СЊ РїСѓРЅРєС‚ Р°РєС‚РёРІРЅС‹Рј РїРѕ РёРЅРґРµРєСЃСѓ (СЃ Р·Р°РєРѕР»СЊС†РѕРІС‹РІР°РЅРёРµРј). */
  function setActive(index: number): void {
    activeIndex = filtered.length === 0 ? -1 : (index + filtered.length) % filtered.length;
    renderSelection();
  }

  /** РћР±РЅРѕРІРёС‚СЊ aria-selected/aria-activedescendant Рё РґРѕС‚-СЃРєСЂРѕР»Р»РёС‚СЊ Р°РєС‚РёРІРЅС‹Р№ РїСѓРЅРєС‚. */
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

  /** РџРµСЂРµСЃС‚СЂРѕРёС‚СЊ СЃРїРёСЃРѕРє РїРѕ С‚РµРєСѓС‰РµРјСѓ С„РёР»СЊС‚СЂСѓ (РіСЂСѓРїРїС‹ + СЃС‡С‘С‚С‡РёРєРё). */
  function render(): void {
    const query = input.value.trim().toLowerCase();
    filtered = query
      ? commands.filter((c) => c.title.toLowerCase().includes(query))
      : commands.slice();

    if (filtered.length === 0) activeIndex = -1;
    else if (activeIndex < 0 || activeIndex >= filtered.length) activeIndex = 0;





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
      line1.textContent = "РќРёС‡РµРіРѕ РЅРµ РЅР°Р№РґРµРЅРѕ";
      const line2 = document.createElement("div");
      line2.textContent = "РџРѕРїСЂРѕР±СѓР№С‚Рµ РґСЂСѓРіРѕР№ Р·Р°РїСЂРѕСЃ.";
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


    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k" && !e.shiftKey && !e.altKey) {
      e.preventDefault();


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
          opts.onError?.("РќРёС‡РµРіРѕ РЅРµ РЅР°Р№РґРµРЅРѕ");
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
    input.placeholder = "Р’РІРµРґРёС‚Рµ РєРѕРјР°РЅРґСѓвЂ¦";
    input.setAttribute("role", "combobox");
    input.setAttribute("aria-expanded", "true");
    input.setAttribute("aria-controls", "palette-list");
    input.setAttribute("aria-autocomplete", "list");
    input.setAttribute("autocomplete", "off");
    input.setAttribute("aria-label", "РџР°Р»РёС‚СЂР° РєРѕРјР°РЅРґ");
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


    content.addEventListener("keydown", onKeydown, { signal });


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
      title: "РџР°Р»РёС‚СЂР° РєРѕРјР°РЅРґ",
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
