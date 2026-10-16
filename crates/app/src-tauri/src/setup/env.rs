//! Подготовка окружения до подъёма плагинов: каталог настроек и WebView2-акселераторы.
//!
//! Вызывается первым в `.setup()`, до [`super::plugins::setup_plugins`]: конфиг должен
//! существовать на диске уже к моменту загрузки плагинов.






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
