//! `init`: создание шаблона плагина из встроенных файлов.

use std::path::{Path, PathBuf};

use crate::templates::{MANIFEST_FILE, TEMPLATE_MAIN_LUA, TEMPLATE_PLUGIN_JSON, TEMPLATE_README};


pub fn run_init(
    dir: &Path,
    id: Option<&str>,
    name: Option<&str>,
    force: bool,
) -> Result<Vec<PathBuf>, String> {
    let dir_name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let id = match id {
        Some(value) => value.to_string(),
        None => {
            let sanitized = sanitize_id(&dir_name);
            if sanitized.is_empty() {
                return Err(format!(
                    "не удалось определить id из имени каталога {dir_name:?}: используйте --id"
                ));
            }
            sanitized
        }
    };
    let name = name.map(|s| s.to_string()).unwrap_or(dir_name);

    let targets = [
        (dir.join(MANIFEST_FILE), TEMPLATE_PLUGIN_JSON),
        (dir.join("main.lua"), TEMPLATE_MAIN_LUA),
        (dir.join("README.md"), TEMPLATE_README),
    ];

    if !force {
        for (path, _) in &targets {
            if path.exists() {
                return Err(format!(
                    "{}: файл уже существует (используйте --force для перезаписи)",
                    path.display()
                ));
            }
        }
    }

    std::fs::create_dir_all(dir)
        .map_err(|e| format!("{}: не создать каталог: {e}", dir.display()))?;

    let mut created = Vec::with_capacity(targets.len());
    for (path, template) in &targets {
        let content = template.replace("{{ID}}", &id).replace("{{NAME}}", &name);
        std::fs::write(path, content)
            .map_err(|e| format!("{}: не записать: {e}", path.display()))?;
        created.push(path.clone());
    }
    Ok(created)
}



fn sanitize_id(name: &str) -> String {
    let mapped: String = name
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();

    let mut collapsed = String::with_capacity(mapped.len());
    let mut prev_dash = false;
    for ch in mapped.chars() {
        if ch == '-' {
            if !prev_dash {
                collapsed.push('-');
            }
            prev_dash = true;
        } else {
            collapsed.push(ch);
            prev_dash = false;
        }
    }
    collapsed.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn init_creates_three_files_with_substitution() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("my-plugin");
        let created = run_init(&dir, None, None, false).unwrap();
        assert_eq!(created.len(), 3);
        for path in &created {
            assert!(path.exists(), "{}: не создан", path.display());
        }

        let manifest_text = fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        assert!(
            manifest_text.contains("\"id\": \"my-plugin\""),
            "manifest: {manifest_text}"
        );
        assert!(
            manifest_text.contains("\"name\": \"my-plugin\""),
            "manifest: {manifest_text}"
        );
        let readme = fs::read_to_string(dir.join("README.md")).unwrap();
        assert!(readme.contains("my-plugin"), "readme: {readme}");
        let lua = fs::read_to_string(dir.join("main.lua")).unwrap();
        assert!(lua.contains("my-plugin"), "lua: {lua}");
    }

    #[test]
    fn init_honours_explicit_id_and_name() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Директория С Пробелами");
        run_init(&dir, Some("custom-id"), Some("Моё имя"), false).unwrap();
        let manifest_text = fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        assert!(
            manifest_text.contains("\"id\": \"custom-id\""),
            "{manifest_text}"
        );
        assert!(
            manifest_text.contains("\"name\": \"Моё имя\""),
            "{manifest_text}"
        );
    }

    #[test]
    fn init_refuses_to_overwrite_without_force() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("exists");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST_FILE), "оригинал").unwrap();

        let err = run_init(&dir, None, None, false).unwrap_err();
        assert!(err.contains("--force"), "err: {err}");
        assert_eq!(
            fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap(),
            "оригинал",
            "файл не должен быть перезаписан"
        );

        
        run_init(&dir, None, None, true).unwrap();
        assert_ne!(
            fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap(),
            "оригинал"
        );
    }

    #[test]
    fn sanitize_id_collapses_and_trims() {
        assert_eq!(sanitize_id("My Plugin!"), "my-plugin");
        assert_eq!(sanitize_id("--Hello__World--"), "hello__world");
        assert_eq!(sanitize_id("a..b"), "a-b");
        assert_eq!(sanitize_id("!!!"), "");
    }
}
