// Шаги плагинного тир-1 view (H2, Фаза 4) и менеджера плагинов (Фаза 5). Список
// представлений приходит из Rust-хоста событием `plugin-views-changed`; клик по
// `data-p-<plugin_id>-action` возвращается в Lua-хендлер (`on_action`) и обновляет
// HTML контейнера. Менеджер (`#plugin-manager`) читает `list_plugins` и меняет
// состояние через `set_plugin_enabled`/`reload_plugin`.
import { Given, Then, When } from "@wdio/cucumber-framework";
import { browser } from "@wdio/globals";

/// Ждёт появления вкладки плагина: значит, хост отдал `plugin_views` и контроллер
/// `pluginViews.ts` построил `.vtab.plugin[data-view=...]`. Аргумент — `plugin_id`;
/// `view_id` имеет вид `<plugin_id>:<kind>`, поэтому ищем вкладку по префиксу.
Given("плагин {string} загружен", async (pluginId) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (id) => !!document.querySelector(`#view-switch .vtab.plugin[data-view^="${id}:"]`),
        pluginId
      ),
    { timeout: 15000, timeoutMsg: `Плагин ${pluginId} не загрузился (нет вкладки)` }
  );
});

Then("в {string} есть вкладка плагина {string}", async (selector, viewId) => {
  // Переключение вкладок приходит асинхронно событием `plugin-views-changed`,
  // поэтому проверка — с ожиданием (в отличие от клика по вкладке).
  await browser.waitUntil(
    async () =>
      browser.execute(
        (sel, id) => !!document.querySelector(`${sel} .vtab.plugin[data-view="${id}"]`),
        selector,
        viewId
      ),
    { timeout: 10000, timeoutMsg: `В ${selector} нет вкладки плагина ${viewId}` }
  );
});

When("я переключаюсь на вкладку плагина {string}", async (viewId) => {
  const ok = await browser.execute((id) => {
    const tab = document.querySelector(`#view-switch .vtab.plugin[data-view="${id}"]`);
    if (!tab) return false;
    tab.click();
    return true;
  }, viewId);
  if (!ok) throw new Error(`Вкладка плагина ${viewId} не найдена`);
  await browser.pause(50);
});

Then("контейнер плагинного view виден", async () => {
  const visible = await browser.execute(() => {
    const el = document.getElementById("plugin-view");
    return !!el && !el.hidden;
  });
  if (!visible) throw new Error("#plugin-view не показан после клика по вкладке");
});

When("я кликаю по плагинному действию {string} {string}", async (pluginId, action) => {
  const ok = await browser.execute(
    (p, a) => {
      const el = document.querySelector(`#plugin-view [data-p-${p}-action="${a}"]`);
      if (!el) return false;
      el.click();
      return true;
    },
    pluginId,
    action
  );
  if (!ok) throw new Error(`Действие ${action} плагина ${pluginId} не найдено`);
});

Then("плагинный маркер равен {string}", async (expected) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (e) => document.getElementById("e2e-marker")?.textContent === e,
        expected
      ),
    { timeout: 10000, timeoutMsg: `Маркер #e2e-marker не стал ${expected}` }
  );
});

// ---------- Менеджер плагинов (H2, Фаза 5) ----------
//
// Панель плагинов живёт в правом модальном дровере (ADR-0024):
// триггер — кнопка `#tb-plugins` в тулбаре, открытый дровер помечен классом
// `.open` на `#drawer`. Тумблер вкл/выкл — настоящий `<input class="pl-enabled">`
// внутри `.pl-item[data-plugin=...]`; его click порождает change → IPC.

