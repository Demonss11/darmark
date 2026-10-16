use super::super::crud::map_register_error;
use super::super::*;
use crate::ai::types::AiValidationResult;
use crate::error::ErrorCode;
use serde_json::Value;
use std::path::Path;

#[test]
fn write_plugin_files_roundtrip() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("p");
    let manifest = serde_json::json!({
        "id": "p",
        "name": "P",
        "version": "1.0.0",
        "api_version": 1,
        "entry": "main.lua"
    });
    write_plugin_files(&dir, &manifest, "function on_activate(ctx) end").unwrap();
    assert!(dir.join("plugin.json").is_file());
    assert_eq!(
        std::fs::read_to_string(dir.join("main.lua")).unwrap(),
        "function on_activate(ctx) end"
    );
}

mod windows_crud {
    use super::*;
    use crate::plugins::host::PluginHost;
    use crate::plugins::manager::PluginManager;
    use crate::plugins::settings::SettingsStore;
    use crate::plugins::views::PluginViews;
    use std::sync::{Arc, Mutex};

    
    fn always_ok(_dir: &Path) -> AiValidationResult {
        AiValidationResult {
            ok: true,
            errors: Vec::new(),
            report: None,
        }
    }

    
    fn host_with_disabled(id: &str) -> PluginHost {
        let mut settings = SettingsStore::default();
        settings.set_plugin_enabled(id, false);
        PluginHost::new(
            PluginManager::default(),
            Arc::new(Mutex::new(PluginViews::new())),
            Arc::new(NoopServices),
            Arc::new(|| {}),
            Arc::new(Mutex::new(Vec::new())),
        )
        .with_settings(settings, None)
    }

    struct NoopServices;
    impl crate::plugins::HostServices for NoopServices {
        fn handle(
            &self,
            _method: &str,
            _args: Value,
        ) -> Result<Value, plugin_core::envelope::PluginError> {
            Err(plugin_core::envelope::PluginError::new(
                "noop",
                "нет услуг в тесте",
            ))
        }
    }

    fn manifest(id: &str) -> Value {
        serde_json::json!({
            "id": id,
            "name": id,
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua"
        })
    }

    const SOURCE: &str = "function on_activate(ctx) end\nfunction on_deactivate(ctx) end\n";

    #[test]
    fn create_update_delete_roundtrip_on_temp_dir() {
        let root = tempfile::tempdir().unwrap();
        let host = host_with_disabled("ai-test");

        let info = create_plugin_at(
            &host,
            root.path(),
            "ai-test",
            &manifest("ai-test"),
            SOURCE,
            always_ok,
        )
        .expect("создание");
        assert_eq!(info.id, "ai-test");
        assert!(root.path().join("ai-test").join("main.lua").is_file());
        let err = create_plugin_at(
            &host,
            root.path(),
            "ai-test",
            &manifest("ai-test"),
            SOURCE,
            always_ok,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::AiPluginExists);
        let updated = update_plugin_at(
            &host,
            root.path(),
            "ai-test",
            &manifest("ai-test"),
            SOURCE,
            always_ok,
        )
        .expect("обновление");
        assert_eq!(updated.id, "ai-test");
        delete_plugin_at(&host, "ai-test").expect("удаление");
        assert!(!root.path().join("ai-test").exists(), "каталог удалён");
        assert!(host.list_plugins().iter().all(|p| p.id != "ai-test"));
        let err = delete_plugin_at(&host, "ai-test").unwrap_err();
        assert_eq!(err.code, ErrorCode::AiPluginNotFound);
    }

    #[test]
    fn update_missing_plugin_reports_not_found() {
        let root = tempfile::tempdir().unwrap();
        let host = host_with_disabled("nope");
        let err = update_plugin_at(
            &host,
            root.path(),
            "nope",
            &manifest("nope"),
            SOURCE,
            always_ok,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::AiPluginNotFound);
    }

    #[test]
    fn invalid_validation_rolls_back_create() {
        let root = tempfile::tempdir().unwrap();
        let host = host_with_disabled("bad");
        let err = create_plugin_at(
            &host,
            root.path(),
            "bad",
            &manifest("bad"),
            SOURCE,
            |_dir| AiValidationResult {
                ok: false,
                errors: vec!["sandbox: упал".to_string()],
                report: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
        assert!(
            !root.path().join("bad").exists(),
            "невалидный плагин откатывается"
        );
    }

    #[test]
    fn register_error_codes_are_separated() {
        use plugin_core::envelope::PluginError;
        let cases = [
            ("plugin_exists", ErrorCode::AiPluginExists),
            ("no_host", ErrorCode::Plugin),
            ("register_failed", ErrorCode::AiPluginRegisterFailed),
            ("invalid_plugin", ErrorCode::AiPluginInvalid),
        ];
        for (code, expected) in cases {
            let err = map_register_error(PluginError::new(code, "msg"));
            assert_eq!(err.code, expected, "код для {code}");
        }
    }

    #[test]
    fn update_does_not_touch_files_when_proposed_content_invalid() {
        let root = tempfile::tempdir().unwrap();
        let host = host_with_disabled("keep");
        let dir = root.path().join("keep");
        write_plugin_files(&dir, &manifest("keep"), SOURCE).unwrap();
        let before = std::fs::read(dir.join("main.lua")).unwrap();

        let err = update_plugin_at(
            &host,
            root.path(),
            "keep",
            &manifest("keep"),
            "-- невалидный исходник",
            |_dir| AiValidationResult {
                ok: false,
                errors: vec!["sandbox: упал".to_string()],
                report: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
        assert_eq!(
            std::fs::read(dir.join("main.lua")).unwrap(),
            before,
            "невалидный контент не должен затирать рабочие файлы"
        );
    }

    #[test]
    fn create_rolls_back_directory_when_registration_fails() {
        let root = tempfile::tempdir().unwrap();
        let host = host_with_disabled("dup");
        let other = tempfile::tempdir().unwrap();
        let other_dir = other.path().join("dup");
        write_plugin_files(&other_dir, &manifest("dup"), SOURCE).unwrap();
        host.register_plugin_dir(&other_dir)
            .expect("первичная регистрация");

        let err = create_plugin_at(
            &host,
            root.path(),
            "dup",
            &manifest("dup"),
            SOURCE,
            always_ok,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::AiPluginExists);
        assert!(
            !root.path().join("dup").exists(),
            "каталог откатывается при ошибке регистрации"
        );
    }
}
