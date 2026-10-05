// Шаги синхронизации скролла (TZ-scroll-sync-v2).
//
// Ожидаемые позиции считаются независимо от кода sync: байтовые смещения
// data-md переводятся в номера строк (byteAtEditorLine / lineOfSubstring), а
// верхний блок предпросмотра — по геометрии DOM.
import { Given, Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  byteAtEditorLine,
  capturedErrors,
  editorTopLine,
  expectedBlockTextAtByte,
  hoverBlockFar,
  insertLinesAtTop,
  insertLinesAtTopAndScrollImmediately,
  lineOfSubstring,
  previewTopBlockText,
  rapidScrollEditorToSubstring,
  scrollEditorToSubstring,
  scrollInnerTable,
  scrollPreviewToBlock,
  scrollState,
  setEditorScrollTop,
  setMarkdown,
  setPreviewVisible,
  setSyncEnabled,
} from "./helpers.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixturesDir = path.resolve(here, "..", "fixtures");

// Состояние между шагами одного сценария (тесты идут последовательно).
const state = {
  previewBefore: 0,
  previewBeforeTable: 0,
  editorBeforeHover: 0,
  previewTopBeforeHover: null,
  rapidTarget: 0,
  editPct: 0,
  previewRatioAfterEdit: 0,
};

// ---------- генераторы документов ----------

function paragraphsDoc(n) {
  const lines = [
    "# Синхронизация скролла",
    "",
    "Вводный абзац для разгона высоты документа.",
    "",
  ];
  for (let i = 1; i <= n; i++) lines.push(`Абзац-${i}.`, "");
  return lines.join("\n");
}

function longLineDoc(n) {
  const long =
    "Очень длинная строка без мягких переносов: " +
    "моноширинный текст продолжается до горизонтальной прокрутки. ".repeat(5);
  const lines = ["# Длинный абзац", "", long.trim(), "", "После длинной строки.", ""];
  for (let i = 1; i <= n; i++) lines.push(`Короткий-${i}.`, "");
  return lines.join("\n");
}

function tableDoc(rows) {
  const lines = ["# Большая таблица", "", "| № | Значение |", "|---:|---|"];
  for (let i = 1; i <= rows; i++) lines.push(`| ${i} | Значение-${i} |`);
  return lines.join("\n");
}

// Неоднородный документ: высокая таблица (анкор и пропорция расходятся)
// плюс короткие абзацы для сценария «пропорция до рендера».
function mixedDoc(n) {
  const lines = ["# Неоднородный документ", "", "| № | Значение |", "|---:|---|"];
  for (let i = 1; i <= 40; i++) lines.push(`| ${i} | Значение-${i} |`);
  lines.push("", "Текст сразу после таблицы.", "");
  for (let i = 1; i <= n; i++) lines.push(`Абзац-${i}.`, "");
  return lines.join("\n");
}

// ---------- предусловия ----------

Given("синхронизация скролла включена", async () => {
  await setSyncEnabled(true);
});

Given("в редактор введён документ с {int} абзацами", async (n) => {
  await setMarkdown(paragraphsDoc(n));
});

Given("в редактор введён документ с большой таблицей", async () => {
  await setMarkdown(tableDoc(80));
});

Given("в редакторе длинный абзац без переноса и {int} коротких абзацев", async (n) => {
  await setMarkdown(longLineDoc(n));
});

Given("в редактор введён документ с таблицей и {int} абзацами", async (n) => {
  await setMarkdown(mixedDoc(n));
});

Given("в редакторе документ из фикстуры {string}", async (name) => {
  await setMarkdown(readFileSync(path.join(fixturesDir, name), "utf8"));
});

// ---------- действия ----------

When("я прокрутил редактор к абзацу {string}", async (text) => {
  const expectedLine = await lineOfSubstring(text);
  if (expectedLine < 0) throw new Error(`В редакторе нет ${JSON.stringify(text)}`);
  await scrollEditorToSubstring(text);
  const actualLine = await editorTopLine();
  if (actualLine !== expectedLine) {
    throw new Error(
      `Редактор не дошёл до строки ${expectedLine} (фактически ${actualLine}) — вероятно, упёрся в конец документа`
    );
  }
});

