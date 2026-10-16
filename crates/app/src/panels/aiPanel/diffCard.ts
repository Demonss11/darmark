





import type { DocumentId } from "../../core/ids";
import { aiApplyEdit, aiDiff, errorMessage, undoDocument } from "../../core/tauri";
import type { AiDiff } from "../../core/tauri";
import { actionButton, el } from "./dom";
import { renderDiffBody } from "./diff";
import { errorCode } from "./errors";
import type { AiPanelState } from "./state";

export interface DiffCardApi {
  requestDiff(col: HTMLElement, content: string, button: HTMLButtonElement): Promise<void>;
}

export function createDiffCard(
  state: AiPanelState,
  deps: {
    appendNotice(col: HTMLElement, text: string, kind: "info" | "error"): void;
    scrollToBottom(): void;
  }
): DiffCardApi {
  async function buildDiff(col: HTMLElement, content: string): Promise<boolean> {
    const doc = state.getActiveDocument();
    if (!doc) {
      deps.appendNotice(col, "Нет активного документа — правку применить некуда.", "error");
      return false;
    }
    try {
      const diff = await aiDiff(doc.id, content);
      if (state.disposed) return false;
      if (diff.ops.length === 0) {
        deps.appendNotice(col, "Правок не требуется — ответ совпадает с документом.", "info");
        return false;
      }
      renderDiffCard(col, diff, doc.id, content);
      return true;
    } catch (e) {
      if (state.disposed) return false;
      deps.appendNotice(col, `Не удалось подготовить правку: ${errorMessage(e)}`, "error");
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
    if (!state.disposed && !ok) button.disabled = false;
  }

  function renderDiffCard(
    col: HTMLElement,
    diff: AiDiff,
    docId: DocumentId,
    content: string
  ): void {
    const id = ++state.cardSeq;
    const card = el("div", "ai-diff");
    card.id = `ai-diff-${id}`;
    card.setAttribute("role", "group");
    card.setAttribute("aria-label", "Предлагаемая правка");

    const header = el("div", "ai-diff-header");
    header.textContent = "Предлагаемая правка";

    const body = el("pre", "ai-diff-body");
    renderDiffBody(body, diff);

    const status = el("div", "ai-diff-status");
    status.setAttribute("role", "status");
    status.textContent = `Изменений: ${diff.ops.length}.`;

    const actions = el("div", "ai-diff-actions");
    const apply = actionButton("Применить", "btn primary", "Применить правку к документу");
    apply.id = `ai-diff-apply-${id}`;
    const reject = actionButton("Отклонить", "btn ghost", "Отклонить правку");
    reject.id = `ai-diff-reject-${id}`;
    actions.append(apply, reject);

    card.append(header, body, status, actions);
    col.appendChild(card);
    if (state.stick) deps.scrollToBottom();

    let applied = false;

    function markApplied(rev: number): void {
      if (applied) return;
      applied = true;
      actions.textContent = "";
      const undo = actionButton("Отменить", "btn ghost", "Отменить применённую правку");
      undo.id = `ai-diff-undo-${id}`;
      undo.addEventListener("click", () => void undoEdit(undo));
      actions.appendChild(undo);
      status.textContent = `Правка применена (rev ${rev}).`;
    }

    async function undoEdit(button: HTMLButtonElement): Promise<void> {
      button.disabled = true;
      try {
        await undoDocument(docId);
        if (state.disposed) return;
        status.textContent = "Правка отменена.";
        actions.textContent = "";
      } catch (e) {
        if (state.disposed) return;
        button.disabled = false;
        status.textContent = `Не удалось отменить: ${errorMessage(e)}`;
      }
    }

    async function applyEdit(): Promise<void> {
      apply.disabled = true;
      reject.disabled = true;
      status.textContent = "Применение…";
      try {
        const res = await aiApplyEdit(docId, diff.base_rev, diff.ops);
        if (state.disposed) return;
        markApplied(res.rev);
      } catch (e) {
        if (state.disposed) return;
        apply.disabled = false;
        reject.disabled = false;
        if (errorCode(e) === "ai_stale_edit") {
          status.textContent = "Документ изменился, пересмотрите правку.";
          const reconsider = actionButton(
            "Пересмотреть",
            "btn ghost",
            "Пересчитать правку по актуальному документу"
          );
          reconsider.addEventListener("click", () => {
            card.remove();
            void buildDiff(col, content);
          });
          actions.appendChild(reconsider);
        } else {
          status.textContent = `Не удалось применить: ${errorMessage(e)}`;
        }
      }
    }

    function rejectEdit(): void {
      card.remove();
    }

    apply.addEventListener("click", () => void applyEdit());
    reject.addEventListener("click", rejectEdit);
  }

  return { requestDiff };
}