use std::path::{Path, PathBuf};
use std::process::Command;

use super::super::*;
use crate::error::ErrorCode;

/// Создаёт junction (`mklink /J`, без прав администратора). `false` — окружение не
/// позволило; вызывающий тест должен явно сообщить о пропуске.
fn try_junction(link: &Path, target: &Path) -> bool {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status();
    matches!(status, Ok(s) if s.success()) && std::fs::symlink_metadata(link).is_ok()
}

/// Создаёт symlink-файл (`mklink` без `/J`). Требует прав администратора или Developer
/// Mode; `false` — недоступно (тест сообщает о пропуске).
fn try_symlink_file(link: &Path, target: &Path) -> bool {
    let status = Command::new("cmd")
        .args(["/C", "mklink"])
        .arg(link)
        .arg(target)
        .status();
    matches!(status, Ok(s) if s.success()) && std::fs::symlink_metadata(link).is_ok()
}

#[test]
fn ensure_plugin_path_allowed_rejects_path_outside_whitelist() {
    let err = ensure_plugin_path_allowed(Path::new("C:\\Windows\\System32"))
        .expect_err("абсолютный путь вне белого списка");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}

#[test]
fn ensure_plugin_path_allowed_rejects_traversal() {
    let mut escaped = std::env::temp_dir();
    for _ in 0..8 {
        escaped.push("..");
    }
    escaped.push("Windows");
    let err = ensure_plugin_path_allowed(&escaped).expect_err("path traversal");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}

#[test]
fn ensure_plugin_path_allowed_accepts_path_inside_temp() {
    let dir = tempfile::tempdir().unwrap();
    let canonical = ensure_plugin_path_allowed(dir.path()).expect("temp разрешён");
    assert!(canonical.is_absolute(), "возвращается канонический путь");
}

#[test]
fn ensure_plugin_path_allowed_resolves_traversal_inside_temp() {
    let base = tempfile::tempdir().unwrap();
    let inner = base.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    let via_parent = inner.join("..");
    ensure_plugin_path_allowed(&via_parent).expect("`..` внутри temp разрешён");
}

#[test]
fn ensure_plugin_path_allowed_rejects_missing_path() {
    let missing = std::env::temp_dir().join(format!(
        "darmark-no-such-dir-{}-{}",
        std::process::id(),
        "abc"
    ));
    let err = ensure_plugin_path_allowed(&missing).expect_err("несуществующий путь");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}

#[test]
fn ensure_plugin_path_allowed_rejects_junction() {
    let base = tempfile::tempdir().unwrap();
    let target = base.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let link = base.path().join("link");
    if !try_junction(&link, &target) {
        eprintln!(
            "SKIP ensure_plugin_path_allowed_rejects_junction: mklink /J недоступен: {}",
            link.display()
        );
        return;
    }
    let err = ensure_plugin_path_allowed(&link).expect_err("junction отклонён");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}

#[test]
fn ensure_plugin_path_allowed_rejects_parent_junction_outward() {
    let base = tempfile::tempdir().unwrap();
    let allowed = base.path().join("allowed");
    let outside = base.path().join("outside");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let junction = allowed.join("junction");
    if !try_junction(&junction, &outside) {
        eprintln!(
            "SKIP ensure_plugin_path_allowed_rejects_parent_junction_outward: mklink /J недоступен: {}",
            junction.display()
        );
        return;
    }
    let child = junction.join("child");
    std::fs::create_dir(&child).unwrap();
    let err = ensure_plugin_path_allowed(&child).expect_err("родительский junction");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}

#[test]
fn ensure_plugin_path_allowed_rejects_symlinked_manifest_file() {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("plugin");
    std::fs::create_dir(&dir).unwrap();
    let target = base.path().join("secret.json");
    std::fs::write(&target, b"{}").unwrap();
    let link = dir.join("plugin.json");
    if !try_symlink_file(&link, &target) {
        eprintln!(
            "SKIP ensure_plugin_path_allowed_rejects_symlinked_manifest_file: symlink недоступен: {}",
            link.display()
        );
        return;
    }
    let err = ensure_plugin_path_allowed(&dir).expect_err("symlink plugin.json отклонён");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}

#[test]
fn ensure_plugin_path_allowed_rejects_symlinked_entry_file() {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("plugin");
    std::fs::create_dir(&dir).unwrap();
    let target = base.path().join("secret.lua");
    std::fs::write(&target, b"-- secret").unwrap();
    let link = dir.join("main.lua");
    if !try_symlink_file(&link, &target) {
        eprintln!(
            "SKIP ensure_plugin_path_allowed_rejects_symlinked_entry_file: symlink недоступен: {}",
            link.display()
        );
        return;
    }
    let err = ensure_plugin_path_allowed(&dir).expect_err("symlink main.lua отклонён");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}