/// Показывает панель плагинов: клик по `#tb-plugins`, если дровер закрыт. Общий
/// помощник для шага «я открываю панель плагинов» и Given о загрузке плагина без
/// view (его статус виден только в менеджере).
async function openPluginsPanel() {
  const open = await browser.execute(() => {
    const drawer = document.getElementById("drawer");
    if (!drawer) return false;
    if (!drawer.classList.contains("open")) {
      document.getElementById("tb-plugins")?.click();
    }
    return drawer.classList.contains("open");
  });
  if (!open) {
    await browser.waitUntil(
      async () =>
        browser.execute(
          () => document.getElementById("drawer")?.classList.contains("open")
        ),
      { timeout: 5000, timeoutMsg: "Панель плагинов не открылась" }
    );
  }
}

When("я открываю панель плагинов", async () => {
  await openPluginsPanel();
});

Then("в менеджере плагинов есть {string} со статусом {string}", async (pluginId, state) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (id, st) => {
          const item = document.querySelector(
            `#plugin-manager .pl-item[data-plugin="${id}"]`
          );
          const badge = item?.querySelector(".pl-badge");
          return !!badge && badge.classList.contains(st);
        },
        pluginId,
        state
      ),
    {
      timeout: 15000,
      timeoutMsg: `В менеджере нет плагина ${pluginId} со статусом ${state}`,
    }
  );
});

/// Ставит тумблер плагина в нужное положение (клик только при расхождении).
async function setPluginToggle(pluginId, enabled) {
  const ok = await browser.execute(
    (id, want) => {
      const item = document.querySelector(
        `#plugin-manager .pl-item[data-plugin="${id}"]`
      );
      const cb = item?.querySelector("input.pl-enabled");
      if (!cb) return false;
      if (cb.checked !== want) cb.click();
      return true;
    },
    pluginId,
    enabled
  );
  if (!ok) throw new Error(`В менеджере нет тумблера плагина ${pluginId}`);
}

When("я выключаю плагин {string}", async (pluginId) => {
  await setPluginToggle(pluginId, false);
});

When("я включаю плагин {string}", async (pluginId) => {
  await setPluginToggle(pluginId, true);
});

Then("в {string} нет вкладки плагина {string}", async (selector, viewId) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (sel, id) => !document.querySelector(`${sel} .vtab.plugin[data-view="${id}"]`),
        selector,
        viewId
      ),
    { timeout: 10000, timeoutMsg: `В ${selector} осталась вкладка плагина ${viewId}` }
  );
});

When("я перезагружаю плагин {string}", async (pluginId) => {
  const ok = await browser.execute((id) => {
    const item = document.querySelector(
      `#plugin-manager .pl-item[data-plugin="${id}"]`
    );
    const btn = item?.querySelector("button.pl-reload");
    if (!btn || btn.disabled) return false;
    btn.click();
    return true;
  }, pluginId);
  if (!ok) throw new Error(`Кнопка «Перезагрузить» плагина ${pluginId} недоступна`);
});

// ---------- Дровер плагинов (ADR-0024) ----------
//
// Правый модальный дровер: триггер `#tb-plugins` в тулбаре, закрытие по Esc,
// клику по подложке `#backdrop` и кнопке `#drawer-close`. Фокус возвращается
// на триггер. Проверяем класс `.open`, не getComputedStyle (анимация).

When("я кликаю по кнопке {string}", async (selector) => {
  const ok = await browser.execute((s) => {
    const el = document.querySelector(s);
    if (!el) return false;
    el.click();
    return true;
  }, selector);
  if (!ok) throw new Error(`Элемент ${selector} не найден`);
});

Then("дровер плагинов открыт", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        () => document.getElementById("drawer")?.classList.contains("open")
      ),
    { timeout: 5000, timeoutMsg: "Дровер плагинов не открылся" }
  );
});

Then("дровер плагинов закрыт", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const drawer = document.getElementById("drawer");
        return !!drawer && !drawer.classList.contains("open") && drawer.hidden;
      }),
    { timeout: 5000, timeoutMsg: "Дровер плагинов не закрылся" }
  );
});

