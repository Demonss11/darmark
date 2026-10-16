

import type { PluginInfo, PluginSettingProperty } from "../../core/tauri";
import { el } from "./dom";

export function settingValue(info: PluginInfo, key: string, prop: PluginSettingProperty): unknown {
  const stored = info.settings_values[key];
  return stored !== undefined ? stored : prop.default;
}

export function renderSettingRow(
  key: string,
  prop: PluginSettingProperty,
  control: HTMLInputElement | HTMLSelectElement
): HTMLElement {
  const row = el("div", "pl-setting");
  const head = el("label", "pl-setting-head");
  const label = el("span", "pl-setting-label");
  label.textContent = prop.title ?? key;
  control.dataset.settingKey = key;
  head.append(label, control);
  row.append(head);
  if (prop.description) {
    const desc = el("div", "pl-setting-desc");
    desc.textContent = prop.description;
    row.append(desc);
  }
  return row;
}
