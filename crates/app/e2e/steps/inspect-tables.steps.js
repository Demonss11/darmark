// Шаги гранулярности инспектора для таблиц (TZ-inspect-tables).
// Наведение эмулируется синтетическим mouseover на <td>/<th>/<tr>/<.md-block>;
// Shift передаётся свойством shiftKey события.
import { Then, When } from "@wdio/cucumber-framework";
import {
  activeInspectInfo,
  clickCellByText,
  columnBandCount,
  editorSelection,
  hoverCellByText,
  hoverHeaderByText,
  hoverRowByText,
  hoverTableBlock,
  leavePreview,
  pressShift,
  releaseShift,
  rowBandText,
} from "./helpers.js";

When("я навожу мышь на ячейку {string}", async (text) => {
  await hoverCellByText(text, false);
});

When("я навожу мышь с Shift на ячейку {string}", async (text) => {
  await hoverCellByText(text, true);
});

When("я навожу мышь на заголовок {string}", async (name) => {
  await hoverHeaderByText(name);
});

When("я навожу мышь на строку {string}", async (text) => {
  await hoverRowByText(text);
});

When("я навожу мышь на таблицу целиком", async () => {
  await hoverTableBlock();
});

When("я кликнул по ячейке {string}", async (text) => {
  await clickCellByText(text);
});

When("я нажимаю Shift", async () => {
  await pressShift();
});

When("я отпускаю Shift", async () => {
  await releaseShift();
});

When("курсор уходит из предпросмотра", async () => {
  await leavePreview();
});

Then("нет активного элемента инспектора", async () => {
  const info = await activeInspectInfo();
  if (info) {
    throw new Error(
      `Активный элемент остался: ${info.tag} ${JSON.stringify(info.text)}`
    );
  }
});

Then("в редакторе выделено {string}", async (expected) => {
  const sel = await editorSelection();
  if (sel.text !== expected) {
    throw new Error(
      `В редакторе выделено ${JSON.stringify(sel.text)}, ожидалось ${JSON.stringify(expected)}`
    );
  }
});

Then("в редакторе выделено, содержащее {string}", async (fragment) => {
  const sel = await editorSelection();
  if (!sel.text.includes(fragment)) {
    throw new Error(
      `Выделено ${JSON.stringify(sel.text)}, ожидалось вхождение ${JSON.stringify(fragment)}`
    );
  }
});

Then("активный элемент инспектора — тег {string}", async (tag) => {
  const info = await activeInspectInfo();
  if (!info) throw new Error("Нет активного элемента инспектора (.inspect-active)");
  if (info.tag !== tag) {
    throw new Error(`Активный тег ${info.tag}, ожидался ${tag}`);
  }
});

Then("подсвечена строка, содержащая {string}", async (fragment) => {
  const text = await rowBandText();
  if (text === null) throw new Error("Нет подсвеченной строки (.inspect-row)");
  if (!text.includes(fragment)) {
    throw new Error(
      `Подсвечена строка ${JSON.stringify(text)}, ожидалось вхождение ${JSON.stringify(fragment)}`
    );
  }
});

Then("нет подсвеченной строки", async () => {
  const text = await rowBandText();
  if (text !== null) {
    throw new Error(`Осталась подсвеченная строка ${JSON.stringify(text)}`);
  }
});

Then("подсвечено ячеек столбца: {int}", async (expected) => {
  const actual = await columnBandCount();
  if (actual !== expected) {
    throw new Error(`Подсвечено ячеек столбца ${actual}, ожидалось ${expected}`);
  }
});