// Счётчик/индикатор триггера `#tb-plugins` (ADR-0024): число
// активных и флаг проблем сверяем с фактическим списком в менеджере.
Then("счётчик триггера плагинов соответствует списку", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const trigger = document.getElementById("tb-plugins");
        const cnt = document.getElementById("tb-plugins-cnt");
        if (!trigger || !cnt) return false;
        let active = 0;
        let problems = 0;
        document.querySelectorAll("#plugin-manager .pl-item .pl-badge").forEach((badge) => {
          if (badge.classList.contains("active")) active += 1;
          if (
            badge.classList.contains("failed") ||
            badge.classList.contains("quarantined")
          ) {
            problems += 1;
          }
        });
        return (
          cnt.textContent === String(active) &&
          trigger.classList.contains("has-problem") === (problems > 0)
        );
      }),
    { timeout: 5000, timeoutMsg: "Счётчик/индикатор триггера не совпал со списком плагинов" }
  );
});

// ---------- Плагинная правка документа (BUG-002, часть A) ----------
//
// Фикстура e2e-edit объявляет только команду (без тир-1 view), поэтому её
// загрузка не видна в #view-switch — статус `active` читаем из менеджера.

Given("плагин {string} активен", async (pluginId) => {
  await openPluginsPanel();
  await browser.waitUntil(
    async () =>
      browser.execute(
        (id) => {
          const item = document.querySelector(
            `#plugin-manager .pl-item[data-plugin="${id}"]`
          );
          const badge = item?.querySelector(".pl-badge");
          return !!badge && badge.classList.contains("active");
        },
        pluginId
      ),
    { timeout: 15000, timeoutMsg: `Плагин ${pluginId} не активен` }
  );
});

/// Клик по кнопке «Выполнить» команды плагина в менеджере
/// (`.pl-item[data-plugin=...] .pl-cmd[data-command=...]`). Через browser.execute:
/// панель может быть вне видимой области, нативный WebDriver click нестабилен.
When("я выполняю команду плагина {string} {string}", async (pluginId, commandId) => {
  const ok = await browser.execute(
    (pid, cid) => {
      const btn = document.querySelector(
        `#plugin-manager .pl-item[data-plugin="${pid}"] .pl-cmd[data-command="${cid}"]`
      );
      if (!btn || btn.disabled) return false;
      btn.click();
      return true;
    },
    pluginId,
    commandId
  );
  if (!ok) throw new Error(`Команда ${commandId} плагина ${pluginId} недоступна`);
});

/// Ждём асинхронного пути внешней правки: document-updated → pull
/// document_snapshot → обновление textarea (#editor). Текст приходит не сразу.
Then("текст редактора содержит {string}", async (fragment) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (f) => (document.getElementById("editor")?.value ?? "").includes(f),
        fragment
      ),
    { timeout: 10000, timeoutMsg: `В редакторе не появилось: ${fragment}` }
  );
});

// ---------- Согласие на разрешения (§11.1 п.4) ----------

/// Ставит чекбокс согласия `input.pl-grant[data-permission=...]` в нужное
/// состояние (клик только при расхождении). Смена прав у включённого плагина
/// перезапускает child, поэтому ждём, пока перерисованный чекбокс подтвердит
/// новое состояние и станет снова активным.
async function setPluginGrant(pluginId, permission, granted) {
  await openPluginsPanel();
  const ok = await browser.execute(
    (id, perm, want) => {
      const item = document.querySelector(
        `#plugin-manager .pl-item[data-plugin="${id}"]`
      );
      const cb = item?.querySelector(`input.pl-grant[data-permission="${perm}"]`);
      if (!cb) return false;
      if (cb.checked !== want) cb.click();
      return true;
    },
    pluginId,
    permission,
    granted
  );
  if (!ok) throw new Error(`В менеджере нет разрешения ${permission} у плагина ${pluginId}`);
  await browser.waitUntil(
    async () =>
      browser.execute(
        (id, perm, want) => {
          const item = document.querySelector(
            `#plugin-manager .pl-item[data-plugin="${id}"]`
          );
          const cb = item?.querySelector(`input.pl-grant[data-permission="${perm}"]`);
          return !!cb && !cb.disabled && cb.checked === want;
        },
        pluginId,
        permission,
        granted
      ),
    {
      timeout: 15000,
      timeoutMsg: `Разрешение ${permission} плагина ${pluginId} не переключилось`,
    }
  );
}

