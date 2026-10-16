//! Команды каталога плагинов: `list`, `install`, `link`, `remove`, `update`.
//!
//! Каталог вынесен в чистый крейт `plugin-catalog` (без plugin-host/mlua): его
//! переиспользуют и CLI, и будущие MCP-адаптеры (guard D6/ADR-0021).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use plugin_catalog::{Catalog, CatalogEntry};

pub fn run_list(repo: Option<PathBuf>, json: bool) -> Result<(), String> {
    let catalog = Catalog::load(&resolve_repo(repo))?;
    if json {
        print_catalog_json(&catalog);
    } else {
        print_catalog_text(&catalog);
    }
    Ok(())
}

pub fn run_install(
    id: &str,
    repo: Option<PathBuf>,
    dest: Option<PathBuf>,
    force: bool,
) -> Result<PathBuf, String> {
    let catalog = Catalog::load(&resolve_repo(repo))?;
    let dest = resolve_dest(dest)?;
    catalog.install(id, &dest, force)
}

pub fn run_link(
    id: &str,
    repo: Option<PathBuf>,
    dest: Option<PathBuf>,
    force: bool,
) -> Result<PathBuf, String> {
    let catalog = Catalog::load(&resolve_repo(repo))?;
    let dest = resolve_dest(dest)?;
    catalog.link(id, &dest, force)
}



pub fn run_remove(id: &str, dest: Option<PathBuf>) -> Result<(PathBuf, bool), String> {
    let dest = resolve_dest(dest)?;
    let removed = plugin_catalog::remove(id, &dest)?;
    Ok((dest.join(id), removed))
}

pub fn run_update(
    id: Option<&str>,
    repo: Option<PathBuf>,
    dest: Option<PathBuf>,
) -> Result<Vec<String>, String> {
    let catalog = Catalog::load(&resolve_repo(repo))?;
    let dest = resolve_dest(dest)?;
    catalog.update(id, &dest)
}


fn resolve_repo(explicit: Option<PathBuf>) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    resolve_repo_with(explicit, &cwd)
}



fn resolve_repo_with(explicit: Option<PathBuf>, cwd: &Path) -> PathBuf {
    if let Some(dir) = explicit {
        return normalize_repo(dir);
    }
    if let Some(found) = plugin_catalog::discover_repo(cwd) {
        return found;
    }
    cwd.join("plugins")
}




fn normalize_repo(dir: PathBuf) -> PathBuf {
    if dir.join(plugin_catalog::CATALOG_FILE).is_file() {
        return dir;
    }
    let nested = dir.join("plugins");
    if nested.join(plugin_catalog::CATALOG_FILE).is_file() {
        return nested;
    }
    dir
}


fn resolve_dest(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    let env = std::env::var_os("DARMARK_PLUGINS_DIR");
    let appdata = std::env::var_os("APPDATA");
    resolve_dest_with(explicit, env, appdata)
}



fn resolve_dest_with(
    explicit: Option<PathBuf>,
    env: Option<OsString>,
    appdata: Option<OsString>,
) -> Result<PathBuf, String> {
    if let Some(dir) = explicit {
        return Ok(dir);
    }
    if let Some(dir) = env {
        return Ok(PathBuf::from(dir));
    }
    let base = appdata
        .ok_or_else(|| "APPDATA не задан: укажите --dest или DARMARK_PLUGINS_DIR".to_string())?;
    Ok(PathBuf::from(base).join("darmark").join("plugins"))
}

