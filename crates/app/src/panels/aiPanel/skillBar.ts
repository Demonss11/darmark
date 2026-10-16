// Ряд навыков над полем ввода: ленивая загрузка списка (aiListSkills) и
// подстановка шаблона в запрос.
//
// Зависимость на стриминг намеренно передаётся параметром: `createStream`
// объявлен в index.ts ниже, но `send` вызывается только из обработчика клика по
// навыку — то есть уже после того, как все фабрики созданы (никакого TDZ).

import { MAX_DOC_BYTES } from "./constants";
import { aiListSkills, errorMessage } from "../../core/tauri";
import type { AiSkill } from "../../core/tauri";
import type { AiPanelState } from "./state";

const PLUGIN_SKILL_IDS: readonly string[] = ["create-plugin", "debug-plugin"];

/// Скилл, к запросу которого дописывается выбранная цель плагина.
const CREATE_PLUGIN_SKILL_ID = "create-plugin";

/** Цель плагина у кнопки «Создать плагин»: строка-намерение дописывается в `{{input}}`.
 *  Компенсация удаления скилла add-view (TASK-004, C7) — вход в сценарий «вкладка»
 *  без второго промта, который дублировал бы API-блок. */
const PLUGIN_GOALS: readonly { id: string; label: string; hint: string }[] = [
  { id: "view", label: "вкладка", hint: "нужна собственная вкладка (view)" },
  { id: "statusbar", label: "статусбар", hint: "нужна запись в статусбар (host.show_message)" },
  { id: "command", label: "команда", hint: "нужна команда (contributes.commands)" },
];

/** Скиллы, результат которых применяется к документу: при усечении документа запрос не
 *  отправляем — модель не видела хвост и «перепишет» документ без него (потеря текста). */
const DOCUMENT_MUTATING_SKILL_IDS: readonly string[] = ["restructure"];

/// Обозначение лимита усечения документа для текстов в чате и журнале.
const DOC_LIMIT_LABEL = `${Math.round(MAX_DOC_BYTES / 1024)} КБ`;

/// Текст узла-подсказки на месте чипов, когда список скиллов не пришёл.
/// Неинтерактивный намеренно: чип без обработчика клика выглядит кнопкой и врёт.
const SKILLS_UNAVAILABLE_TEXT = "Скиллы недоступны — перезапустите ПЕНАЛ";

export interface SkillBarApi {
  ensureSkills(): void;
  renderSkills(): void;
}

