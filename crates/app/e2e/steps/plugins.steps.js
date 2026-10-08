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
// Панель плагинов скрыта, пока активна панель «Файлы»: открываем её кликом по
// rail-кнопке (если уже видима — не трогаем, иначе клик свернул бы sidebar).
// Тумблер вкл/выкл — настоящий `<input class="pl-enabled">` внутри
// `.pl-item[data-plugin=...]`; его click порождает change → IPC.

/// Показывает панель плагинов: клик по rail-кнопке, если панель скрыта. Общий
/// помощник для шага «я открываю панель плагинов» и Given о загрузке плагина без
/// view (его статус виден только в менеджере).
async function openPluginsPanel() {
  const visible = await browser.execute(() => {
    const panel = document.getElementById("panel-plugins");
    if (panel && !panel.hidden) return true;
    document.getElementById("rail-plugins")?.click();
    return !document.getElementById("panel-plugins")?.hidden;
  });
  if (!visible) {
    await browser.waitUntil(
      async () =>
        browser.execute(() => !document.getElementById("panel-plugins")?.hidden),
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
