// Общий шаг: заполнение редактора документом (markdown или сырой HTML).
import { Given } from "@wdio/cucumber-framework";
import { setMarkdown } from "./helpers.js";

Given("в редактор введён документ:", async (text) => {
  await setMarkdown(text);
});