fn print_catalog_text(catalog: &Catalog) {
    let entries = catalog.list();
    let permissions = |entry: &CatalogEntry| {
        if entry.permissions.is_empty() {
            "-".to_string()
        } else {
            entry.permissions.join(",")
        }
    };
    let id_w = entries
        .iter()
        .map(|e| e.id.len())
        .chain(std::iter::once("id".len()))
        .max()
        .unwrap_or(0);
    let version_w = entries
        .iter()
        .map(|e| e.version.len())
        .chain(std::iter::once("version".len()))
        .max()
        .unwrap_or(0);
    let permissions_w = entries
        .iter()
        .map(|e| permissions(e).len())
        .chain(std::iter::once("permissions".len()))
        .max()
        .unwrap_or(0);

    println!(
        "{:<id_w$}  {:<version_w$}  {:<permissions_w$}  description",
        "id", "version", "permissions"
    );
    for entry in entries {
        println!(
            "{:<id_w$}  {:<version_w$}  {:<permissions_w$}  {}",
            entry.id,
            entry.version,
            permissions(entry),
            entry.description
        );
    }
}

fn print_catalog_json(catalog: &Catalog) {
    let items: Vec<serde_json::Value> = catalog
        .list()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "id": entry.id,
                "name": entry.name,
                "version": entry.version,
                "api_version": entry.api_version,
                "permissions": entry.permissions,
                "description": entry.description,
                "path": entry.path,
            })
        })
        .collect();
    match serde_json::to_string_pretty(&serde_json::Value::Array(items)) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("ошибка сериализации JSON: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn os(value: &str) -> Option<OsString> {
        Some(OsString::from(value))
    }

    
    
    
    #[test]
    fn normalize_repo_accepts_catalog_dir_and_repo_root() {
        let root = tempfile::tempdir().unwrap();

        
        let plugins = root.path().join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join(plugin_catalog::CATALOG_FILE), "{}").unwrap();
        assert_eq!(normalize_repo(plugins.clone()), plugins);

        
        assert_eq!(normalize_repo(root.path().to_path_buf()), plugins);

        
        let empty = root.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(normalize_repo(empty.clone()), empty);
    }

    
    #[test]
    fn normalize_repo_ignores_examples_plugins_layout() {
        let root = tempfile::tempdir().unwrap();
        let examples = root.path().join("examples").join("plugins");
        fs::create_dir_all(&examples).unwrap();
        fs::write(examples.join(plugin_catalog::CATALOG_FILE), "{}").unwrap();
        
        assert_eq!(normalize_repo(root.path().to_path_buf()), root.path());
    }

    
    #[test]
    fn resolve_repo_prefers_explicit_then_discovery() {
        let root = tempfile::tempdir().unwrap();
        let explicit = root.path().join("explicit");
        fs::create_dir_all(&explicit).unwrap();

        
        assert_eq!(
            resolve_repo_with(Some(explicit.clone()), root.path()),
            explicit
        );
        assert_eq!(
            resolve_repo_with(None, root.path()),
            root.path().join("plugins"),
            "ничего не найдено — <cwd>/plugins"
        );
    }

    
    #[test]
    fn resolve_repo_discovers_plugins_from_nested_cwd() {
        let root = tempfile::tempdir().unwrap();
        let plugins = root.path().join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join(plugin_catalog::CATALOG_FILE), "{}").unwrap();

        let nested = root.path().join("crates").join("plugin-cli");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(resolve_repo_with(None, &nested), plugins);
    }

    #[test]
    fn resolve_dest_prefers_explicit_then_env_then_appdata() {
        assert_eq!(
            resolve_dest_with(
                Some(PathBuf::from("explicit")),
                os("ignored"),
                os("ignored")
            ),
            Ok(PathBuf::from("explicit"))
        );
        assert_eq!(
            resolve_dest_with(None, os("env-dest"), os("ignored")),
            Ok(PathBuf::from("env-dest")),
            "DARMARK_PLUGINS_DIR важнее APPDATA"
        );
        assert_eq!(
            resolve_dest_with(None, None, os("appdata-root")),
            Ok(PathBuf::from("appdata-root")
                .join("darmark")
                .join("plugins"))
        );
    }

    #[test]
    fn resolve_dest_without_appdata_errors() {
        let err = resolve_dest_with(None, None, None).unwrap_err();
        assert_eq!(
            err,
            "APPDATA не задан: укажите --dest или DARMARK_PLUGINS_DIR"
        );
    }
}
