//! Подготовка окружения до подъёма плагинов: каталог настроек и WebView2-акселераторы.
//!
//! Вызывается первым в `.setup()`, до [`super::plugins::setup_plugins`]: конфиг должен
//! существовать на диске уже к моменту загрузки плагинов.

/// Создаёт каталог `%APPDATA%/darmark/` и `config.json` при первом запуске (хвост TZ-H1 п.8).
///
/// Автозапуск enabled-плагинов и их представлений поднимает
/// [`crate::setup::plugins::setup_plugins`] (Фаза 4); менеджер UI — Фаза 5. Здесь важен
/// лишь факт существования конфига, чтобы `SettingsStore::load` работал предсказуемо.
pub(crate) fn init_settings_dir() {
    use crate::plugins::settings::SettingsStore;
    if let Some(path) = SettingsStore::config_path() {
        if !path.exists() {
            if let Err(e) = SettingsStore::default().save(&path) {
                eprintln!("не создать {}: {e}", path.display());
            }
        }
    }
}

/// Отключает браузерные акселераторы WebView2 (Ctrl+P — печать, F5, Ctrl+F …).
///
/// По умолчанию WebView2 перехватывает их на уровне движка, и JS-обработчик `keydown`
/// их не получает: `Ctrl+P` открывал печать вместо переключения предпросмотра.
/// Обычные клавиши редактирования (Ctrl+C/V/X/A/Z) не затронуты.
pub(crate) fn disable_browser_accelerator_keys(app: &tauri::App) {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows_core::Interface;

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.with_webview(|webview| {
        let controller = webview.controller();
        unsafe {
            let Ok(core) = controller.CoreWebView2() else {
                return;
            };
            let Ok(settings) = core.Settings() else {
                return;
            };
            let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() else {
                return;
            };
            let _ = settings3.SetAreBrowserAcceleratorKeysEnabled(false);
        }
    });
}
