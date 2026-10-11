// Шаги режима «AI-ассистент» нижнего дока (TZ-AI-ASSISTANT §8; ADR-0030/0032/0034).
//
// Проверяем только наблюдаемый DOM: заглушка/чат/скиллы/карточки ошибок, классы
// индикатора #ai-dot и персист настроек в реальный config.json. Сетевой путь к
// живому провайдеру в E2E не воспроизводим — недоступный base_url задаём через
// IPC-шов (ai_get_settings/ai_set_settings), т.к. UI поле base_url только для чтения.
//
// Изоляция профиля — DARMARK_CONFIG_PATH (см. wdio.conf.js preparePlugins): тот же
// путь, что читает/пишет Rust (plugins::settings::SettingsStore).
import { Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";
import fs from "node:fs";

// ---------- IPC-шов и конфиг ----------

/// Вызов Tauri-команды из webview тем же внутренним путём, что и @tauri-apps/api
/// (window.__TAURI_INTERNALS__.invoke всегда инжектится Tauri, в отличие от __TAURI__).
async function invoke(cmd, args) {
  return browser.execute(
    async (c, a) => {
      const internals = window.__TAURI_INTERNALS__;
      if (!internals || typeof internals.invoke !== "function") {
        throw new Error("window.__TAURI_INTERNALS__.invoke недоступен");
      }
      return await internals.invoke(c, a);
    },
    cmd,
    args ?? {}
  );
}

/// Путь изолированного конфига E2E (выставлен в onPrepare wdio.conf.js).
function aiConfigPath() {
  const p = process.env.DARMARK_CONFIG_PATH;
  if (!p) {
    throw new Error(
      "DARMARK_CONFIG_PATH не задан — изоляция конфига не настроена (см. wdio.conf.js preparePlugins)"
    );
  }
  return p;
}

/// Секция `ai` из config.json (файла нет — пустой объект).
function readAiConfig() {
  const p = aiConfigPath();
  if (!fs.existsSync(p)) return {};
  const parsed = JSON.parse(fs.readFileSync(p, "utf8"));
  return parsed.ai ?? {};
}

// ---------- Готовность режима ----------

/// Ждёт загрузки настроек AI (createAiPanel читает их асинхронно при старте):
/// до ответа `ai_get_settings` поле base URL пусто, после — заполнено дефолтом.
async function waitAiReady() {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const el = document.getElementById("ai-base-url");
        return !!el && (el.value ?? "").length > 0;
      }),
    { timeout: 5000, timeoutMsg: "Настройки AI не загрузились (base URL пуст)" }
  );
}

/// Открывает док в режиме AI (идемпотентно) и дожидается загрузки настроек.
async function openAiDock() {
  await browser.execute(() => {
    const panel = document.getElementById("logs-panel");
    const expandedAi =
      !!panel &&
      !panel.hidden &&
      !panel.classList.contains("collapsed") &&
      panel.dataset.mode === "ai";
    if (!expandedAi) document.getElementById("sb-ai")?.click();
  });
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const panel = document.getElementById("logs-panel");
        const body = document.getElementById("ai-mode");
        return (
          !!panel &&
          !panel.hidden &&
          !panel.classList.contains("collapsed") &&
          panel.dataset.mode === "ai" &&
          !!body &&
          !body.hidden
        );
      }),
    { timeout: 5000, timeoutMsg: "Режим AI не открылся" }
  );
  await waitAiReady();
}

When("я открываю AI-ассистент", async () => {
  await openAiDock();
});

/// Открывает поповер настроек AI (клик по #ai-settings только при закрытом).
async function ensureSettingsOpen() {
  await browser.execute(() => {
    const pop = document.getElementById("ai-settings-pop");
    if (pop?.hidden) document.getElementById("ai-settings")?.click();
  });
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.getElementById("ai-settings-pop")?.hidden === false
      ),
    { timeout: 3000, timeoutMsg: "Поповер настроек AI не открылся" }
  );
}

When("я закрываю настройки AI", async () => {
  await browser.execute(() => document.getElementById("ai-settings-close")?.click());
});

Then("настройки AI закрыты", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.getElementById("ai-settings-pop")?.hidden === true
      ),
    { timeout: 3000, timeoutMsg: "Поповер настроек AI не закрылся" }
  );
});

// ---------- Состояние дока/режима ----------

Then("тело AI-режима видно", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => document.getElementById("ai-mode")?.hidden === false),
    { timeout: 5000, timeoutMsg: "Тело режима AI не показано" }
  );
});

