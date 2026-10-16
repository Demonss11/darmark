









import { openDialog, type Dialog } from "../../ui/dialog";
import type { AiSettingsInput } from "../../core/tauri";
import type { IndicatorApi } from "./indicator";
import type { AiPanelState } from "./state";

export interface ConsentApi {
  askConsent(onAllowed: () => void): void;
}



function hostOf(url: string | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).host || null;
  } catch {
    return null;
  }
}

export function createConsent(
  state: AiPanelState,
  deps: { persist(patch: Partial<AiSettingsInput>): Promise<void>; indicator: IndicatorApi }
): ConsentApi {
  function askConsent(onAllowed: () => void): void {
    const settings = state.settings;
    const provider = settings?.providers.find((item) => item.id === settings.active_provider);
    const name = provider?.name || provider?.id || "провайдер";
    const host = hostOf(provider?.settings.base_url) ?? "внешний сервер";
    const model = settings?.active_model ?? "";

    let accepted = false;
    const content = document.createElement("div");
    content.className = "ai-consent";
    const p = document.createElement("p");
    p.textContent =
      `AI-ассистент может отправлять содержимое документа и историю чата в облачный сервис ` +
      `${name} (${host})${model ? ` для запроса к модели ${model}` : ""}. Разрешить?`;
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
      title: `Отправка содержимого в ${host}`,
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
