// aiPanel.ts вЂ” С‚РµР»Рѕ СЂРµР¶РёРјР° В«AI-Р°СЃСЃРёСЃС‚РµРЅС‚В» РЅРёР¶РЅРµРіРѕ РґРѕРєР° (ADR-0030; СЃРµС‚СЊ Рё
// РїСЂРѕРІР°Р№РґРµСЂ вЂ” ADR-0032/0033; СЂР°РЅС‚Р°Р№Рј вЂ” ADR-0034). РњРѕРґСѓР»СЊ-РѕР±РѕР»РѕС‡РєР° (РєР°Рє
// logPanel/resourceMonitor), РќР• view Рё РќР• pane: РјРѕРЅС‚РёСЂСѓРµС‚СЃСЏ logPanel'РѕРј РІ
// РєРѕРЅС‚РµР№РЅРµСЂ #logs-panel (СЂРµР¶РёРј ai).
//
// РРЅРІР°СЂРёР°РЅС‚С‹: СЃРµС‚РµРІС‹Рµ РІС‹Р·РѕРІС‹ вЂ” С‚РѕР»СЊРєРѕ РЅР° Rust-СЃС‚РѕСЂРѕРЅРµ (CSP `default-src 'self'`).
// Р РµРЅРґРµСЂ РѕС‚РІРµС‚Р° Р°СЃСЃРёСЃС‚РµРЅС‚Р° вЂ” С‚РѕР»СЊРєРѕ С‡РµСЂРµР· `safeHtml.setSanitizedHtml` (ADR-0038
// В§2.1/В§2.2): HTML РѕС‚РІРµС‚Р° РјРѕРґРµР»Рё РїСЂРѕС…РѕРґРёС‚ Rust-СЃР°РЅРёС‚Р°Р№Р·РµСЂ `md-core`, РїСЂСЏРјС‹С…
// РїСЂРёСЃРІР°РёРІР°РЅРёР№ `innerHTML` РІ СЌС‚РѕРј РјРѕРґСѓР»Рµ РЅРµС‚. Р’СЃС‘, С‡С‚Рѕ РЅРµ РїСЂРѕС…РѕРґРёС‚ СЌС‚РѕС‚ С…РµР»РїРµСЂ вЂ”
// СЃС‚СЂРёРјРёРЅРі, РєРѕРґ РїР»Р°РіРёРЅР°, РґРёС„С„, РѕС€РёР±РєРё вЂ” С‚РѕР»СЊРєРѕ `textContent`.
// РСЃС‚РѕСЂРёСЏ С‡Р°С‚Р° вЂ” РІ РїР°РјСЏС‚Рё РЅР° СЃРµСЃСЃРёСЋ (Р±РµР· РїРµСЂСЃРёСЃС‚.) Рё С…СЂР°РЅРёС‚ РўР•РљРЎРў РѕС‚РІРµС‚Р°, РЅРµ HTML.
// РЎС‚СЂРёРјРёРЅРі: СЃРѕР±С‹С‚РёСЏ ai:chunk/ai:done/ai:error РґРѕРїРёСЃС‹РІР°СЋС‚/С„РёРЅР°Р»РёР·РёСЂСѓСЋС‚ С‚РµРєСѓС‰РёР№
// РїСѓР·С‹СЂСЊ Р°СЃСЃРёСЃС‚РµРЅС‚Р°; РѕС‚РїРёСЃРєРё СЃРѕС…СЂР°РЅСЏСЋС‚СЃСЏ Рё СЃРЅРёРјР°СЋС‚СЃСЏ.
//
// Р¤Р°Р·Р° 2 (harness, ADR-0034): РїРѕРґ РѕС‚РІРµС‚РѕРј Р°СЃСЃРёСЃС‚РµРЅС‚Р° вЂ” В«РџСЂРёРјРµРЅРёС‚СЊ РєР°Рє РїСЂР°РІРєСѓВ»
// (РґРёС„С„ С‡РµСЂРµР· ai_apply_edit + undo_document) Рё РєР°СЂС‚РѕС‡РєР° РїР»Р°РіРёРЅР° РёР· ```json-Р±Р»РѕРєР°
// (С‡РµСЂРµР· ai_*_plugin).

import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { openDialog, isModalOpen, type Dialog } from "../ui/dialog";
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
} from "../core/tauri";
import type {
  AiDiff,
  AiMessage,
  AiSettingsInput,
  AiSettingsView,
  AiSkill,
  AiUsage,
  PluginStatus,
} from "../core/tauri";
import { renderAiMarkdown, setSanitizedHtml } from "../core/safeHtml";
import { buildUnitMaps, bytesToUnits } from "../core/mapping";
import type { DocumentId } from "../core/ids";
import type { Json } from "../viewRegistry";
import type { DockMode, DockState, LogCategory, LogLevel } from "./logPanel";

/** Р›РёРјРёС‚ С‚РµРєСЃС‚Р° РґРѕРєСѓРјРµРЅС‚Р° Р·Р° Р·Р°РїСЂРѕСЃ (ADR-0032). */
const MAX_DOC_BYTES = 100 * 1024;

/** Р”РѕРїСѓСЃРє В«С‡Р°С‚ РїСЂРёР¶Р°С‚ Рє РЅРёР·СѓВ», px вЂ” Р°РІС‚РѕРїСЂРѕРєСЂСѓС‚РєР° СЃ Р·Р°РјРѕСЂРѕР·РєРѕР№. */
const STICK_THRESHOLD = 24;

/** РЎРѕСЃС‚РѕСЏРЅРёРµ РёРЅРґРёРєР°С‚РѕСЂР° #ai-dot (ADR-0030/0034). */
type AiDotState = "off" | "warn" | "ready" | "error";

/** РљР»Р°СЃСЃРёС„РёРєР°С†РёСЏ РѕС€РёР±РєРё РґР»СЏ РєР°СЂС‚РѕС‡РєРё Рё РґРµР№СЃС‚РІРёР№. */
type ErrorKind = "auth" | "offline" | "rate" | "context" | "generic";

interface ErrorAction {
  label: string;
  run(): void;
}