Then("тело журнала видно", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => document.getElementById("logs-body")?.hidden === false),
    { timeout: 5000, timeoutMsg: "Тело режима «Журнал» не показано" }
  );
});

/// Порядок вкладок дока по `data-mode` в контейнере #logs-modes (AC-1).
Then(
  "порядок вкладок дока {string}, {string}, {string}",
  async (first, second, third) => {
    const order = await browser.execute(() =>
      Array.from(document.querySelectorAll("#logs-modes .mode-btn")).map(
        (el) => el.dataset.mode
      )
    );
    const expected = [first, second, third];
    if (order.length !== expected.length || order.some((m, i) => m !== expected[i])) {
      throw new Error(
        `Порядок вкладок дока ${JSON.stringify(order)}, ожидалось ${JSON.stringify(expected)}`
      );
    }
  }
);

// ---------- Заглушка и ввод (opt-in) ----------

Then("заглушка AI видна", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => document.getElementById("ai-off")?.hidden === false),
    { timeout: 5000, timeoutMsg: "Заглушка AI (#ai-off) не показана" }
  );
});

Then("заглушка AI скрыта", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => document.getElementById("ai-off")?.hidden === true),
    { timeout: 5000, timeoutMsg: "Заглушка AI (#ai-off) не скрыта" }
  );
});

Then("поле ввода AI недоступно", async () => {
  const disabled = await browser.execute(
    () => document.getElementById("ai-input")?.disabled === true
  );
  if (!disabled) throw new Error("Поле ввода AI доступно при выключенном AI");
});

Then("поле ввода AI доступно", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => document.getElementById("ai-input")?.disabled === false),
    { timeout: 5000, timeoutMsg: "Поле ввода AI осталось недоступным" }
  );
});

Then("кнопка отправки AI недоступна", async () => {
  const disabled = await browser.execute(
    () => document.getElementById("ai-send")?.disabled === true
  );
  if (!disabled) throw new Error("Кнопка отправки AI доступна при выключенном AI");
});

Then("тело чата AI скрыто", async () => {
  const hidden = await browser.execute(
    () => document.getElementById("ai-chat")?.hidden === true
  );
  if (!hidden) throw new Error("Тело чата AI показано при выключенном AI");
});

Then("ряд скиллов AI скрыт", async () => {
  const hidden = await browser.execute(
    () => document.getElementById("ai-skills")?.hidden === true
  );
  if (!hidden) throw new Error("Ряд скиллов AI показан при выключенном AI");
});

// ---------- Тумблер включения/персист ----------

Then("тумблер AI включён", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => document.getElementById("ai-enabled")?.checked === true),
    { timeout: 5000, timeoutMsg: "Тумблер AI не включился" }
  );
});

/// Закрывает поповер настроек AI, если открыт. Важно для изоляции: открытый поповер
/// перехватывает Escape в capture-фазе (aiPanel.onDocKeyDown) и «переехал» бы в
/// следующие сценарии/фичи, ломая их обработку Escape (см. logs.feature).
async function closeAiSettings() {
  await browser.execute(() => {
    const pop = document.getElementById("ai-settings-pop");
    if (pop && !pop.hidden) document.getElementById("ai-settings-close")?.click();
  });
}

/// Клик по тумблеру #ai-enabled в поповере настроек (только при расхождении).
async function setAiToggle(want) {
  await ensureSettingsOpen();
  const ok = await browser.execute((w) => {
    const cb = document.getElementById("ai-enabled");
    if (!cb) return false;
    if (cb.checked !== w) cb.click();
    return true;
  }, want);
  if (!ok) throw new Error("В настройках AI нет тумблера #ai-enabled");
  await browser.waitUntil(
    async () =>
      browser.execute((w) => document.getElementById("ai-enabled")?.checked === w, want),
    { timeout: 5000, timeoutMsg: `Тумблер AI не переключился в ${want}` }
  );
  await closeAiSettings();
}

When("я выключаю AI тумблером", async () => {
  await setAiToggle(false);
});

When("я включаю AI тумблером", async () => {
  await setAiToggle(true);
});

/// Персист секции `ai` сверяем с реальным config.json (DARMARK_CONFIG_PATH).
Then(
  "в конфиге настройка AI {string} равна {string}",
  async (key, expected) => {
    await browser.waitUntil(
      async () => {
        const ai = readAiConfig();
        return String(ai[key]) === expected;
      },
      { timeout: 5000, timeoutMsg: `В конфиге ai.${key} не стало ${expected}` }
    );
  }
);