export function createSkillBar(
  state: AiPanelState,
  deps: {
    send(display: string, content: string): void;
    appendSystemNote(text: string): void;
  }
): SkillBarApi {
  const { dom } = state;

  /**
   * Выбранная цель плагина: дописывается в `{{input}}` скилла «Создать плагин».
   * Живёт в замыкании, а не в `AiPanelState`, — выбор переживает перерисовку ряда
   * (`renderSkills` вызывается и при загрузке списка), но не обязан жить между
   * сессиями: цель — подсказка к конкретному описанию, а не настройка панели.
   */
  let selectedGoal: string | null = null;

  /**
   * Документ уходит в запрос целиком — обрезаем до лимита БАЙТ, а не символов.
   * Признак усечения возвращается наружу: записи в лог-панели недостаточно, для
   * пишущих скиллов усечение — повод не отправлять запрос вовсе (см. onSkillClick).
   */
  function truncateDoc(text: string): { text: string; truncated: boolean } {
    const bytes = new TextEncoder().encode(text);
    if (bytes.length <= MAX_DOC_BYTES) return { text, truncated: false };
    const cut = new TextDecoder().decode(bytes.subarray(0, MAX_DOC_BYTES));
    state.log("warn", "system", `AI: документ обрезан до ${DOC_LIMIT_LABEL}`);
    return { text: cut.replace(/\uFFFD+$/u, ""), truncated: true };
  }

  function ensureSkills(): void {
    if (state.skillsLoaded || state.skillsLoading) return;
    state.skillsLoading = true;
    void aiListSkills()
      .then((list) => {
        state.skills = list;
      })
      .catch((e) => {
        // Источник промтов один — при отказе IPC подставлять нечего: честный пустой
        // список превращается в подсказку в ряду (renderSkills), а не в чужие чипы.
        state.skills = [];
        state.log(
          "error",
          "system",
          `AI: список скиллов недоступен (${errorMessage(e)}) — перезапустите ПЕНАЛ`
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
    // Подсказка — только после завершённой загрузки: до неё пустой список означает
    // «ещё грузится», а не «недоступно» (index.ts зовёт renderSkills на монтировании).
    if (state.skills.length === 0 && state.skillsLoaded) {
      const hint = document.createElement("span");
      hint.className = "skill-unavailable";
      hint.textContent = SKILLS_UNAVAILABLE_TEXT;
      dom.skillsRow.appendChild(hint);
      return;
    }
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
      // Чип цели относится только к «Создать плагин» (TASK-009 §2/§7): группа стоит
      // вплотную за своей кнопкой, а не в хвосте ряда. Нет скилла — нет и группы,
      // поэтому условие проверяется на месте, а не отдельным проходом после цикла.
      if (skill.id === CREATE_PLUGIN_SKILL_ID) {
        btn.insertAdjacentElement("afterend", renderGoals());
      }
    }
  }

  /**
   * Группа переключателей цели плагина (`radiogroup`): выбранный чип помечен
   * `aria-checked` и классом `is-on`. Разметка строится через `createElement` —
   * в ряду скиллов нет ни одной разрешённой точки вставки HTML.
   */
  function renderGoals(): HTMLElement {
    const group = document.createElement("div");
    group.className = "ai-goals";
    group.setAttribute("role", "radiogroup");
    group.setAttribute("aria-label", "Цель плагина");
    for (const goal of PLUGIN_GOALS) {
      const on = selectedGoal === goal.id;
      const chip = document.createElement("button");
      chip.type = "button";
      chip.className = on ? "ai-goal is-on" : "ai-goal";
      chip.dataset.goal = goal.id;
      chip.setAttribute("role", "radio");
      chip.setAttribute("aria-checked", on ? "true" : "false");
      chip.title = goal.hint;
      chip.textContent = goal.label;
      chip.addEventListener("click", () => onGoalClick(goal.id));
      group.appendChild(chip);
    }
    return group;
  }

  /// Повторный клик по выбранному чипу снимает выбор: цель — уточнение, а не
  /// обязанность, и пустой выбор оставляет поведение запроса прежним.
  function onGoalClick(id: string): void {
    selectedGoal = selectedGoal === id ? null : id;
    dom.skillsRow.querySelector(".ai-goals")?.replaceWith(renderGoals());
  }

  function onSkillClick(skill: AiSkill): void {
    if (!state.settings?.enabled) return;
    const tpl = skill.prompt_template;
    if (tpl.includes("{{doc}}")) {
      const doc = truncateDoc(state.getDocumentText());
      // Пишущий скилл на усечённом документе опасен: модель «перепишет» только видимую
      // часть, а «Применить как правку» удалит хвост молча — поэтому запрос не уходит.
      if (doc.truncated && DOCUMENT_MUTATING_SKILL_IDS.includes(skill.id)) {
        deps.appendSystemNote(
          `Документ больше ${DOC_LIMIT_LABEL} — «${skill.title}» не запущен: ассистент ` +
            `перепишет только видимую часть, и применение правки удалит остальное. ` +
            `Разбейте документ на части`
        );
        dom.input.focus();
        return;
      }
      if (doc.truncated) {
        deps.appendSystemNote(`Документ обрезан до ${DOC_LIMIT_LABEL} — разбор неполный`);
      }
      // Функция-заменитель, а не строка: в тексте документа `$&`, `$'`, `` $` `` и
      // `$1` — спецпоследовательности replace и молча искажали бы промт.
      deps.send(skill.title, tpl.replace("{{doc}}", () => doc.text));
      return;
    }
    if (tpl.includes("{{input}}")) {
      const desc = dom.input.value.trim();
      if (!desc) {
        dom.input.focus();
        state.log(
          "info",
          "system",
          `AI: для скилла «${skill.title}» нужно описание в поле ввода`
        );
        return;
      }
      // Цель уточняет «что» (намерение), но не заменяет описание плагина: пустой ввод
      // по-прежнему блокирует запрос. В подстановку уходит дописанный текст, в чат —
      // то, что пользователь написал сам (C7).
      const goal = PLUGIN_GOALS.find((g) => g.id === selectedGoal);
      const hint = skill.id === CREATE_PLUGIN_SKILL_ID && goal ? `\n\n${goal.hint}` : "";
      // Функция-заменитель, а не шаблонная строка: `$&` и подобные последовательности
      // в описании пользователя не должны трактоваться правилами replace.
      deps.send(desc, tpl.replace("{{input}}", () => `${desc}${hint}`));
      return;
    }
    deps.send(skill.title, tpl);
  }

  return { ensureSkills, renderSkills };
}