When("я выдаю разрешение {string} {string}", async (pluginId, permission) => {
  await setPluginGrant(pluginId, permission, true);
});

When("я снимаю разрешение {string} {string}", async (pluginId, permission) => {
  await setPluginGrant(pluginId, permission, false);
});

/// Негативная проверка гейта: дожидаемся конца IPC-пути команды (кнопки команд
/// снова активны после `refresh`), затем короткая страховка на асинхронную
/// обработку события плагином — и убеждаемся, что запрещённая правка НЕ дошла.
Then("текст редактора не содержит {string}", async (fragment) => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const buttons = Array.from(
          document.querySelectorAll("#plugin-manager .pl-cmd")
        );
        return buttons.length > 0 && buttons.every((b) => !b.disabled);
      }),
    { timeout: 5000, timeoutMsg: "Кнопки команд не вернулись в активное состояние" }
  );
  await browser.pause(300);
  const has = await browser.execute(
    (f) => (document.getElementById("editor")?.value ?? "").includes(f),
    fragment
  );
  if (has) throw new Error(`В редакторе неожиданно появилось: ${fragment}`);
});

// ---------- BUG-004: полосы плагинов в тулбаре быть не должно ----------
//
// Ранее `#plugin-strip` наполнялся кнопками-монограммами и разрастался с числом
// плагинов. Полосу удалили (плагинные view доступны вкладками `#view-switch`).
// Тест — защита от повторного появления.
Then("в тулбаре нет полосы плагинов", async () => {
  const info = await browser.execute(() => ({
    strip: document.getElementById("plugin-strip") !== null,
    pbtn: document.querySelectorAll("#toolbar .pbtn").length,
  }));
  if (info.strip || info.pbtn > 0) {
    throw new Error(`Полоса плагинов вернулась (BUG-004): ${JSON.stringify(info)}`);
  }
});

// ---------- Reload-хоткей Ctrl+R и per-plugin статусбар (§11.1 п.5) ----------

/// Синтетический keydown в window — тот же путь, что у реальной клавиатуры
/// (shell.ts слушает window). Спека задаёт сочетание как "Ctrl+R"/"Ctrl+Shift+R".
When("я нажимаю хоткей {string}", async (spec) => {
  await browser.execute((keys) => {
    const parts = keys.split("+");
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: parts[parts.length - 1].toLowerCase(),
        ctrlKey: parts.includes("Ctrl"),
        shiftKey: parts.includes("Shift"),
        bubbles: true,
        cancelable: true,
      })
    );
  }, spec);
  await browser.pause(50);
});

/// Сентинел страницы: если бы WebView перезагрузил страницу, переменная исчезла бы.
When("я ставлю сентинел страницы {string}", async (value) => {
  await browser.execute((v) => {
    window.__ctrlrSentinel = v;
  }, value);
});

Then("сентинел страницы равен {string}", async (value) => {
  const got = await browser.execute(() => window.__ctrlrSentinel);
  if (got !== value) {
    throw new Error(`Страница перезагрузилась (сентинел ${JSON.stringify(got)} ≠ ${value})`);
  }
});

/// Элемент плагина в статусбаре: включён и получил право ui:statusbar.
Then("в статусбаре есть плагин {string} с цветом", async (pluginId) => {
  await browser.waitUntil(
    async () =>
      browser.execute((id) => {
        const el = document.querySelector(`#plugin-status .ps-item[data-plugin="${id}"]`);
        if (!el) return false;
        const pc = getComputedStyle(el).getPropertyValue("--pc").trim();
        return pc.length > 0;
      }, pluginId),
    { timeout: 10000, timeoutMsg: `В статусбаре нет плагина ${pluginId} с цветом --pc` }
  );
});