// ---------- Скиллы ----------

Then("в ряду скиллов AI есть боевой скилл {string}", async (title) => {
  await browser.waitUntil(
    async () =>
      browser.execute((t) => {
        const row = document.getElementById("ai-skills");
        return Array.from(row?.querySelectorAll("button.skill:not(.skill-soon)") ?? []).some(
          (b) => !b.disabled && (b.textContent ?? "").trim().includes(t)
        );
      }, title),
    { timeout: 5000, timeoutMsg: `В ряду скиллов нет боевого скилла «${title}»` }
  );
});

Then("в ряду скиллов AI ровно {int} боевых скилла", async (count) => {
  await browser.waitUntil(
    async () =>
      browser.execute((n) => {
        const row = document.getElementById("ai-skills");
        return (
          (row?.querySelectorAll("button.skill:not(.skill-soon):not([disabled])").length ??
            0) === n
        );
      }, count),
    { timeout: 5000, timeoutMsg: `В ряду скиллов не ${count} боевых скиллов` }
  );
});

Then("в ряду скиллов AI есть чип {string}", async (label) => {
  await browser.waitUntil(
    async () =>
      browser.execute((l) => {
        const chip = document.querySelector("#ai-skills .skill-soon");
        return !!chip && chip.disabled && (chip.textContent ?? "").includes(l);
      }, label),
    { timeout: 5000, timeoutMsg: `В ряду скиллов нет чипа «${label}»` }
  );
});

// ---------- Чат: ввод, отправка, карточки ----------

When("я ввожу в поле AI {string}", async (text) => {
  await browser.execute((t) => {
    const input = document.getElementById("ai-input");
    if (!input) throw new Error("нет #ai-input");
    input.value = t;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, text);
});

When("я отправляю сообщение AI", async () => {
  const ok = await browser.execute(() => {
    const btn = document.getElementById("ai-send");
    if (!btn || btn.disabled) return false;
    btn.click();
    return true;
  });
  if (!ok) throw new Error("Кнопка отправки AI (#ai-send) недоступна");
});

When("я очищаю чат AI", async () => {
  await browser.execute(() => document.getElementById("ai-clear")?.click());
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.querySelectorAll("#ai-chat .msg").length === 0
      ),
    { timeout: 3000, timeoutMsg: "Чат AI не очистился" }
  );
});

Then("в чате AI есть сообщение пользователя", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => !!document.querySelector("#ai-chat .msg.user")),
    { timeout: 5000, timeoutMsg: "В чате AI нет сообщения пользователя" }
  );
});

Then("чат AI пуст", async () => {
  const count = await browser.execute(
    () => document.querySelectorAll("#ai-chat .msg").length
  );
  if (count !== 0) throw new Error(`В чате AI ${count} сообщений, ожидалось пусто`);
});

Then("в чате AI есть карточка ошибки", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => !!document.querySelector("#ai-chat .error-card")),
    { timeout: 5000, timeoutMsg: "В чате AI нет карточки ошибки" }
  );
});

Then("карточка ошибки AI содержит {string}", async (fragment) => {
  await browser.waitUntil(
    async () =>
      browser.execute((f) => {
        const card = document.querySelector("#ai-chat .error-card");
        return !!card && (card.textContent ?? "").toLowerCase().includes(f.toLowerCase());
      }, fragment),
    { timeout: 5000, timeoutMsg: `Карточка ошибки AI не содержит «${fragment}»` }
  );
});

// ---------- Диалог согласия ----------

Then("открыт диалог согласия AI", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const consent = document.querySelector("#modal-root .dialog-backdrop .ai-consent");
        return !!consent;
      }),
    { timeout: 5000, timeoutMsg: "Диалог согласия AI не открылся" }
  );
});

/// Клик по кнопке в диалоге согласия по её видимому тексту.
async function answerConsent(label) {
  const ok = await browser.execute((l) => {
    const btn = Array.from(
      document.querySelectorAll("#modal-root .dialog-backdrop .dialog-actions button")
    ).find((b) => (b.textContent ?? "").trim() === l);
    if (!btn) return false;
    btn.click();
    return true;
  }, label);
  if (!ok) throw new Error(`В диалоге согласия AI нет кнопки «${label}»`);
}

When("я не разрешаю отправку в диалоге согласия AI", async () => {
  await answerConsent("Не разрешать");
});

