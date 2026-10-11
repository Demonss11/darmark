// Инструмент диагностики скорости E2E: пишет по одной строке JSONL на сценарий
// (имя, функционал, теги, длительность, статус, краткая ошибка).
//
// Активируется только при заданном DARMARK_E2E_TIMING (путь к файлу):
// обычный прогон `npm run test:e2e` не пишет ничего.
//
// Длительность — wall-clock между нашим Before и After (полное время сценария,
// включая шаги, ожидания и очистку).
import { After, Before } from "@wdio/cucumber-framework";
import fs from "node:fs";

const out = process.env.DARMARK_E2E_TIMING;

// Сценарии выполняются последовательно (maxInstances: 1).
let startedAt = null;

Before(function () {
  if (out) startedAt = Date.now();
});

After(function (arg) {
  if (!out) return;
  try {
    const pickle = arg?.pickle;
    const doc = arg?.gherkinDocument;
    const res = arg?.result ?? {};
    const ms = startedAt == null ? null : Date.now() - startedAt;
    startedAt = null;
    const rec = {
      feature: doc?.feature?.name ?? "",
      uri: pickle?.uri ?? doc?.uri ?? "",
      name: pickle?.name ?? "",
      tags: (pickle?.tags ?? []).map((t) => t.name),
      durationMs: ms,
      status: res?.status ?? (res?.passed ? "PASSED" : "FAILED"),
      error: res?.message ? String(res.message).split("\n")[0].slice(0, 300) : null,
    };
    fs.appendFileSync(out, JSON.stringify(rec) + "\n");
  } catch {
    // диагностика не должна валить прогон
  }
});