Then("в статусбаре нет плагина {string}", async (pluginId) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (id) => !document.querySelector(`#plugin-status .ps-item[data-plugin="${id}"]`),
        pluginId
      ),
    { timeout: 10000, timeoutMsg: `В статусбаре остался плагин ${pluginId}` }
  );
});

/// Сообщение плагина отображается в его элементе (host.show_message → plugin-message).
Then("в статусбаре плагин {string} показывает {string}", async (pluginId, text) => {
  await browser.waitUntil(
    async () =>
      browser.execute(
        (id, t) =>
          document.querySelector(`#plugin-status .ps-item[data-plugin="${id}"] .ps-text`)
            ?.textContent === t,
        pluginId,
        text
      ),
    { timeout: 3500, timeoutMsg: `В статусбаре плагин ${pluginId} не показал ${text}` }
  );
});

// ---------- Кнопка информации о границах изоляции (реализовано в pluginManager.ts) ----------
//
// У карточки плагина есть кнопка `button.pl-info` (в `.pl-item-head`, после имени).
// Клик открывает поповер `.pl-info-pop` с формулировками `PluginInfo.notices`;
// Escape (и повторный клик / клик-вне) закрывают. В E2E проверяем клик и Escape;
// hover-открытие и позиционирование не трогаем — нестабильны в WebdriverIO.

/// Кнопка информации у плагина: `button.pl-info` внутри `.pl-item[data-plugin]`.
/// Связь кнопки с поповером — через `aria-controls` (наличие атрибута, §2).
Then("у плагина {string} есть кнопка информации", async (pluginId) => {
  await browser.waitUntil(
    async () =>
      browser.execute((id) => {
        const btn = document.querySelector(
          `#plugin-manager .pl-item[data-plugin="${id}"] button.pl-info`
        );
        return !!btn && !!btn.getAttribute("aria-controls");
      }, pluginId),
    { timeout: 10000, timeoutMsg: `У плагина ${pluginId} нет кнопки информации` }
  );
});

/// Клик через browser.execute: панель может быть вне видимой области, нативный
/// WebDriver click нестабилен (тот же приём, что у тумблера/команд).
When("я кликаю по кнопке информации плагина {string}", async (pluginId) => {
  const ok = await browser.execute((id) => {
    const btn = document.querySelector(
      `#plugin-manager .pl-item[data-plugin="${id}"] button.pl-info`
    );
    if (!btn) return false;
    btn.click();
    return true;
  }, pluginId);
  if (!ok) throw new Error(`Кнопка информации плагина ${pluginId} не найдена`);
});

/// Поповер открыт: элемент `.pl-info-pop` существует, не скрыт атрибутом
/// `hidden` и кнопка сообщает состояние `aria-expanded="true"` (§2, §5).
Then("поповер информации плагина {string} виден", async (pluginId) => {
  await browser.waitUntil(
    async () =>
      browser.execute((id) => {
        const item = document.querySelector(
          `#plugin-manager .pl-item[data-plugin="${id}"]`
        );
        const pop = item?.querySelector(".pl-info-pop");
        const btn = item?.querySelector("button.pl-info");
        return !!pop && !pop.hidden && btn?.getAttribute("aria-expanded") === "true";
      }, pluginId),
    { timeout: 5000, timeoutMsg: `Поповер информации плагина ${pluginId} не виден` }
  );
});

/// Открытый (не скрытый) поповер содержит искомый текст notice. Скоуп — любой
/// видимый `.pl-info-pop`: по §8 одновременно открыт не более одного поповера.
Then("поповер содержит текст {string}", async (text) => {
  await browser.waitUntil(
    async () =>
      browser.execute((t) => {
        const visible = Array.from(
          document.querySelectorAll("#plugin-manager .pl-info-pop")
        ).filter((pop) => !pop.hidden);
        return visible.some((pop) => (pop.textContent ?? "").includes(t));
      }, text),
    { timeout: 5000, timeoutMsg: `Открытый поповер не содержит текст: ${text}` }
  );
});