When("я прокрутил предпросмотр к блоку {string}", async (text) => {
  await scrollPreviewToBlock(text);
});

When("я прокрутил редактор на {int}% вниз", async (pct) => {
  const st = await scrollState("editor");
  await setEditorScrollTop((st.max * pct) / 100);
});

When("я выключил синхронизацию скролла", async () => {
  state.previewBefore = (await scrollState("preview")).scrollTop;
  await setSyncEnabled(false);
});

When("я включил синхронизацию скролла", async () => {
  await setSyncEnabled(true);
});

When("я вставил {int} строк в начало документа", async (n) => {
  await insertLinesAtTop(n);
});

When(
  "я вставил {int} строк в начало и немедленно прокрутил редактор на {int}% вниз",
  async (n, pct) => {
    state.editPct = pct;
    state.previewRatioAfterEdit = (
      await insertLinesAtTopAndScrollImmediately(n, pct)
    ).previewRatio;
  }
);

When("я быстро прокрутил редактор серией до абзаца {string}", async (text) => {
  const res = await rapidScrollEditorToSubstring(text);
  state.rapidTarget = res.target;
});

When("я прокрутил внутреннюю область таблицы", async () => {
  state.previewBeforeTable = (await scrollState("preview")).scrollTop;
  await scrollInnerTable();
});

When("я скрыл предпросмотр", async () => {
  await setPreviewVisible(false);
});

When("я навёл курсор на далёкий блок {string}", async (text) => {
  state.editorBeforeHover = (await scrollState("editor")).scrollTop;
  state.previewTopBeforeHover = await previewTopBlockText();
  await hoverBlockFar(text);
});

// ---------- проверки ----------

Then("верхний видимый блок предпросмотра — {string}", async (expected) => {
  const actual = await previewTopBlockText();
  if (actual !== expected) {
    throw new Error(`Сверху блока ${JSON.stringify(actual)}, ожидался ${JSON.stringify(expected)}`);
  }
});

Then("предпросмотр остался у верхнего блока {string}", async (expected) => {
  const actual = await previewTopBlockText();
  if (actual !== expected) {
    throw new Error(
      `Предпросмотр съехал: сверху ${JSON.stringify(actual)}, ожидался ${JSON.stringify(expected)}`
    );
  }
});

Then(
  "верхняя логическая строка редактора соответствует началу блока {string}",
  async (text) => {
    const expected = await lineOfSubstring(text);
    if (expected < 0) throw new Error(`В редакторе нет ${JSON.stringify(text)}`);
    const actual = await editorTopLine();
    if (actual !== expected) {
      throw new Error(
        `Верхняя строка редактора ${actual}, ожидалась ${expected} (начало блока ${JSON.stringify(text)})`
      );
    }
  }
);

Then(
  "верхняя логическая строка редактора соответствует верхнему блоку предпросмотра",
  async () => {
    const top = await previewTopBlockText();
    if (top === null) throw new Error("В предпросмотре нет data-md-блоков");
    const expected = await lineOfSubstring(top);
    if (expected < 0) {
      throw new Error(`Текст верхнего блока ${JSON.stringify(top)} не найден дословно в исходнике`);
    }
    const actual = await editorTopLine();
    if (actual !== expected) {
      throw new Error(
        `Верхняя строка редактора ${actual}, ожидалась ${expected} для блока ${JSON.stringify(top)}`
      );
    }
  }
);

Then("предпросмотр прокручен примерно на {int}%", async (pct) => {
  const st = await scrollState("preview");
  if (st.max <= 0) throw new Error("Предпросмотр не прокручивается (max = 0)");
  const ratio = st.scrollTop / st.max;
  const want = pct / 100;
  if (Math.abs(ratio - want) > 0.03) {
    throw new Error(`Доля предпросмотра ${ratio.toFixed(3)}, ожидалась ~${want}`);
  }
});

