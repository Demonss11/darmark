// Шаги режима инспектора: маппинг preview↔editor и корректность Unicode-границ
// (AC-10). Наведение эмулируется событием mouseover на [data-md]-блоке.
import { Given, Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  activeElementId,
  activeInspectText,
  clickBlockByText,
  editorSelection,
  hoverBlockByRaw,
  hoverBlockByText,
  inspectorBlocks,
  refBytesToUnits,
  selectEditorRange,
  setInspectorActive,
  setMarkdown,
} from "./helpers.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixturesDir = path.resolve(here, "..", "fixtures");

Given("режим инспектора включён", async () => {
  await setInspectorActive(true);
});

Given("в редакторе документ из фикстуры {string} с CRLF", async (name) => {
  const lf = readFileSync(path.join(fixturesDir, name), "utf8");
  const crlf = lf.replace(/\r?\n/g, "\r\n");
  await setMarkdown(crlf);
});

Then(
  "блок предпросмотра {string} выделяет в редакторе {string}",
  async (blockText, source) => {
    await hoverBlockByText(blockText);
    const sel = await editorSelection();
    if (sel.text !== source) {
      throw new Error(
        `Выделено ${JSON.stringify(sel.text)}, ожидалось ${JSON.stringify(source)}`
      );
    }
  }
);

Then(
  "для всех блоков инспектора выделение совпадает с UTF-16-границами",
  async () => {
    const blocks = await inspectorBlocks();
    if (blocks.length === 0) throw new Error("В предпросмотре нет data-md-блоков");

    for (const { raw } of blocks) {
      const [bStart, bEnd] = raw.split(",").map(Number);
      const expected = await refBytesToUnits(bStart, bEnd);

      await hoverBlockByRaw(raw);
      // Выделение выставляется синхронно обработчиком mouseover, но под
      // нагрузкой может «доехать» с задержкой — ждём, а не читаем один раз.
      let sel = await editorSelection();
      try {
        await browser.waitUntil(
          async () => {
            sel = await editorSelection();
            return sel.start === expected.start && sel.end === expected.end;
          },
          {
            timeout: 3000,
            interval: 50,
            timeoutMsg: `Блок data-md="${raw}": выделено ${sel.start}..${sel.end}, ожидалось ${expected.start}..${expected.end}`,
          }
        );
      } catch (e) {
        const value = await browser.execute(() => document.getElementById("editor").value);
        throw new Error(
          `${e.message}\n  выделенный текст: ${JSON.stringify(sel.text)}\n` +
            `  длина value: ${value.length} UTF-16, ${Buffer.byteLength(value, "utf8")} байт`
        );
      }
      if (sel.start === sel.end) {
        throw new Error(`Блок data-md="${raw}" выделился пустым диапазоном`);
      }

      // Ни один край не должен попадать в low-surrogate (разрыв эмодзи).
      const splitsSurrogate = await browser.execute(
        (s, e) => {
          const value = document.getElementById("editor").value;
          const isLow = (i) => {
            const c = value.charCodeAt(i);
            return c >= 0xdc00 && c <= 0xdfff;
          };
          return isLow(s) || isLow(e);
        },
        sel.start,
        sel.end
      );
      if (splitsSurrogate) {
        throw new Error(
          `Границы ${sel.start}..${sel.end} у блока data-md="${raw}" разрывают суррогатную пару`
        );
      }
    }
  }
);

When("в редакторе выделен диапазон {int}..{int}", async (start, end) => {
  await selectEditorRange(start, end);
});

Then("подсвечен блок предпросмотра {string}", async (text) => {
  const active = await activeInspectText();
  if (active !== text) {
    throw new Error(
      `Подсвечен ${JSON.stringify(active)}, ожидался ${JSON.stringify(text)}`
    );
  }
});

When("я кликнул по блоку предпросмотра {string}", async (text) => {
  await clickBlockByText(text);
});

Then("выделение редактора непустое", async () => {
  const sel = await editorSelection();
  if (sel.start === sel.end) {
    throw new Error("Выделение редактора пустое");
  }
});

Then("фокус находится в редакторе", async () => {
  const id = await activeElementId();
  if (id !== "editor") {
    throw new Error(`Фокус на ${JSON.stringify(id)}, ожидался "editor"`);
  }
});