/// Поповер скрыт: `hidden` вернулся, кнопка сообщает `aria-expanded="false"`.
Then("поповер информации плагина {string} скрыт", async (pluginId) => {
  await browser.waitUntil(
    async () =>
      browser.execute((id) => {
        const item = document.querySelector(
          `#plugin-manager .pl-item[data-plugin="${id}"]`
        );
        const pop = item?.querySelector(".pl-info-pop");
        const btn = item?.querySelector("button.pl-info");
        return !!pop && pop.hidden && btn?.getAttribute("aria-expanded") === "false";
      }, pluginId),
    { timeout: 5000, timeoutMsg: `Поповер информации плагина ${pluginId} не скрылся` }
  );
});

// ---------- Панель плагинов: аккордеон, счётчик, подписи команд (TZ-UX-SPEC-CLEANUP.md §9) ----------
//
// Карточка `.pl-item` — аккордеон: голова `.pl-item-head` и тело `.pl-item-body`
// (id `pl-body-<id>`). Тело ВСЕГДА в DOM и скрывается только CSS (`.pl-item:not(.open)`
// → `display:none`); раскрытие задаёт класс `.pl-item.open` и `aria-expanded`/`aria-controls`
// шеврона. Клики — `browser.execute(el => el.click())` (проектный паттерн: панель
// может быть вне видимой области, synth-click работает и для скрытых CSS контролов).

/// Приводит карточку к целевому состоянию: кликаем по шеврону `button.pl-item-expand`
/// только если `.open` не совпадает с целью — иначе toggle инвертировал бы уже
/// достигнутое состояние, и шаг «сворачиваю» закрыл бы раскрытую карточку соседнего
/// сценария (состояние раскрытия живёт в `Set` вне DOM и переживает сценарии).
async function setExpandState(pluginId, open) {
  const ok = await browser.execute(
    (id, target) => {
      const item = document.querySelector(
        `#plugin-manager .pl-item[data-plugin="${id}"]`
      );
      const btn = item?.querySelector("button.pl-item-expand");
      if (!btn) return false;
      if (item.classList.contains("open") !== target) btn.click();
      return true;
    },
    pluginId,
    open
  );
  if (!ok) throw new Error(`Шеврон карточки плагина ${pluginId} не найден`);
}

When("я раскрываю карточку плагина {string}", async (pluginId) => {
  await setExpandState(pluginId, true);
});

When("я сворачиваю карточку плагина {string}", async (pluginId) => {
  await setExpandState(pluginId, false);
});

/// Свёрнутое состояние: нет `.open`, `aria-expanded="false"` и тело скрыто CSS
/// (computed `display` === `none`) — synth-click обошёл бы `display:none`, поэтому
/// видимость проверяем отдельно от наличия в DOM.
Then("карточка плагина {string} свёрнута", async (pluginId) => {
  await browser.waitUntil(
    async () =>
      browser.execute((id) => {
        const item = document.querySelector(
          `#plugin-manager .pl-item[data-plugin="${id}"]`
        );
        if (!item || item.classList.contains("open")) return false;
        const body = item.querySelector(".pl-item-body");
        const btn = item.querySelector("button.pl-item-expand");
        return (
          !!body &&
          !!btn &&
          btn.getAttribute("aria-expanded") === "false" &&
          getComputedStyle(body).display === "none"
        );
      }, pluginId),
    { timeout: 10000, timeoutMsg: `Карточка плагина ${pluginId} не свёрнута` }
  );
});

/// Раскрытое состояние — зеркало свёрнутого: `.open` есть, `aria-expanded="true"`,
/// тело реально показано (computed `display` ≠ `none`).
Then("карточка плагина {string} раскрыта", async (pluginId) => {
  await browser.waitUntil(
    async () =>
      browser.execute((id) => {
        const item = document.querySelector(
          `#plugin-manager .pl-item[data-plugin="${id}"]`
        );
        if (!item || !item.classList.contains("open")) return false;
        const body = item.querySelector(".pl-item-body");
        const btn = item.querySelector("button.pl-item-expand");
        return (
          !!body &&
          !!btn &&
          btn.getAttribute("aria-expanded") === "true" &&
          getComputedStyle(body).display !== "none"
        );
      }, pluginId),
    { timeout: 10000, timeoutMsg: `Карточка плагина ${pluginId} не раскрылась` }
  );
});

