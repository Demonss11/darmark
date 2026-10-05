// Шаги проверки предпросмотра: рендер реальным md-core и безопасность HTML.
import { Then } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import { previewHtml, waitPreviewContains } from "./helpers.js";

Then("HTML предпросмотра содержит {string}", async (fragment) => {
  await waitPreviewContains(fragment);
});

Then("HTML предпросмотра не содержит {string}", async (fragment) => {
  const html = await previewHtml();
  if (html.toLowerCase().includes(fragment.toLowerCase())) {
    throw new Error(`Предпросмотр содержит запрещённое «${fragment}»: ${html}`);
  }
});

Then("javascript-ловушка {string} не сработала", async (name) => {
  const value = await browser.execute((varName) => window[varName], name);
  // WebDriver сериализует undefined как null — оба означают «ловушка не сработала».
  if (value !== undefined && value !== null) {
    throw new Error(`Ловушка ${name} сработала (значение: ${value})`);
  }
});
