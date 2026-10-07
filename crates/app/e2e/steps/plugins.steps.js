// Шаги плагинного тир-1 view (H2, Фаза 4). Список представлений приходит из
// Rust-хоста событием `plugin-views-changed`; клик по `data-p-<plugin_id>-action`
// возвращается в Lua-хендлер (`on_action`) и обновляет HTML контейнера.
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
  const ok = await browser.execute(
    (sel, id) => !!document.querySelector(`${sel} .vtab.plugin[data-view="${id}"]`),
    selector,
    viewId
  );
  if (!ok) throw new Error(`В ${selector} нет вкладки плагина ${viewId}`);
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