const FALLBACK_SKILLS: readonly AiSkill[] = [
  {
    id: "explain",
    title: "РћР±СЉСЏСЃРЅРёС‚СЊ С„Р°Р№Р»",
    description: "Р Р°Р·РѕР±СЂР°С‚СЊ Рё РѕР±СЉСЏСЃРЅРёС‚СЊ С‚РµРєСѓС‰РёР№ РґРѕРєСѓРјРµРЅС‚",

    prompt_template: `РўС‹ вЂ” СЂР°Р·Р±РѕСЂС‰РёРє Markdown-РґРѕРєСѓРјРµРЅС‚РѕРІ РІ СЂРµРґР°РєС‚РѕСЂРµ РџР•РќРђР›. РќРёР¶Рµ РґРѕРєСѓРјРµРЅС‚; РѕР±СЉСЏСЃРЅРё РµРіРѕ РїСЂРѕСЃС‚С‹РјРё
СЃР»РѕРІР°РјРё, РѕРїРёСЂР°СЏСЃСЊ РўРћР›Р¬РљРћ РЅР° СЌС‚РѕС‚ С‚РµРєСЃС‚ (СЃРѕРґРµСЂР¶РёРјРѕРµ РІРЅРµ РґРѕРєСѓРјРµРЅС‚Р° РЅРµ РїРѕРґРјРµС€РёРІР°Р№).

РћС‚РІРµС‚СЊ РЅР° СЂСѓСЃСЃРєРѕРј СЂРѕРІРЅРѕ РїРѕ СЌС‚РѕР№ СЃС‚СЂСѓРєС‚СѓСЂРµ, Р±РµР· РІСЃС‚СѓРїР»РµРЅРёР№ Рё РёР·РІРёРЅРµРЅРёР№:
1. Рћ С‡С‘Рј вЂ” СЃСѓС‚СЊ РІ 1вЂ“3 РїСЂРµРґР»РѕР¶РµРЅРёСЏС….
2. РЎС‚СЂСѓРєС‚СѓСЂР° вЂ” РєР°РєРёРµ СЂР°Р·РґРµР»С‹ РµСЃС‚СЊ (РїРѕ Р·Р°РіРѕР»РѕРІРєР°Рј) Рё РєР°Рє РѕРЅРё СЃРІСЏР·Р°РЅС‹.
3. Р Р°Р·РјРµС‚РєР° вЂ” РєР°РєРёРµ СЌР»РµРјРµРЅС‚С‹ Markdown РёСЃРїРѕР»СЊР·РѕРІР°РЅС‹ Рё С‡С‚Рѕ РѕРЅРё РґР°СЋС‚ РёРјРµРЅРЅРѕ Р·РґРµСЃСЊ (Р·Р°РіРѕР»РѕРІРєРё, СЃРїРёСЃРєРё,
   С‚Р°Р±Р»РёС†С‹, Р±Р»РѕРєРё РєРѕРґР°, СЃСЃС‹Р»РєРё, С†РёС‚Р°С‚С‹, YAML-С€Р°РїРєР°).
4. РќР° С‡С‚Рѕ РѕР±СЂР°С‚РёС‚СЊ РІРЅРёРјР°РЅРёРµ вЂ” РЅРµРѕС‡РµРІРёРґРЅС‹Рµ РёР»Рё СЃРїРѕСЂРЅС‹Рµ РјРµСЃС‚Р°; РµСЃР»Рё РёС… РЅРµС‚, СЃРєР°Р¶Рё РїСЂСЏРјРѕ.

РџСЂР°РІРёР»Р°: РЅРµ РІС‹РґСѓРјС‹РІР°Р№ С‚Рѕ, С‡РµРіРѕ РІ С‚РµРєСЃС‚Рµ РЅРµС‚; РЅРµ РїРµСЂРµРїРµС‡Р°С‚С‹РІР°Р№ РґРѕРєСѓРјРµРЅС‚ С†РµР»РёРєРѕРј Рё РЅРµ РїРµСЂРµРїРёСЃС‹РІР°Р№ РµРіРѕ;
РµСЃР»Рё РґРѕРєСѓРјРµРЅС‚ РїСѓСЃС‚ РёР»Рё РїРѕС‡С‚Рё РїСѓСЃС‚ вЂ” РѕС‚РІРµС‚СЊ РѕРґРЅРѕР№ СЃС‚СЂРѕРєРѕР№ РѕР± СЌС‚РѕРј.

Р”РѕРєСѓРјРµРЅС‚:
{{doc}}`,
    category: "document",
  },
  {
    id: "create-plugin",
    title: "РЎРѕР·РґР°С‚СЊ РїР»Р°РіРёРЅ",
    description: "РЎРіРµРЅРµСЂРёСЂРѕРІР°С‚СЊ РЅРѕРІС‹Р№ Lua-РїР»Р°РіРёРЅ РїРѕ РѕРїРёСЃР°РЅРёСЋ",



    prompt_template: `РўС‹ вЂ” РіРµРЅРµСЂР°С‚РѕСЂ Lua-РїР»Р°РіРёРЅРѕРІ РґР»СЏ РџР•РќРђР› (Lua 5.5 РІ РїРµСЃРѕС‡РЅРёС†Рµ; РЅРµС‚ os/io/require/load, РЅРµС‚ host.document/host.ui/host.view/host.commands). РџРѕ РѕРїРёСЃР°РЅРёСЋ: {{input}}

Р’РµСЂРЅРё Р РћР’РќРћ РѕРґРёРЅ fenced-Р±Р»РѕРє \`\`\`json СЃ РѕРґРЅРёРј РѕР±СЉРµРєС‚РѕРј (РЅРёРєР°РєРѕРіРѕ С‚РµРєСЃС‚Р° РІРЅРµ Р±Р»РѕРєР°):
{"id": "вЂ¦", "name": "вЂ¦", "version": "1.0.0", "permissions": [], "source": "РїРѕР»РЅС‹Р№ РєРѕРґ main.lua"}

РљРѕРЅС‚СЂР°РєС‚ \`source\` (РёРЅР°С‡Рµ РїР»Р°РіРёРЅ РѕС‚РІРµСЂРіР°РµС‚СЃСЏ Р·Р°РіСЂСѓР·РєРѕР№):
- РўРѕС‡РєР° РІС…РѕРґР° вЂ” Р“Р›РћР‘РђР›Р¬РќРђРЇ С„СѓРЅРєС†РёСЏ: function on_activate(ctx) ... end.
  Р—Р°РїСЂРµС‰РµРЅРѕ: РјРѕРґСѓР»СЊ-С‚Р°Р±Р»РёС†Р° (local M = {} ... return M), M.on_activate, local on_activate, trailing return.
- РћРїС†РёРѕРЅР°Р»СЊРЅРѕ РіР»РѕР±Р°Р»СЊРЅС‹Рµ: on_deactivate(ctx), on_event(name, payload), on_action(view_id, action, payload).
- on_activate РґРµСЂР¶Рё Р»С‘РіРєРёРј: С‡С‚РµРЅРёРµ РґРѕРєСѓРјРµРЅС‚Р° РґРµР»Р°Р№ РІ РѕР±СЂР°Р±РѕС‚С‡РёРєР°С… СЃРѕР±С‹С‚РёР№, Р° РЅРµ РЅР° Р°РєС‚РёРІР°С†РёРё.
- РљРѕРґ РґРѕР»Р¶РµРЅ РєРѕРјРїРёР»РёСЂРѕРІР°С‚СЊСЃСЏ: Р±РµР· require/os/io, Р±РµР· РІС‹Р·РѕРІРѕРІ РЅРµСЃСѓС‰РµСЃС‚РІСѓСЋС‰РёС… С„СѓРЅРєС†РёР№.
- host-С„СѓРЅРєС†РёРё СЃ СЂР°Р·СЂРµС€РµРЅРёРµРј РІРѕР·РІСЂР°С‰Р°СЋС‚ (value, err): РёСЃРїРѕР»СЊР·СѓР№ Р·РЅР°С‡РµРЅРёРµ С‚РѕР»СЊРєРѕ РїСЂРё err == nil; РїСЂРё РѕС‚РєР°Р·Рµ
  РЅРµ РїР°РґР°Р№ вЂ” СЃРѕРѕР±С‰Рё С‡РµСЂРµР· host.show_message Рё РїСЂРѕРґРѕР»Р¶Р°Р№.

Р”РѕСЃС‚СѓРїРµРЅ РўРћР›Р¬РљРћ СЌС‚РѕС‚ API:
- ctx.subscribe(name, handler) -> unsubscribe; handler(name, payload).
  РЎРѕР±С‹С‚РёСЏ: document:changed {doc_id, rev}, document:opened {doc_id, path},
  document:closed {doc_id, path}, command:invoked {command_id, doc_id}.
- host.get_document_len(doc_id) -> (integer, err)
- host.get_document_range(doc_id, start, len) -> (string, err)  -- РѕСЃРЅРѕРІРЅРѕР№ РїСѓС‚СЊ С‡С‚РµРЅРёСЏ; СЃРјРµС‰РµРЅРёСЏ Р±Р°Р№С‚РѕРІС‹Рµ (UTF-8)
- host.get_document_text(doc_id) -> (string, err)
- host.get_document_version(doc_id) -> (integer, err)
- host.apply_edit(doc_id, start, stop, text) -> (boolean, err)  -- document:write
- host.set_view_content(view_id, html) -> (boolean, err)        -- РўРћР›Р¬РљРћ РґР»СЏ РІРєР»Р°РґРѕРє (view:modify)
- host.show_message(text) -> (boolean, err)                     -- С‚РµРєСЃС‚ РІ СЃС‚Р°С‚СѓСЃР±Р°СЂ (ui:statusbar)
- host.log(level, message), host.get_setting(key), host.set_setting(key, value), host.export_html(html)
- md.to_html(text), md.to_html_mapped(text), json.encode(value), json.decode(s)

РќРµ РїСѓС‚Р°Р№ С‚СЂРё РјРµС…Р°РЅРёР·РјР°:
- Р’РєР»Р°РґРєР° (view): РѕР±СЉСЏРІРё РІ contributes.views [{"kind": "<kind>", "title": "вЂ¦", "tier": 1}] (СЂРѕРІРЅРѕ СЌС‚Рё РїРѕР»СЏ),
  permission view:create; СЃРѕРґРµСЂР¶РёРјРѕРµ РїРёС€Рё С‡РµСЂРµР· host.set_view_content(VIEW_ID, html) (РЅСѓР¶РµРЅ view:modify), РіРґРµ
  VIEW_ID = "<id РїР»Р°РіРёРЅР°>:<kind>": <id РїР»Р°РіРёРЅР°> вЂ” СЌС‚Рѕ РїРѕР»Рµ id РёР· СЌС‚РѕРіРѕ Р¶Рµ JSON, <kind> вЂ” РёР· contributes.views.
  HTML СЃР°РЅРёС‚РёР·РёСЂСѓРµС‚СЃСЏ РЅР° С…РѕСЃС‚Рµ.
- РЎС‚Р°С‚СѓСЃР±Р°СЂ: РєРѕСЂРѕС‚РєРёР№ С‚РµРєСЃС‚ РїРѕРєР°Р·С‹РІР°Р№ С‡РµСЂРµР· host.show_message(text) (permission ui:statusbar);
  host.set_view_content РґР»СЏ СЃС‚Р°С‚СѓСЃР±Р°СЂР° РќР• РёСЃРїРѕР»СЊР·СѓР№.
- РљРѕРјР°РЅРґР°: РѕР±СЉСЏРІРё РІ contributes.commands Рё РѕР±СЂР°Р±Р°С‚С‹РІР°Р№ СЃРѕР±С‹С‚РёРµ command:invoked {command_id}.

permissions вЂ” С‚РѕР»СЊРєРѕ РёР· РЅР°Р±РѕСЂР°: document:read, document:write, view:create, view:modify, ui:statusbar.
id вЂ” С‚РѕР»СЊРєРѕ [a-z0-9_-]. Р•СЃР»Рё РІРєР»Р°РґРєР°/РєРѕРјР°РЅРґР°/СЃС‚Р°С‚СѓСЃР±Р°СЂ РЅРµ РЅСѓР¶РЅС‹ вЂ” РєР»СЋС‡ contributes РЅРµ РґРѕР±Р°РІР»СЏР№.

РџСЂРёРјРµСЂ РєР°СЂРєР°СЃР° (РїРѕРґСЃС‚СЂРѕР№ РїРѕРґ Р·Р°РґР°С‡Сѓ, РЅРµ РєРѕРїРёСЂСѓР№ РґРѕСЃР»РѕРІРЅРѕ):
\`\`\`lua
local VIEW_ID = "my-plugin:stats"   -- "<id РїР»Р°РіРёРЅР°>:<kind>"

function on_activate(ctx)
  ctx.subscribe("document:changed", function(name, payload)
    local text, err = host.get_document_text(payload.doc_id)
    if err == nil and text then host.set_view_content(VIEW_ID, "<p>РґР»РёРЅР° " .. #text .. "</p>") end
  end)
end
\`\`\``,
    category: "plugins",
  },
];

