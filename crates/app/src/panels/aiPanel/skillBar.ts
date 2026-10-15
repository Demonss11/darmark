// Ряд навыков над полем ввода: ленивая загрузка списка (aiListSkills) с
// откатом на встроенные `FALLBACK_SKILLS` и подстановка шаблона в запрос.
//
// Зависимость на стриминг намеренно передаётся параметром: `createStream`
// объявлен в index.ts ниже, но `send` вызывается только из обработчика клика по
// навыку — то есть уже после того, как все фабрики созданы (никакого TDZ).

import { MAX_DOC_BYTES } from "./constants";
import { aiListSkills, errorMessage } from "../../core/tauri";
import type { AiSkill } from "../../core/tauri";
import { FALLBACK_SKILLS } from "./skills";
import type { AiPanelState } from "./state";

const PLUGIN_SKILL_IDS: readonly string[] = ["create-plugin", "debug-plugin", "add-view"];

export interface SkillBarApi {
  ensureSkills(): void;
  renderSkills(): void;
}

export function createSkillBar(
  state: AiPanelState,
  deps: { send(display: string, content: string): void }
): SkillBarApi {
  const { dom } = state;

  /** Документ уходит в запрос целиком — обрезаем до лимита байт, а не символов. */
  function truncateDoc(text: string): string {
    const bytes = new TextEncoder().encode(text);
    if (bytes.length <= MAX_DOC_BYTES) return text;
    const cut = new TextDecoder().decode(bytes.subarray(0, MAX_DOC_BYTES));
    state.log("warn", "system", `AI: документ обрезан до ${Math.round(MAX_DOC_BYTES / 1024)} КБ`);
    return cut.replace(/\uFFFD+$/u, "");
  }

  function ensureSkills(): void {
    if (state.skillsLoaded || state.skillsLoading) return;
    state.skillsLoading = true;
    void aiListSkills()
      .then((list) => {
        state.skills = list.length > 0 ? list : FALLBACK_SKILLS;
      })
      .catch((e) => {
        state.skills = FALLBACK_SKILLS;
        state.log(
          "warn",
          "system",
          `AI: скиллы недоступны (${errorMessage(e)}) — используются встроенные`
        );
      })
      .finally(() => {
        state.skillsLoading = false;
        state.skillsLoaded = true;
        renderSkills();
      });
  }

  function renderSkills(): void {
    dom.skillsRow.textContent = "";
    for (const skill of state.skills) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "skill";
      btn.title = skill.description;
      btn.setAttribute("aria-label", `${skill.title}: ${skill.description}`);
      const icon = document.createElement("span");
      icon.className = "skill-icon";
      icon.setAttribute("aria-hidden", "true");
      icon.textContent = PLUGIN_SKILL_IDS.includes(skill.id) ? "🧩" : "▤";
      const title = document.createElement("span");
      title.textContent = skill.title;
      btn.append(icon, title);
      btn.addEventListener("click", () => onSkillClick(skill));
      dom.skillsRow.appendChild(btn);
    }
  }

  function onSkillClick(skill: AiSkill): void {
    if (!state.settings?.enabled) return;
    const tpl = skill.prompt_template;
    if (tpl.includes("{{doc}}")) {
      deps.send(skill.title, tpl.replace("{{doc}}", truncateDoc(state.getDocumentText())));
      return;
    }
    if (tpl.includes("{{input}}")) {
      const desc = dom.input.value.trim();
      if (!desc) {
        dom.input.focus();
        state.log(
          "info",
          "system",
          "AI: для скилла «Создать плагин» нужно описание в поле ввода"
        );
        return;
      }
      deps.send(desc, tpl.replace("{{input}}", desc));
      return;
    }
    deps.send(skill.title, tpl);
  }

  return { ensureSkills, renderSkills };
}