Then("предпросмотр остался на прежней позиции", async () => {
  const now = (await scrollState("preview")).scrollTop;
  if (Math.abs(now - state.previewBefore) > 1) {
    throw new Error(`Предпросмотр сдвинулся с ${state.previewBefore} на ${now} при выключенном sync`);
  }
});

Then("предпросмотр прокручен по пропорции до рендера", async () => {
  const want = state.editPct / 100;
  const got = state.previewRatioAfterEdit;
  if (Math.abs(got - want) > 0.03) {
    throw new Error(
      `Доля предпросмотра ${got.toFixed(3)}, ожидалась ~${want.toFixed(3)} (пропорция до рендера)`
    );
  }
});

Then("после рендера предпросмотр выровнен по анкору верхней строки", async () => {
  await browser.pause(300); // debounce 120 мс + IPC-рендер + кадр sync
  const line = await editorTopLine();
  const byte = await byteAtEditorLine(line);
  const expected = await expectedBlockTextAtByte(byte);
  const actual = await previewTopBlockText();
  if (actual !== expected) {
    throw new Error(
      `После рендера анкор не восстановился: сверху ${JSON.stringify(actual)}, ` +
        `ожидался блок верхней строки ${JSON.stringify(expected)}`
    );
  }
});

Then("редактор остался у последней позиции", async () => {
  const now = (await scrollState("editor")).scrollTop;
  if (Math.abs(now - state.rapidTarget) > 3) {
    throw new Error(`Редактор уехал с ${state.rapidTarget} на ${now} после серии прокруток`);
  }
});

Then("прокрутка предпросмотра стабильна", async () => {
  const a = (await scrollState("preview")).scrollTop;
  await new Promise((r) => setTimeout(r, 180));
  const b = (await scrollState("preview")).scrollTop;
  if (Math.abs(a - b) > 1) {
    throw new Error(`Предпросмотр дрожит: ${a} → ${b}`);
  }
});

Then("панель предпросмотра не сдвинулась", async () => {
  const now = (await scrollState("preview")).scrollTop;
  if (Math.abs(now - state.previewBeforeTable) > 1) {
    throw new Error(`Панель предпросмотра сдвинулась с ${state.previewBeforeTable} на ${now}`);
  }
});

Then("синхронизация не выбросила ошибку", async () => {
  const errors = await capturedErrors();
  if (errors.length) throw new Error(`Ошибки в runtime: ${errors.join(" | ")}`);
});

Then("у редактора отключён мягкий перенос", async () => {
  const info = await browser.execute(() => {
    const el = document.getElementById("editor");
    const cs = getComputedStyle(el);
    return {
      wrap: el.getAttribute("wrap"),
      whiteSpace: cs.whiteSpace,
      scrollWidth: el.scrollWidth,
      clientWidth: el.clientWidth,
    };
  });
  if (info.wrap !== "off") throw new Error(`wrap=${JSON.stringify(info.wrap)}, ожидался "off"`);
  if (info.whiteSpace !== "pre") {
    throw new Error(`white-space=${JSON.stringify(info.whiteSpace)}, ожидался "pre"`);
  }
  if (info.scrollWidth <= info.clientWidth) {
    throw new Error(
      `Длинная строка не даёт горизонтальной прокрутки (scrollWidth ${info.scrollWidth} ≤ clientWidth ${info.clientWidth})`
    );
  }
});

Then("редактор не перетащен синхронизацией от hover", async () => {
  const after = (await scrollState("editor")).scrollTop;
  const moved = Math.abs(after - state.editorBeforeHover);
  // Допускаем малое смещение от выделения, но не скачок к верхнему блоку
  // предпросмотра (это был бы эффект обратной связи sync).
  if (moved > 3) {
    const previewTop = await previewTopBlockText();
    throw new Error(
      `Hover увёл редактор на ${moved}px (${state.editorBeforeHover} → ${after}); ` +
        `верхний блок предпросмотра теперь ${JSON.stringify(previewTop)}`
    );
  }
  const previewNow = await previewTopBlockText();
  if (previewNow === state.previewTopBeforeHover) {
    throw new Error("Hover не вызвал программный scrollIntoView (верхний блок не изменился)");
  }
});