const ERROR_TITLES: Record<ErrorKind, string> = {
  auth: "РќРµРІРµСЂРЅС‹Р№ API-РєР»СЋС‡",
  offline: "РќРµС‚ СЃРѕРµРґРёРЅРµРЅРёСЏ",
  rate: "РџСЂРµРІС‹С€РµРЅ Р»РёРјРёС‚ Р·Р°РїСЂРѕСЃРѕРІ",
  context: "Р”РѕРєСѓРјРµРЅС‚ СЃР»РёС€РєРѕРј Р±РѕР»СЊС€РѕР№",
  generic: "РћС€РёР±РєР° AI",
};


function errorTitle(kind: ErrorKind, code: string): string {
  if (code === "ai_no_key") return "РљР»СЋС‡ routerai.ru РЅРµ Р·Р°РґР°РЅ";
  if (code === "ai_no_consent") return "РќСѓР¶РЅРѕ СЃРѕРіР»Р°СЃРёРµ РЅР° РѕС‚РїСЂР°РІРєСѓ";
  if (code === "ai_disabled") return "AI РІС‹РєР»СЋС‡РµРЅ";
  return ERROR_TITLES[kind];
}

export interface AiPanelOptions {
  /** РљРѕРЅС‚РµР№РЅРµСЂ С‚РµР»Р° СЂРµР¶РёРјР° AI (#ai-mode). */
  root: HTMLElement;
  /** РЁРѕРІ Р·Р°РїРёСЃРё РІ Р¶СѓСЂРЅР°Р»: РІР»Р°РґРµР»РµС† Р»РµРЅС‚С‹ вЂ” logPanel (Р±РµР· СЃРІРѕРµР№ РїРѕРґРїРёСЃРєРё). */
  log(level: LogLevel, category: LogCategory, message: string, detail?: string): void;
  /** РўРµРєСѓС‰РёР№ С‚РµРєСЃС‚ Р°РєС‚РёРІРЅРѕРіРѕ РґРѕРєСѓРјРµРЅС‚Р° (РґР»СЏ РїРѕРґСЃС‚Р°РЅРѕРІРєРё {{doc}}). */
  getDocumentText(): string;
  /** РђРєС‚РёРІРЅС‹Р№ РґРѕРєСѓРјРµРЅС‚ РґР»СЏ harness-РїСЂР°РІРѕРє (id + rev) РёР»Рё `null`, РµСЃР»Рё РѕС‚РєСЂС‹С‚РѕРіРѕ РЅРµС‚. */
  getActiveDocument(): { id: DocumentId; rev: number } | null;
}

export interface AiPanel {
  /** Р РµР°РєС†РёСЏ РЅР° СЃРјРµРЅСѓ СЂРµР¶РёРјР°/СЃРѕСЃС‚РѕСЏРЅРёСЏ РґРѕРєР°: С„РѕРєСѓСЃ РІРІРѕРґР°, Р·Р°РєСЂС‹С‚РёРµ РїРѕРїРѕРІРµСЂР°. */
  onDockState(mode: DockMode, state: DockState): void;
  dispose(): void;
}


function errorKind(code: string, message: string): ErrorKind {
  const c = code.toLowerCase();
  const m = message.toLowerCase();
  if (c.includes("key") || c.includes("auth") || c.includes("401") || m.includes("401") || m.includes("РєР»СЋС‡")) {
    return "auth";
  }
  if (
    c.includes("offline") ||
    c.includes("network") ||
    c.includes("timeout") ||
    m.includes("СЃРѕРµРґРёРЅ") ||
    m.includes("network") ||
    m.includes("timeout") ||
    m.includes("РѕС„Р»Р°Р№РЅ")
  ) {
    return "offline";
  }
  if (c.includes("rate") || m.includes("429") || m.includes("Р»РёРјРёС‚")) return "rate";
  if (c.includes("context") || m.includes("context length") || m.includes("СЃР»РёС€РєРѕРј Р±РѕР»СЊС€РѕР№")) {
    return "context";
  }
  return "generic";
}


function byId<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`aiPanel: РЅРµ РЅР°Р№РґРµРЅ СЌР»РµРјРµРЅС‚ #${id}`);
  return el as T;
}

/** Р—РЅР°С‡РµРЅРёРµ СЃ РїСЂРѕРІРµСЂРєРѕР№ РЅР° null (РґР»СЏ `querySelector`/`closest`). */
function must<T>(value: T | null, what: string): T {
  if (!value) throw new Error(`aiPanel: РЅРµ РЅР°Р№РґРµРЅ ${what}`);
  return value;
}