When("я разрешаю отправку в диалоге согласия AI", async () => {
  await answerConsent("Разрешить");
});

Then("диалог согласия AI закрыт", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => !document.querySelector("#modal-root .dialog-backdrop .ai-consent")
      ),
    { timeout: 5000, timeoutMsg: "Диалог согласия AI остался открытым" }
  );
});

// ---------- Индикатор #ai-dot (AC-8) ----------

Then("у индикатора AI состояние {string}", async (state) => {
  const cls = `state-${state}`;
  await browser.waitUntil(
    async () =>
      browser.execute((c) => {
        const dot = document.getElementById("ai-dot");
        const mirror = document.getElementById("ai-status-dot");
        return !!dot && dot.classList.contains(c) && (!mirror || mirror.classList.contains(c));
      }, cls),
    { timeout: 5000, timeoutMsg: `Индикатор AI не в состоянии ${cls}` }
  );
});

// ---------- Настройки через IPC-шов (base_url недоступен в UI) ----------

When("я задаю API-ключ AI через настройки {string}", async (key) => {
  await ensureSettingsOpen();
  await browser.execute((k) => {
    const input = document.getElementById("ai-key");
    if (!input) throw new Error("нет #ai-key");
    input.value = k;
  }, key);
  await browser.execute(() => document.getElementById("ai-key-save")?.click());
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.getElementById("ai-key-hint")?.textContent === "Ключ сохранён"
      ),
    { timeout: 5000, timeoutMsg: "API-ключ AI не сохранился (подсказка не «Ключ сохранён»)" }
  );
  await closeAiSettings();
});

When("я направляю AI на недоступный адрес {string}", async (url) => {
  const view = await invoke("ai_get_settings");
  const settings = {
    enabled: view.enabled,
    provider: view.provider,
    base_url: url,
    model: view.model,
    max_tokens: view.max_tokens,
    temperature: view.temperature,
    consent_document_send: view.consent_document_send,
  };
  await invoke("ai_set_settings", { settings });
  const after = await invoke("ai_get_settings");
  if (after.base_url !== url) {
    throw new Error(`base_url AI не стал ${url} (получено ${after.base_url})`);
  }
});

// ---------- Хоткей, поповер и удаление ключа (правки Фазы 1) ----------
//
// Хоткей Ctrl+Shift+A проверяется существующим шагом «я нажимаю хоткей …»
// (plugins.steps.js): он шлёт синтетический keydown в window — тот же путь, что у
// shell.ts. Здесь добавляем только открытие поповера и удаление ключа.

When("я открываю настройки AI", async () => {
  await ensureSettingsOpen();
});

Then("настройки AI открыты", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.getElementById("ai-settings-pop")?.hidden === false
      ),
    { timeout: 3000, timeoutMsg: "Поповер настроек AI не открылся" }
  );
});

Then("кнопка удаления ключа AI доступна", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.getElementById("ai-key-clear")?.disabled === false
      ),
    { timeout: 3000, timeoutMsg: "Кнопка удаления ключа AI недоступна" }
  );
});

Then("кнопка удаления ключа AI недоступна", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.getElementById("ai-key-clear")?.disabled === true
      ),
    { timeout: 3000, timeoutMsg: "Кнопка удаления ключа AI осталась доступной" }
  );
});

/// Удаление ключа — клик по #ai-key-clear (пустое поле → ai_set_api_key("")).
When("я удаляю ключ AI", async () => {
  const ok = await browser.execute(() => {
    const btn = document.getElementById("ai-key-clear");
    if (!btn || btn.disabled) return false;
    btn.click();
    return true;
  });
  if (!ok) throw new Error("Кнопка удаления ключа AI (#ai-key-clear) недоступна");
});

Then("подсказка ключа AI {string}", async (text) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (t) => document.getElementById("ai-key-hint")?.textContent === t,
        text
      ),
    { timeout: 5000, timeoutMsg: `Подсказка ключа AI не стала «${text}»` }
  );
});

/// Персист удаления: ключ больше не считается заданным (`has_key=false`).
/// Сверяем IPC-швом (в config.json секрет может лежать как `null` — хрупко).
Then("у AI нет сохранённого ключа", async () => {
  await browser.waitUntil(
    async () => {
      const view = await invoke("ai_get_settings");
      return view?.has_key === false;
    },
    { timeout: 5000, timeoutMsg: "У AI всё ещё есть сохранённый ключ (has_key ≠ false)" }
  );
});
