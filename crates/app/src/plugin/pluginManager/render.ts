// Рендер идентичности плагина и кнопок карточки.

import type { PluginInfo } from "../../core/tauri";
import { bodyId, el, infoPopoverId } from "./dom";
import { chevronIcon, infoIcon } from "./icons";

export function renderIdentity(info: PluginInfo): HTMLElement {
  const ident = el("div", "pl-ident");
  ident.title = info.id;

  const name = el("span", "pl-name");
  name.textContent = info.name || info.id;
  ident.append(name);

  if (info.version) {
    const version = el("span", "pl-version");
    version.textContent = `· ${info.version}`;
    ident.append(version);
  }

  const id = el("span", "pl-id");
  id.textContent = info.id;
  id.title = `id: ${info.id}`;
  ident.append(id);

  return ident;
}

export function renderInfoButton(info: PluginInfo): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "pl-info";
  btn.setAttribute("aria-expanded", "false");
  btn.setAttribute("aria-controls", infoPopoverId(info.id));
  btn.setAttribute("aria-label", `Информация о границах изоляции плагина ${info.id}`);
  btn.append(infoIcon());
  return btn;
}

export function renderInfoPopover(info: PluginInfo): HTMLElement {
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

export function renderExpandButton(info: PluginInfo, open: boolean): HTMLButtonElement {
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