/// Красный тест на инвариант «тело всегда в DOM»: у свёрнутой карточки тело есть,
/// и его id — `pl-body-<id>` (иначе `aria-controls` шеврона повис бы в пустоту).
Then(
  "тело карточки плагина {string} в DOM даже когда карточка свёрнута",
  async (pluginId) => {
    await browser.waitUntil(
      async () =>
        browser.execute((id) => {
          const item = document.querySelector(
            `#plugin-manager .pl-item[data-plugin="${id}"]`
          );
          if (!item || item.classList.contains("open")) return false;
          const body = item.querySelector(".pl-item-body");
          return !!body && body.id === `pl-body-${id}`;
        }, pluginId),
      {
        timeout: 10000,
        timeoutMsg:
          `Тело карточки плагина ${pluginId} отсутствует в DOM при свёрнутой карточке`,
      }
    );
  }
);

/// Связь шеврона с телом: `aria-controls` совпадает с `body.id`, а `getElementById`
/// резолвит именно это тело (не только «атрибут есть»).
Then(
  "aria-controls шеврона плагина {string} указывает на тело карточки",
  async (pluginId) => {
    await browser.waitUntil(
      async () =>
        browser.execute((id) => {
          const item = document.querySelector(
            `#plugin-manager .pl-item[data-plugin="${id}"]`
          );
          const btn = item?.querySelector("button.pl-item-expand");
          const body = item?.querySelector(".pl-item-body");
          const controls = btn?.getAttribute("aria-controls");
          return (
            !!controls &&
            !!body &&
            controls === body.id &&
            document.getElementById(controls) === body
          );
        }, pluginId),
      {
        timeout: 5000,
        timeoutMsg:
          `aria-controls шеврона плагина ${pluginId} не указывает на тело карточки`,
      }
    );
  }
);

/// Подпись кнопки команды = `command.title` из манифеста (Фаза 2 отменила
/// безымянное «Выполнить»). Сверяем `textContent` с переданным title.
Then(
  "кнопка команды {string} {string} подписана {string}",
  async (pluginId, commandId, title) => {
    await browser.waitUntil(
      async () =>
        browser.execute(
          (pid, cid, expected) => {
            const btn = document.querySelector(
              `#plugin-manager .pl-item[data-plugin="${pid}"] .pl-cmd[data-command="${cid}"]`
            );
            return !!btn && (btn.textContent ?? "").trim() === expected;
          },
          pluginId,
          commandId,
          title
        ),
      {
        timeout: 10000,
        timeoutMsg:
          `Кнопка команды ${commandId} плагина ${pluginId} не подписана «${title}»`,
      }
    );
  }
);

/// Счётчик `#side-count` в шапке: число загруженных плагинов сверяем с числом
/// карточек `.pl-item` (устойчивее хардкода — набор фикстур может меняться).
/// Текст может быть «N» или «N с проблемой/с проблемами» (ветка `warn`) — берём
/// ведущее число, а не парсим строку целиком.
Then("счётчик плагинов в шапке равен числу карточек", async () => {
  await browser.waitUntil(
    async () =>
      browser.execute(() => {
        const countEl = document.getElementById("side-count");
        if (!countEl) return false;
        const match = /(\d+)/.exec(countEl.textContent ?? "");
        const shown = match ? Number(match[1]) : NaN;
        const cards = document.querySelectorAll("#plugin-manager .pl-item").length;
        return Number.isInteger(shown) && shown === cards;
      }),
    {
      timeout: 10000,
      timeoutMsg: "Счётчик #side-count не совпал с числом карточек плагинов",
    }
  );
});
