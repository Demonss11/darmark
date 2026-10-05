// Шаги дымовых проверок оболочки.
import { Given, Then } from "@wdio/cucumber-framework";
import { $ } from "@wdio/globals";
import { waitPreviewContains } from "./helpers.js";

Given("приложение mdedit запущено", async () => {
  await $("#editor").waitForExist({ timeout: 30_000 });
  await $("#preview").waitForExist({ timeout: 30_000 });
});

Then("поле редактора содержит непустой документ", async () => {
  const value = await $("#editor").getValue();
  if (!value || value.length === 0) throw new Error("Поле редактора пусто");
});

Then("предпросмотр содержит заголовок {string}", async (title) => {
  await waitPreviewContains(title);
});