/** В«14:31:02В» вЂ” РІСЂРµРјСЏ СЃРѕРѕР±С‰РµРЅРёСЏ. */
function nowTime(): string {
  const d = new Date();
  const p = (n: number): string => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** РљРѕСЂРѕС‚РєРѕРµ РёРјСЏ РјРѕРґРµР»Рё РґР»СЏ СЃС‚СЂРѕРєРё РІ С€Р°РїРєРµ: В«routerai В· deepseek-v4.1-flashВ». */
function shortModel(model: string): string {
  const parts = model.split("/");
  return parts[parts.length - 1] || model;
}

/** Р›РёРјРёС‚ СЃС‚СЂРѕРє РґРёС„С„Р° РІ РєР°СЂС‚РѕС‡РєРµ: Р·Р°С‰РёС‚Р° РѕС‚ РѕРіСЂРѕРјРЅС‹С… РїСЂР°РІРѕРє (ADR-0032, 10 000 СЃС‚СЂРѕРє). */
const MAX_DIFF_LINES = 500;

const PLUGIN_STATUS_LABELS: Record<PluginStatus["state"], string> = {
  active: "Р°РєС‚РёРІРµРЅ",
  failed: "РѕС€РёР±РєР°",
  quarantined: "РєР°СЂР°РЅС‚РёРЅ",
  stopped: "РѕСЃС‚Р°РЅРѕРІР»РµРЅ",
};

/** РљРѕРґРѕРІС‹Р№ Р±Р»РѕРє ```json РІ РѕС‚РІРµС‚Рµ РјРѕРґРµР»Рё вЂ” РїСЂРµРґР»РѕР¶РµРЅРёРµ РїР»Р°РіРёРЅР° (СЃРєРёР»Р» create-plugin, ADR-0034/ADR-0026). */
const PLUGIN_FENCE = /```json[^\n]*\n([\s\S]*?)```/gi;

interface AiPluginProposal {
  id: string;
  name: string;
  version: string;
  source: string;
  /** РЎРѕР±СЂР°РЅРЅС‹Р№ `plugin.json` (Р±РµР· `source`) РґР»СЏ `ai_validate_plugin`/`ai_create_plugin`. */
  manifest: Json;
}

/** РљР°СЂС‚РѕС‡РєР° РїР»Р°РіРёРЅР°, Р·Р°СЂРµРіРёСЃС‚СЂРёСЂРѕРІР°РЅРЅР°СЏ РґР»СЏ СЂРµР°РєС†РёРё РЅР° СЃРѕР±С‹С‚РёСЏ `ai:plugin_*`. */
interface PluginCardState {
  id: string;
  /** РЈСЃС‚Р°РЅРѕРІР»РµРЅ Р»Рё РїР»Р°РіРёРЅ РєР°СЂС‚РѕС‡РєРё (РёРґРµРјРїРѕС‚РµРЅС‚РЅР°СЏ СЂРµР°РєС†РёСЏ РЅР° СЃРѕР±С‹С‚РёСЏ). */
  isInstalled(): boolean;
  installed(label: string, stateClass: string): void;
  deleted(): void;
}

/** РљРѕРґ IPC-РѕС€РёР±РєРё РёР· `CommandError { code, message }` (РёР»Рё `undefined`). */
function errorCode(e: unknown): string | undefined {
  if (e && typeof e === "object" && "code" in e) {
    const code = (e as { code?: unknown }).code;
    if (typeof code === "string") return code;
  }
  return undefined;
}

/** РЎРѕР·РґР°С‘С‚ СЌР»РµРјРµРЅС‚ СЃ РєР»Р°СЃСЃРѕРј: РєР°СЂС‚РѕС‡РєРё СЃРѕР±РёСЂР°СЋС‚СЃСЏ С‚РѕР»СЊРєРѕ DOM API (Р±РµР· innerHTML). */
function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  return node;
}

/** РљРЅРѕРїРєР° РєР°СЂС‚РѕС‡РєРё: С‚РёРї, РїРѕРґРїРёСЃСЊ Рё РґРѕСЃС‚СѓРїРЅРѕРµ РёРјСЏ (РґР»СЏ СЃРєСЂРёРЅСЂРёРґРµСЂРѕРІ). */
function actionButton(label: string, className: string, ariaLabel?: string): HTMLButtonElement {
  const button = el("button", className);
  button.type = "button";
  button.textContent = label;
  if (ariaLabel) button.setAttribute("aria-label", ariaLabel);
  return button;
}

/** Р Р°Р·Р±РёРІР°РµС‚ С‚РµРєСЃС‚ РЅР° СЃС‚СЂРѕРєРё; РїСѓСЃС‚РѕР№ РІРІРѕРґ вЂ” РїСѓСЃС‚РѕР№ СЃРїРёСЃРѕРє (РїСѓСЃС‚СѓСЋ СЃС‚СЂРѕРєСѓ РЅРµ СЂРёСЃСѓРµРј). */
function splitLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.split("\n");
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/** РЎС‚СЂРѕРєР° РґРёС„С„Р°: `+`/`-` + С‚РµРєСЃС‚ (С‚РѕР»СЊРєРѕ textContent); РїРµСЂРµРЅРѕСЃ РґР°С‘С‚ `display:block`. */
function appendDiffLine(pre: HTMLElement, kind: "add" | "del", line: string): void {
  const span = el("span", kind === "add" ? "ai-diff-add" : "ai-diff-del");
  span.textContent = `${kind === "add" ? "+" : "-"} ${line}`;
  pre.appendChild(span);
}



function renderDiffBody(pre: HTMLElement, diff: AiDiff): void {
  const maps = buildUnitMaps(diff.original);
  const ops = [...diff.ops].sort((a, b) => a.start - b.start);
  let shown = 0;
  let truncated = false;
  for (const op of ops) {
    const removed = diff.original.slice(bytesToUnits(maps, op.start), bytesToUnits(maps, op.end));
    for (const line of splitLines(removed)) {
      if (shown >= MAX_DIFF_LINES) {
        truncated = true;
        break;
      }
      appendDiffLine(pre, "del", line);
      shown++;
    }
    if (truncated) break;
    for (const line of splitLines(op.text)) {
      if (shown >= MAX_DIFF_LINES) {
        truncated = true;
        break;
      }
      appendDiffLine(pre, "add", line);
      shown++;
    }
    if (truncated) break;
  }
  if (truncated) {
    const note = el("span", "ai-diff-more");
    note.textContent = `вЂ¦ РґРёС„С„ СЃРѕРєСЂР°С‰С‘РЅ (Р»РёРјРёС‚ ${MAX_DIFF_LINES} СЃС‚СЂРѕРє)`;
    pre.appendChild(note);
  }
}


function extractPluginProposal(content: string): AiPluginProposal | null {
  PLUGIN_FENCE.lastIndex = 0; // РіР»РѕР±Р°Р»СЊРЅС‹Р№ regexp: СЃР±СЂР°СЃС‹РІР°РµРј РїРѕР·РёС†РёСЋ РјРµР¶РґСѓ РІС‹Р·РѕРІР°РјРё
  let match: RegExpExecArray | null;
  while ((match = PLUGIN_FENCE.exec(content)) !== null) {
    const proposal = parsePluginBlock(match[1]);
    if (proposal) return proposal;
  }
  return null;
}

