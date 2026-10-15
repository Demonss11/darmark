// Согласие на отправку содержимого документа во внешний сервис.
//
// Показывается один раз за сессию до первого запроса; отказ запоминается в
// `consentDenied`, чтобы диалог не спрашивал заново на каждый ввод. Разрешение
// сохраняется через `persist` — это обычная настройка, поэтому новый модуль не
// нужен и состояние согласия живёт в общем.

import { openDialog, type Dialog } from "../../ui/dialog";
import type { AiSettingsInput } from "../../core/tauri";
import type { IndicatorApi } from "./indicator";
import type { AiPanelState } from "./state";

export interface ConsentApi {
  askConsent(onAllowed: () => void): void;
}

export function createConsent(
  state: AiPanelState,
  deps: { persist(patch: Partial<AiSettingsInput>): Promise<void>; indicator: IndicatorApi }
): ConsentApi {
  function askConsent(onAllowed: () => void): void {
    let accepted = false;
    const content = document.createElement("div");
    content.className = "ai-consent";
    const p = document.createElement("p");
    p.textContent =
      "AI-ассистент может отправлять содержимое документа и историю чата в облачный сервис routerai.ru (для запроса к модели deepseek/deepseek-v4.1-flash). Разрешить?";
    const warn = document.createElement("div");
    warn.className = "ai-consent-warning";
    warn.textContent =
      "Содержимое уйдёт на внешний сервер. Не отправляйте конфиденциальные документы.";
    const actions = document.createElement("div");
    actions.className = "dialog-actions";
    const deny = document.createElement("button");
    deny.type = "button";
    deny.className = "btn ghost";
    deny.textContent = "Не разрешать";
    const allow = document.createElement("button");
    allow.type = "button";
    allow.className = "btn primary";
    allow.textContent = "Разрешить";
    actions.append(deny, allow);
    content.append(p, warn, actions);

    const dialog: Dialog = openDialog({
      title: "Отправка содержимого в routerai.ru",
      content,
      initialFocus: allow,
      onClose: () => {
        if (accepted) {
          void grantConsent().then(onAllowed);
        } else {
          state.consentDenied = true;
          deps.indicator.applyState();
          state.log("info", "system", "AI: отправка отклонена (нет согласия)");
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
    await deps.persist({ consent_document_send: true });
    if (state.settings?.consent_document_send) {
      state.consentDenied = false;
      deps.indicator.applyState();
      state.log("info", "system", "AI: согласие на отправку получено");
    }
  }

  return { askConsent };
}