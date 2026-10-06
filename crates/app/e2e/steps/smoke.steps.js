// Шаги дымовых проверок оболочки.
import { Given, Then } from "@wdio/cucumber-framework";
import { $, browser } from "@wdio/globals";
import { waitPreviewContains } from "./helpers.js";

Given("приложение mdedit запущено", async () => {
  await $("#editor").waitForExist({ timeout: 30_000 });
  await $("#preview").waitForExist({ timeout: 30_000 });
});

Then("поле редактора содержит непустой документ", async () => {
  // Стартовый документ создаётся через IPC (bootstrap → newDocument), поэтому
  // дожидаемся непустого значения, а не только существования #editor.
  await browser.waitUntil(
    async () => ((await $("#editor").getValue()) ?? "").length > 0,
    { timeout: 8_000, timeoutMsg: "Поле редактора пусто" }
  );
});

Then("предпросмотр содержит заголовок {string}", async (title) => {
  await waitPreviewContains(title);
});