/** Р Р°Р·Р±РёСЂР°РµС‚ JSON-Р±Р»РѕРє РїР»Р°РіРёРЅР° Рё СЃРѕР±РёСЂР°РµС‚ `plugin.json` (Р±РµР· `source`). */
function parsePluginBlock(raw: string): AiPluginProposal | null {
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!data || typeof data !== "object" || Array.isArray(data)) return null;
  const obj = data as Record<string, unknown>;
  const id = typeof obj.id === "string" ? obj.id.trim() : "";
  const source = typeof obj.source === "string" ? obj.source : "";
  if (!id || !source) return null;
  const name = typeof obj.name === "string" && obj.name.trim() ? obj.name.trim() : id;
  const version =
    typeof obj.version === "string" && obj.version.trim() ? obj.version.trim() : "0.1.0";
  const rawPermissions: unknown[] = Array.isArray(obj.permissions) ? obj.permissions : [];
  const permissions = rawPermissions.filter((p): p is string => typeof p === "string");

  const manifest: Json = { id, name, version, api_version: 1, entry: "main.lua", permissions };



  const contributes = obj.contributes;
  if (contributes && typeof contributes === "object" && !Array.isArray(contributes)) {
    manifest.contributes = contributes as Json;
  }
  return { id, name, version, source, manifest };
}


