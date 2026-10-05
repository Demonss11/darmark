// Шаги интерактивных таблиц: сортировка, фильтр, мини-поиск (P1.4).
import { Given, When, Then } from "@wdio/cucumber-framework";
import {
  clickFilterButton,
  clickHeader,
  openFilter,
  setFilterMiniSearch,
  tableCounter,
  visibleFilterItems,
  visibleRows,
  waitTableEnhanced,
} from "./helpers.js";

Given("таблица в предпросмотре украшена", async () => {
  await waitTableEnhanced();
});

When("я кликаю по заголовку {string}", async (name) => {
  await clickHeader(name);
});

Then("видимые строки таблицы по порядку:", async (table) => {
  const expected = table.raw().map((row) => row[0]);
  const actual = (await visibleRows()).map((cells) => cells[0]);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `Порядок строк не совпал.\nОжидалось: ${expected.join(", ")}\nПолучено:  ${actual.join(", ")}`
    );
  }
});

Then("счётчик таблицы показывает {string}", async (expected) => {
  const actual = await tableCounter();
  if (actual !== expected) throw new Error(`Счётчик «${actual}» ≠ «${expected}»`);
});

When("я открываю фильтр колонки {string}", async (name) => {
  await openFilter(name);
});

When("в мини-поиске фильтра я ввожу {string}", async (query) => {
  await setFilterMiniSearch(query);
});

When("я очищаю мини-поиск фильтра", async () => {
  await setFilterMiniSearch("");
});

Then("в списке фильтра виден только пункт {string}", async (value) => {
  const items = await visibleFilterItems();
  if (items.length !== 1 || items[0] !== value) {
    throw new Error(`Ожидался единственный пункт «${value}», получено: [${items.join(", ")}]`);
  }
});

When("в фильтре колонки я нажимаю {string}", async (label) => {
  await clickFilterButton(label);
});