#[test]
fn ensure_plugin_path_allowed_rejects_reparse_entry_without_symlink_privilege() {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("plugin");
    let target = base.path().join("target");
    std::fs::create_dir(&dir).unwrap();
    std::fs::create_dir(&target).unwrap();
    let link = dir.join("main.lua");
    if !try_junction(&link, &target) {
        eprintln!(
            "SKIP ensure_plugin_path_allowed_rejects_reparse_entry_without_symlink_privilege: mklink /J недоступен: {}",
            link.display()
        );
        return;
    }
    let err = ensure_plugin_path_allowed(&dir).expect_err("reparse main.lua отклонён");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}

#[test]
fn ensure_plugin_path_allowed_accepts_path_inside_plugins_root() {
    use crate::ai::history::env_lock;

    let root = tempfile::tempdir().unwrap();
    let plugin_dir = root.path().join("my-plugin");
    std::fs::create_dir(&plugin_dir).unwrap();

    // Общий env-лок `ai`: `DARMARK_PLUGINS_DIR` — переменная процесса, параллельные тесты
    // не должны видеть её промежуточное значение.
    let _guard = env_lock();
    let previous = std::env::var_os("DARMARK_PLUGINS_DIR");
    std::env::set_var("DARMARK_PLUGINS_DIR", root.path());
    let result = ensure_plugin_path_allowed(&plugin_dir);
    match previous {
        Some(value) => std::env::set_var("DARMARK_PLUGINS_DIR", value),
        None => std::env::remove_var("DARMARK_PLUGINS_DIR"),
    }

    result.expect("путь внутри plugins_root разрешён");
}

#[test]
fn ensure_plugin_path_allowed_accepts_root_and_normalized_layouts() {
    let temp = std::env::temp_dir();
    ensure_plugin_path_allowed(&temp).expect("сам корень temp разрешён");
    let dotted = temp.join(".").join("");
    ensure_plugin_path_allowed(&dotted).expect("нормализованная раскладка внутри temp");
    let upper = PathBuf::from(temp.to_string_lossy().to_uppercase());
    ensure_plugin_path_allowed(&upper).expect("регистр нормализуется канонизацией");
}

/// Объявленный в манифесте `entry` (не `main.lua`) читается после гейта.
#[test]
fn ensure_plugin_entry_allowed_accepts_declared_entry() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("init.lua"), b"-- init").unwrap();
    let canonical = ensure_plugin_path_allowed(dir.path()).expect("temp разрешён");

    let entry = ensure_plugin_entry_allowed(&canonical, "init.lua").expect("объявленный entry");

    assert_eq!(
        entry,
        canonical.join("init.lua"),
        "возвращается канонический путь"
    );
}

/// Отсутствующий или выходящий за каталог `entry` — отказ (fail-closed), а не чтение «мимо».
#[test]
fn ensure_plugin_entry_allowed_rejects_missing_and_traversal() {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("plugin");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(base.path().join("secret.lua"), b"-- secret").unwrap();
    let canonical = ensure_plugin_path_allowed(&dir).expect("temp разрешён");

    for entry in ["missing.lua", "../secret.lua"] {
        let err = ensure_plugin_entry_allowed(&canonical, entry)
            .expect_err("небезопасный entry обязан быть отклонён");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid, "{entry}: {err:?}");
    }
}

/// Symlink на объявленном `entry` отвергается тем же гейтом, что `plugin.json`/`main.lua`
/// (AC-MCP-12): иначе исходник читался бы вслед за ссылкой за пределы `plugins/`.
#[test]
fn ensure_plugin_entry_allowed_rejects_symlinked_entry() {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join("plugin");
    std::fs::create_dir(&dir).unwrap();
    let target = base.path().join("secret.lua");
    std::fs::write(&target, b"-- secret").unwrap();
    let link = dir.join("init.lua");
    if !try_symlink_file(&link, &target) {
        eprintln!(
            "SKIP ensure_plugin_entry_allowed_rejects_symlinked_entry: symlink недоступен: {}",
            link.display()
        );
        return;
    }
    let canonical = ensure_plugin_path_allowed(&dir).expect("temp разрешён");

    let err = ensure_plugin_entry_allowed(&canonical, "init.lua")
        .expect_err("symlink объявленного entry отклонён");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}