function confirmDeletePlugin(pluginId: string): Promise<boolean> {
  return new Promise((resolve) => {
    const content = el("div", "ai-confirm");
    const text = el("p", "ai-confirm-text");
    text.textContent = `РџР»Р°РіРёРЅ В«${pluginId}В» Р±СѓРґРµС‚ СѓРґР°Р»С‘РЅ Р±РµР·РІРѕР·РІСЂР°С‚РЅРѕ: С„Р°Р№Р»С‹ Рё РЅР°СЃС‚СЂРѕР№РєРё.`;
    content.appendChild(text);

    const buttons = el("div", "dialog-actions");
    const cancel = actionButton("РћС‚РјРµРЅР°", "btn ghost");
    const confirm = actionButton("РЈРґР°Р»РёС‚СЊ", "btn primary");
    buttons.append(cancel, confirm);
    content.appendChild(buttons);

    let dialog: Dialog | null = null;
    const finish = (value: boolean): void => {


      resolve(value);
      dialog?.close();
    };
    cancel.addEventListener("click", () => finish(false));
    confirm.addEventListener("click", () => finish(true));

    dialog = openDialog({
      title: "РЈРґР°Р»РµРЅРёРµ РїР»Р°РіРёРЅР°",
      content,
      initialFocus: cancel,
      onClose: () => resolve(false),
    });
  });
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
  const inputRow = must(input.closest<HTMLElement>(".ai-input"), "РєРѕРЅС‚РµР№РЅРµСЂ .ai-input");


  let settings: AiSettingsView | null = null;
  let skills: readonly AiSkill[] = [];
  let skillsLoaded = false;
  let skillsLoading = false;
  let consentDenied = false; // РїРѕР»СЊР·РѕРІР°С‚РµР»СЊ РѕС‚РєР°Р·Р°Р» РІ РѕС‚РїСЂР°РІРєРµ (Р¶С‘Р»С‚С‹Р№)
  let authError = false; // РЅРµРІРµСЂРЅС‹Р№ РєР»СЋС‡ (401) вЂ” Р¶С‘Р»С‚С‹Р№ В«РЅРµС‚ РєР»СЋС‡Р°В»
  let lastError = false; // РѕС€РёР±РєР° СЃРµС‚Рё/LLM (РєСЂР°СЃРЅС‹Р№)
  let busy = false;
  let stick = true;
  let popoverOpen = false;
  let wasAiExpanded = false;
  let history: AiMessage[] = [];
  let lastRequest: { display: string; content: string } | null = null;
  let disposed = false;
  const unlisteners: UnlistenFn[] = [];

  /** РўРµРєСѓС‰РёР№ СЃС‚СЂРёРјРёРЅРіРѕРІС‹Р№ Р·Р°РїСЂРѕСЃ (РѕРґРёРЅ Р·Р° СЂР°Р·). */
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
  emptyHint.textContent = "Р’С‹Р±РµСЂРёС‚Рµ СЃРєРёР»Р» РІС‹С€Рµ РёР»Рё РЅР°РїРёС€РёС‚Рµ Р·Р°РїСЂРѕСЃ.";
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
    if (st === "off") return "РІС‹РєР»СЋС‡РµРЅ";
    if (st === "error") return "РѕС€РёР±РєР° В· РЅРµС‚ СЃРѕРµРґРёРЅРµРЅРёСЏ";
    if (st === "ready") return "РґРѕСЃС‚СѓРїРµРЅ В· РІСЃС‚СЂРѕРµРЅ";
    if (authError) return "РЅРµРІРµСЂРЅС‹Р№ РєР»СЋС‡";
    if (settings && !settings.has_key) return "РЅРµС‚ РєР»СЋС‡Р°";
    return "РЅРµС‚ СЃРѕРіР»Р°СЃРёСЏ РЅР° РѕС‚РїСЂР°РІРєСѓ";
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
    enabledSub.textContent = enabled ? "РІРєР»СЋС‡С‘РЅ" : "РІС‹РєР»СЋС‡РµРЅ";
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
      if (modelLabel) modelLabel.textContent = `routerai В· ${shortModel(settings.model)}`;
    }
    keyHint.textContent = settings?.has_key ? "РљР»СЋС‡ СЃРѕС…СЂР°РЅС‘РЅ" : "РљР»СЋС‡ РЅРµ Р·Р°РґР°РЅ";
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



  /** Id РїР»Р°РіРёРЅРЅС‹С… СЃРєРёР»Р»РѕРІ (РєР°С‚РµРіРѕСЂРёСЏ `plugins`) вЂ” РґР»СЏ РёРєРѕРЅРєРё рџ§©; РѕСЃС‚Р°Р»СЊРЅС‹Рµ вЂ” РґРѕРєСѓРјРµРЅС‚РЅС‹Рµ в–¤. */
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
        log("warn", "system", `AI: СЃРєРёР»Р»С‹ РЅРµРґРѕСЃС‚СѓРїРЅС‹ (${errorMessage(e)}) вЂ” РёСЃРїРѕР»СЊР·СѓСЋС‚СЃСЏ РІСЃС‚СЂРѕРµРЅРЅС‹Рµ`);
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
      icon.textContent = PLUGIN_SKILL_IDS.includes(skill.id) ? "рџ§©" : "в–¤";
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
        log("info", "system", "AI: РґР»СЏ СЃРєРёР»Р»Р° В«РЎРѕР·РґР°С‚СЊ РїР»Р°РіРёРЅВ» РЅСѓР¶РЅРѕ РѕРїРёСЃР°РЅРёРµ РІ РїРѕР»Рµ РІРІРѕРґР°");
        return;
      }
      send(desc, tpl.replace("{{input}}", desc));
      return;
    }
    send(skill.title, tpl);
  }

  /** РћР±СЂРµР·РєР° С‚РµРєСЃС‚Р° РґРѕРєСѓРјРµРЅС‚Р° РґРѕ Р»РёРјРёС‚Р° Р±Р°Р№С‚ UTF-8 (Р±РµР· СЂР°Р·СЂС‹РІР° СЃРёРјРІРѕР»Р°). */
  function truncateDoc(text: string): string {
    const bytes = new TextEncoder().encode(text);
    if (bytes.length <= MAX_DOC_BYTES) return text;
    const cut = new TextDecoder().decode(bytes.subarray(0, MAX_DOC_BYTES));
    log("warn", "system", `AI: РґРѕРєСѓРјРµРЅС‚ РѕР±СЂРµР·Р°РЅ РґРѕ ${Math.round(MAX_DOC_BYTES / 1024)} РљР‘`);
    return cut.replace(/\uFFFD+$/u, "");
  }



  function appendUser(text: string): void {
    const row = document.createElement("div");
    row.className = "msg user";
    row.setAttribute("role", "article");
    row.setAttribute("aria-label", "РЎРѕРѕР±С‰РµРЅРёРµ РїРѕР»СЊР·РѕРІР°С‚РµР»СЏ");
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
    row.setAttribute("aria-label", "РЎРѕРѕР±С‰РµРЅРёРµ Р°СЃСЃРёСЃС‚РµРЅС‚Р°");
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
    el.textContent = `С‚РѕРєРµРЅС‹: ${usage.prompt_tokens} + ${usage.completion_tokens}`;
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
    icon.textContent = "вљ ";
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
      appendNotice(col, "РќРµС‚ Р°РєС‚РёРІРЅРѕРіРѕ РґРѕРєСѓРјРµРЅС‚Р° вЂ” РїСЂР°РІРєСѓ РїСЂРёРјРµРЅРёС‚СЊ РЅРµРєСѓРґР°.", "error");
      return false;
    }
    try {
      const diff = await aiDiff(doc.id, content);
      if (disposed) return false;
      if (diff.ops.length === 0) {
        appendNotice(col, "РџСЂР°РІРѕРє РЅРµ С‚СЂРµР±СѓРµС‚СЃСЏ вЂ” РѕС‚РІРµС‚ СЃРѕРІРїР°РґР°РµС‚ СЃ РґРѕРєСѓРјРµРЅС‚РѕРј.", "info");
        return false;
      }
      renderDiffCard(col, diff, doc.id, content);
      return true;
    } catch (e) {
      if (disposed) return false;
      appendNotice(col, `РќРµ СѓРґР°Р»РѕСЃСЊ РїРѕРґРіРѕС‚РѕРІРёС‚СЊ РїСЂР°РІРєСѓ: ${errorMessage(e)}`, "error");
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
    card.setAttribute("aria-label", "РџСЂРµРґР»Р°РіР°РµРјР°СЏ РїСЂР°РІРєР°");

    const header = el("div", "ai-diff-header");
    header.textContent = "РџСЂРµРґР»Р°РіР°РµРјР°СЏ РїСЂР°РІРєР°";

    const body = el("pre", "ai-diff-body");
    renderDiffBody(body, diff);

    const status = el("div", "ai-diff-status");
    status.setAttribute("role", "status");
    status.textContent = `РР·РјРµРЅРµРЅРёР№: ${diff.ops.length}.`;

    const actions = el("div", "ai-diff-actions");
    const apply = actionButton("РџСЂРёРјРµРЅРёС‚СЊ", "btn primary", "РџСЂРёРјРµРЅРёС‚СЊ РїСЂР°РІРєСѓ Рє РґРѕРєСѓРјРµРЅС‚Сѓ");
    apply.id = `ai-diff-apply-${id}`;
    const reject = actionButton("РћС‚РєР»РѕРЅРёС‚СЊ", "btn ghost", "РћС‚РєР»РѕРЅРёС‚СЊ РїСЂР°РІРєСѓ");
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
      const undo = actionButton("РћС‚РјРµРЅРёС‚СЊ", "btn ghost", "РћС‚РјРµРЅРёС‚СЊ РїСЂРёРјРµРЅС‘РЅРЅСѓСЋ РїСЂР°РІРєСѓ");
      undo.id = `ai-diff-undo-${id}`;
      undo.addEventListener("click", () => void undoEdit(undo));
      actions.appendChild(undo);
      status.textContent = `РџСЂР°РІРєР° РїСЂРёРјРµРЅРµРЅР° (rev ${rev}).`;
    }

    async function undoEdit(button: HTMLButtonElement): Promise<void> {
      button.disabled = true;
      try {
        await undoDocument(docId);
        if (disposed) return;
        status.textContent = "РџСЂР°РІРєР° РѕС‚РјРµРЅРµРЅР°.";
        actions.textContent = "";
      } catch (e) {
        if (disposed) return;
        button.disabled = false;
        status.textContent = `РќРµ СѓРґР°Р»РѕСЃСЊ РѕС‚РјРµРЅРёС‚СЊ: ${errorMessage(e)}`;
      }
    }

    async function applyEdit(): Promise<void> {
      apply.disabled = true;
      reject.disabled = true;
      status.textContent = "РџСЂРёРјРµРЅРµРЅРёРµвЂ¦";
      try {
        const res = await aiApplyEdit(docId, diff.base_rev, diff.ops);
        if (disposed) return;
        markApplied(res.rev);
      } catch (e) {
        if (disposed) return;
        apply.disabled = false;
        reject.disabled = false;
        if (errorCode(e) === "ai_stale_edit") {

          status.textContent = "Р”РѕРєСѓРјРµРЅС‚ РёР·РјРµРЅРёР»СЃСЏ, РїРµСЂРµСЃРјРѕС‚СЂРёС‚Рµ РїСЂР°РІРєСѓ.";
          const reconsider = actionButton(
            "РџРµСЂРµСЃРјРѕС‚СЂРµС‚СЊ",
            "btn ghost",
            "РџРµСЂРµСЃС‡РёС‚Р°С‚СЊ РїСЂР°РІРєСѓ РїРѕ Р°РєС‚СѓР°Р»СЊРЅРѕРјСѓ РґРѕРєСѓРјРµРЅС‚Сѓ"
          );
          reconsider.addEventListener("click", () => {
            card.remove();
            void buildDiff(col, content);
          });
          actions.appendChild(reconsider);
        } else {
          status.textContent = `РќРµ СѓРґР°Р»РѕСЃСЊ РїСЂРёРјРµРЅРёС‚СЊ: ${errorMessage(e)}`;
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
    card.setAttribute("aria-label", `РџР»Р°РіРёРЅ ${proposal.name} РІРµСЂСЃРёРё ${proposal.version}`);

    const header = el("div", "ai-plugin-header");
    const icon = el("span", "ai-plugin-icon");
    icon.setAttribute("aria-hidden", "true");
    icon.textContent = "рџ§©";
    const name = el("span", "ai-plugin-name");
    name.textContent = proposal.name;
    const version = el("span", "ai-plugin-version");
    version.textContent = `v${proposal.version}`;
    const badge = el("span", "ai-plugin-badge");
    badge.textContent = "РїСЂРµРґР»Р°РіР°РµС‚СЃСЏ";
    header.append(icon, name, version, badge);

    const code = el("pre", "ai-plugin-code");
    code.textContent = proposal.source; // СЃС‹СЂРѕР№ РІС‹РІРѕРґ РјРѕРґРµР»Рё вЂ” С‚РѕР»СЊРєРѕ textContent

    const status = el("div", "ai-plugin-status");
    status.setAttribute("role", "status");
    status.textContent = "РџСЂРѕРІРµСЂРєР°вЂ¦";

    const actions = el("div", "ai-plugin-actions");
    const save = actionButton("РЎРѕР·РґР°С‚СЊ", "btn primary", `РЎРѕР·РґР°С‚СЊ РїР»Р°РіРёРЅ ${proposal.id}`);
    save.id = `ai-plugin-save-${id}`;
    const reload = actionButton(
      "РџРµСЂРµР·Р°РіСЂСѓР·РёС‚СЊ",
      "btn ghost",
      `РџРµСЂРµР·Р°РіСЂСѓР·РёС‚СЊ РїР»Р°РіРёРЅ ${proposal.id}`
    );
    reload.id = `ai-plugin-reload-${id}`;
    reload.disabled = true; // РґРѕСЃС‚СѓРїРЅР° С‚РѕР»СЊРєРѕ РєРѕРіРґР° РїР»Р°РіРёРЅ СѓР¶Рµ СѓСЃС‚Р°РЅРѕРІР»РµРЅ
    const remove = actionButton("РЈРґР°Р»РёС‚СЊ", "btn ghost danger", `РЈРґР°Р»РёС‚СЊ РїР»Р°РіРёРЅ ${proposal.id}`);
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
      save.textContent = "РЎРѕС…СЂР°РЅРёС‚СЊ";
      save.setAttribute("aria-label", `РЎРѕС…СЂР°РЅРёС‚СЊ РїР»Р°РіРёРЅ ${proposal.id}`);
      save.disabled = false;
      reload.disabled = false;
      remove.disabled = false;
      status.classList.remove("is-error");
    }

    function markDeleted(): void {
      exists = false;
      badge.className = "ai-plugin-badge stopped";
      badge.textContent = "СѓРґР°Р»С‘РЅ";
      save.disabled = true;
      reload.disabled = true;
      remove.disabled = true;
      status.classList.remove("is-error", "is-ok");
      status.textContent = "РџР»Р°РіРёРЅ СѓРґР°Р»С‘РЅ.";
      pluginCards.delete(state);
    }


    async function validate(): Promise<void> {
      status.textContent = "РџСЂРѕРІРµСЂРєР°вЂ¦";
      try {
        const res = await aiValidatePlugin(proposal.id, proposal.manifest, proposal.source);
        if (disposed) return;
        status.classList.toggle("is-ok", res.ok);
        status.classList.toggle("is-error", !res.ok);
        status.textContent = res.ok
          ? "РџСЂРѕРІРµСЂРєР°: вњ“ РїСЂРѕР№РґРµРЅР°"
          : `РџСЂРѕРІРµСЂРєР° РЅРµ РїСЂРѕР№РґРµРЅР°: ${res.errors.join("; ") || "РЅРµРёР·РІРµСЃС‚РЅР°СЏ РѕС€РёР±РєР°"}`;
      } catch (e) {
        if (disposed) return;
        status.classList.remove("is-ok");
        status.classList.add("is-error");
        status.textContent = `РџСЂРѕРІРµСЂРєР° РЅРµРґРѕСЃС‚СѓРїРЅР°: ${errorMessage(e)}`;
      }
    }


    async function savePlugin(): Promise<void> {
      save.disabled = true;
      reload.disabled = true;
      remove.disabled = true;
      status.classList.remove("is-error", "is-ok");
      status.textContent = exists ? "РЎРѕС…СЂР°РЅРµРЅРёРµвЂ¦" : "РЎРѕР·РґР°РЅРёРµвЂ¦";
      try {
        const created = !exists;
        const info = exists
          ? await aiUpdatePlugin(proposal.id, proposal.manifest, proposal.source)
          : await aiCreatePlugin(proposal.id, proposal.manifest, proposal.source);
        if (disposed) return;
        markInstalled(PLUGIN_STATUS_LABELS[info.status.state], info.status.state);
        status.classList.add("is-ok");
        status.textContent = created ? "РџР»Р°РіРёРЅ СЃРѕР·РґР°РЅ." : "РџР»Р°РіРёРЅ РѕР±РЅРѕРІР»С‘РЅ.";
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
        save.textContent = "РЎРѕС…СЂР°РЅРёС‚СЊ";
        save.setAttribute("aria-label", `РЎРѕС…СЂР°РЅРёС‚СЊ РїР»Р°РіРёРЅ ${proposal.id}`);
        reload.disabled = false;
        remove.disabled = false;
        status.textContent = "РџР»Р°РіРёРЅ СѓР¶Рµ СЃСѓС‰РµСЃС‚РІСѓРµС‚ вЂ” РЅР°Р¶РјРёС‚Рµ В«РЎРѕС…СЂР°РЅРёС‚СЊВ», С‡С‚РѕР±С‹ РѕР±РЅРѕРІРёС‚СЊ.";
      } else if (code === "ai_plugin_invalid") {
        status.textContent = `РџР»Р°РіРёРЅ РЅРµ РїСЂРѕС€С‘Р» РїСЂРѕРІРµСЂРєСѓ: ${errorMessage(e)}`;
      } else {
        status.textContent = `РќРµ СѓРґР°Р»РѕСЃСЊ СЃРѕР·РґР°С‚СЊ РїР»Р°РіРёРЅ: ${errorMessage(e)}`;
      }
    }

    async function reloadPluginCard(): Promise<void> {
      reload.disabled = true;
      try {
        await reloadPlugin(proposal.id);
        if (disposed) return;
        status.classList.remove("is-error");
        status.textContent = "РџР»Р°РіРёРЅ РїРµСЂРµР·Р°РіСЂСѓР¶РµРЅ.";
      } catch (e) {
        if (disposed) return;
        status.classList.add("is-error");
        status.textContent = `РќРµ СѓРґР°Р»РѕСЃСЊ РїРµСЂРµР·Р°РіСЂСѓР·РёС‚СЊ: ${errorMessage(e)}`;
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
        status.textContent = `РќРµ СѓРґР°Р»РѕСЃСЊ СѓРґР°Р»РёС‚СЊ: ${errorMessage(e)}`;
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
      "РџСЂРёРјРµРЅРёС‚СЊ РєР°Рє РїСЂР°РІРєСѓ",
      "btn ghost",
      "РџСЂРёРјРµРЅРёС‚СЊ РѕС‚РІРµС‚ Р°СЃСЃРёСЃС‚РµРЅС‚Р° РєР°Рє РїСЂР°РІРєСѓ РґРѕРєСѓРјРµРЅС‚Р°"
    );
    apply.addEventListener("click", () => void requestDiff(col, content, apply));
    actions.appendChild(apply);
    col.appendChild(actions);

    const proposal = extractPluginProposal(content);
    if (proposal) renderPluginCard(col, proposal);
  }



  /** В«СЃРµСЃСЃРёСЏ РѕС‚ 14.10, 16:23В» вЂ” С„РѕСЂРјР°С‚ dd.mm, HH:MM РёР· unix-СЃРµРєСѓРЅРґ. */
  function formatSessionDate(unixSecs: number): string {
    const d = new Date(unixSecs * 1000);
    const p = (n: number): string => String(n).padStart(2, "0");
    return `${p(d.getDate())}.${p(d.getMonth() + 1)}, ${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  function showSessionHeader(updatedAt: number): void {
    if (!sessionHeader) return;
    sessionHeader.textContent = `СЃРµСЃСЃРёСЏ РѕС‚ ${formatSessionDate(updatedAt)}`;
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
      log("warn", "system", `AI: РёСЃС‚РѕСЂРёСЏ РЅРµРґРѕСЃС‚СѓРїРЅР° (${errorMessage(e)})`);
    }
  }

  /** Fire-and-forget СЃРѕС…СЂР°РЅРµРЅРёРµ РёСЃС‚РѕСЂРёРё; РѕС€РёР±РєР° вЂ” С‚РѕР»СЊРєРѕ РІ Р»РѕРі. */
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
        log("warn", "system", `AI: РёСЃС‚РѕСЂРёСЏ РЅРµ СЃРѕС…СЂР°РЅРµРЅР° (${errorMessage(e)})`);
      });
  }



  function errorActions(kind: ErrorKind, code: string): ErrorAction[] {
    const c = code.toLowerCase();
    if (kind === "auth") return [{ label: "РћС‚РєСЂС‹С‚СЊ РЅР°СЃС‚СЂРѕР№РєРё", run: openSettings }];

    if (kind === "context" || c === "ai_no_consent" || c === "ai_disabled") return [];
    return [{ label: "РџРѕРІС‚РѕСЂРёС‚СЊ", run: retry }];
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
      log("warn", "system", `AI: Markdown-СЂРµРЅРґРµСЂ РѕС‚РІРµС‚Р° РЅРµРґРѕСЃС‚СѓРїРµРЅ вЂ” РїРѕРєР°Р·Р°РЅ С‚РµРєСЃС‚ (${errorMessage(e)})`);
    }
  }


  function complete(p: Pending, usage: AiUsage | null): void {
    p.finalized = true;
    pending = null;
    setBusy(false);
    const isEmpty = p.content === "";
    const truncated = p.finishReason === "length";


    const content = isEmpty ? (truncated ? "" : "РџСѓСЃС‚РѕР№ РѕС‚РІРµС‚") : p.content;

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
      appendNotice(p.col, "РћС‚РІРµС‚ РѕР±РѕСЂРІР°РЅ Р»РёРјРёС‚РѕРј С‚РѕРєРµРЅРѕРІ вЂ” СѓРІРµР»РёС‡СЊС‚Рµ max_tokens РёР»Рё РїРѕРІС‚РѕСЂРёС‚Рµ Р·Р°РїСЂРѕСЃ.", "error");
    }
    authError = false;
    lastError = false;
    log(
      "info",
      "system",
      `AI: РѕС‚РІРµС‚ РїРѕР»СѓС‡РµРЅ${usage ? ` (prompt ${usage.prompt_tokens}, completion ${usage.completion_tokens})` : ""}`
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
    log("error", "system", `AI: ${errorTitle(kind, c)} вЂ” ${message}`);
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
      log("warn", "system", `AI: РёСЃС‚РѕСЂРёСЏ РЅРµ РѕС‡РёС‰РµРЅР° (${errorMessage(e)})`);
    });
  }



  function askConsent(onAllowed: () => void): void {
    let accepted = false;
    const content = document.createElement("div");
    content.className = "ai-consent";
    const p = document.createElement("p");

    p.textContent =
      "AI-Р°СЃСЃРёСЃС‚РµРЅС‚ РјРѕР¶РµС‚ РѕС‚РїСЂР°РІР»СЏС‚СЊ СЃРѕРґРµСЂР¶РёРјРѕРµ РґРѕРєСѓРјРµРЅС‚Р° Рё РёСЃС‚РѕСЂРёСЋ С‡Р°С‚Р° РІ РѕР±Р»Р°С‡РЅС‹Р№ СЃРµСЂРІРёСЃ routerai.ru (РґР»СЏ Р·Р°РїСЂРѕСЃР° Рє РјРѕРґРµР»Рё deepseek/deepseek-v4.1-flash). Р Р°Р·СЂРµС€РёС‚СЊ?";
    const warn = document.createElement("div");
    warn.className = "ai-consent-warning";
    warn.textContent = "РЎРѕРґРµСЂР¶РёРјРѕРµ СѓР№РґС‘С‚ РЅР° РІРЅРµС€РЅРёР№ СЃРµСЂРІРµСЂ. РќРµ РѕС‚РїСЂР°РІР»СЏР№С‚Рµ РєРѕРЅС„РёРґРµРЅС†РёР°Р»СЊРЅС‹Рµ РґРѕРєСѓРјРµРЅС‚С‹.";
    const actions = document.createElement("div");
    actions.className = "dialog-actions";
    const deny = document.createElement("button");
    deny.type = "button";
    deny.className = "btn ghost";
    deny.textContent = "РќРµ СЂР°Р·СЂРµС€Р°С‚СЊ";
    const allow = document.createElement("button");
    allow.type = "button";
    allow.className = "btn primary";
    allow.textContent = "Р Р°Р·СЂРµС€РёС‚СЊ";
    actions.append(deny, allow);
    content.append(p, warn, actions);

    const dialog: Dialog = openDialog({
      title: "РћС‚РїСЂР°РІРєР° СЃРѕРґРµСЂР¶РёРјРѕРіРѕ РІ routerai.ru",
      content,
      initialFocus: allow,
      onClose: () => {
        if (accepted) {
          void grantConsent().then(onAllowed);
        } else {
          consentDenied = true;
          applyState();
          log("info", "system", "AI: РѕС‚РїСЂР°РІРєР° РѕС‚РєР»РѕРЅРµРЅР° (РЅРµС‚ СЃРѕРіР»Р°СЃРёСЏ)");
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
      log("info", "system", "AI: СЃРѕРіР»Р°СЃРёРµ РЅР° РѕС‚РїСЂР°РІРєСѓ РїРѕР»СѓС‡РµРЅРѕ");
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
      log("error", "system", `AI: РЅР°СЃС‚СЂРѕР№РєРё РЅРµ СЃРѕС…СЂР°РЅРµРЅС‹ (${errorMessage(e)})`);
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
      log("info", "system", key ? "AI: API-РєР»СЋС‡ СЃРѕС…СЂР°РЅС‘РЅ" : "AI: API-РєР»СЋС‡ СѓРґР°Р»С‘РЅ");
    } catch (e) {
      log("error", "system", `AI: РєР»СЋС‡ РЅРµ СЃРѕС…СЂР°РЅС‘РЅ (${errorMessage(e)})`);
    }
  }

  async function testConnection(): Promise<void> {
    testBtn.disabled = true;
    testResult.textContent = "РџСЂРѕРІРµСЂРєР° СЃРѕРµРґРёРЅРµРЅРёСЏвЂ¦";
    try {
      const res = await aiTestConnection();
      if (res.ok) {
        authError = false;
        lastError = false;
        testResult.textContent = res.model ? `РЎРѕРµРґРёРЅРµРЅРёРµ СѓСЃС‚Р°РЅРѕРІР»РµРЅРѕ В· ${res.model}` : "РЎРѕРµРґРёРЅРµРЅРёРµ СѓСЃС‚Р°РЅРѕРІР»РµРЅРѕ";
        log("info", "system", "AI: СЃРѕРµРґРёРЅРµРЅРёРµ СѓСЃС‚Р°РЅРѕРІР»РµРЅРѕ");
      } else {
        if (settings?.has_key) lastError = true; // В«РЅРµС‚ РєР»СЋС‡Р°В» РѕСЃС‚Р°С‘С‚СЃСЏ Р¶С‘Р»С‚С‹Рј
        testResult.textContent = res.error ? `РћС€РёР±РєР°: ${res.error}` : "РќРµ СѓРґР°Р»РѕСЃСЊ СЃРѕРµРґРёРЅРёС‚СЊСЃСЏ";
        log("warn", "system", `AI: РїСЂРѕРІРµСЂРєР° СЃРѕРµРґРёРЅРµРЅРёСЏ РЅРµ СѓРґР°Р»Р°СЃСЊ${res.error ? ` (${res.error})` : ""}`);
      }
      applyState();
    } catch (e) {
      if (settings?.has_key) lastError = true;
      testResult.textContent = `РћС€РёР±РєР°: ${errorMessage(e)}`;
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
      log("warn", "system", `AI: СЃСЃС‹Р»РєР° РЅРµ РѕС‚РєСЂС‹С‚Р° вЂ” ${errorMessage(err)}`);
    });
  };

  const onSettingsClick = (): void => toggleSettings();
  const onSettingsClose = (): void => closeSettings();
  const onKeySave = (): void => void saveKey();
  const onKeyClear = (): void => {
    keyInput.value = ""; // СЏРІРЅР°СЏ РѕС‡РёСЃС‚РєР°: РїСѓСЃС‚РѕР№ РєР»СЋС‡ в†’ ai_set_api_key("")
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
    if (p.messageId !== null && p.messageId !== message_id) return; // С‡Р°РЅРє С‡СѓР¶РѕРіРѕ/СѓСЃС‚Р°СЂРµРІС€РµРіРѕ РѕС‚РІРµС‚Р°
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
        card.installed("СѓСЃС‚Р°РЅРѕРІР»РµРЅ", "active");
      }
    }
  });

  subscribe<{ plugin_id: string }>("ai:plugin_updated", (event) => {
    for (const card of pluginCards) {
      if (card.id === event.payload.plugin_id && !card.isInstalled()) {
        card.installed("РѕР±РЅРѕРІР»С‘РЅ", "active");
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
      log("error", "system", `AI: РЅР°СЃС‚СЂРѕР№РєРё РЅРµРґРѕСЃС‚СѓРїРЅС‹ (${errorMessage(e)})`);
      applyState();
    });
  void restoreHistory().then(() => {
    if (disposed) return;
    applyState();
  });

  renderSkills(); // С‡РёРї В«СЃРєРѕСЂРѕВ» РІРёРґРµРЅ СЃСЂР°Р·Сѓ; Р±РѕРµРІС‹Рµ СЃРєРёР»Р»С‹ РґРѕР±Р°РІСЏС‚СЃСЏ РїРѕСЃР»Рµ Р·Р°РіСЂСѓР·РєРё
  applyState();
  updateEmpty();

  return { onDockState, dispose };
}
