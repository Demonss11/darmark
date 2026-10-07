//! Манифест плагина и его валидация (§4.1 TZ-H2, §6.4/§8 DESIGN_DOC).
//!
//! Валидация — первый барьер жизненного цикла (`scan → validate → load → activate`). Она ловит
//! несовместимый `api_version`, разрешения вне H2 и небезопасные значения `id`/`entry` до запуска
//! child-процесса.

use serde::{Deserialize, Serialize};

use crate::HOST_API_VERSION;

/// Разрешения, реально поддержанные хостом в H2 (§4.4 TZ-H2).
pub const H2_PERMISSIONS: &[&str] = &[
    "document:read",
    "document:write",
    "view:create",
    "view:modify",
    "ui:statusbar",
];

/// Разрешения, объявленные в §8 DESIGN_DOC, но отложенные за пределы H2. Распознаются как
/// **известные**, но отклоняются валидатором с отдельным сообщением (это не «неизвестное»).
pub const KNOWN_OUT_OF_H2_PERMISSIONS: &[&str] = &["ui:menu", "ui:sidebar", "ui:toolbar"];

/// Манифест плагина (`plugin.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub entry: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub contributes: Contributes,
}

/// Точки расширения (§6.4 DESIGN_DOC).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Contributes {
    #[serde(default)]
    pub views: Vec<ViewContrib>,
    #[serde(default)]
    pub commands: Vec<CommandContrib>,
    #[serde(default)]
    pub statusbar: Vec<StatusbarContrib>,
    #[serde(default)]
    pub toolbar: Vec<ToolbarContrib>,
    #[serde(default)]
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewContrib {
    pub kind: String,
    pub title: String,
    #[serde(default = "default_tier")]
    pub tier: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandContrib {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keybinding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusbarContrib {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolbarContrib {
    pub id: String,
    pub title: String,
    pub command: String,
}

fn default_tier() -> u8 {
    1
}

/// Ошибка валидации манифеста или исходника плагина.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// Не разобран JSON.
    Parse(String),
    /// Пустое обязательное поле.
    EmptyField(&'static str),
    /// `id` не является безопасным идентификатором.
    InvalidId(String),
    /// `api_version` новее, чем поддерживает хост.
    ApiVersionTooNew { found: u32, host: u32 },
    /// `entry` не является безопасным относительным `.lua`-файлом.
    InvalidEntry(String),
    /// Разрешение отсутствует в наборе §8.
    UnknownPermission(String),
    /// Разрешение известно, но не реализовано в H2.
    PermissionNotInH2(String),
    /// Разрешение не реализуется вообще (ФС/сеть).
    UnsupportedPermission(String),
    /// `.lua`-исходник больше лимита.
    SourceTooLarge { len: u64, max: u64 },
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "манифест не разобран: {e}"),
            Self::EmptyField(name) => write!(f, "обязательное поле пусто: {name}"),
            Self::InvalidId(id) => write!(
                f,
                "недопустимый id плагина {id:?}: только строчные латинские, цифры, '-' и '_'"
            ),
            Self::ApiVersionTooNew { found, host } => {
                write!(f, "api_version {found} новее поддерживаемого хостом {host}")
            }
            Self::InvalidEntry(entry) => {
                write!(
                    f,
                    "недопустимый entry {entry:?}: ожидается имя *.lua-файла без путей"
                )
            }
            Self::UnknownPermission(p) => write!(f, "неизвестное разрешение: {p}"),
            Self::PermissionNotInH2(p) => write!(f, "разрешение {p} не реализовано в H2"),
            Self::UnsupportedPermission(p) => {
                write!(
                    f,
                    "разрешение {p} не реализуется (ФС/сеть не предоставляются)"
                )
            }
            Self::SourceTooLarge { len, max } => {
                write!(f, "исходник плагина {len} байт превышает лимит {max} байт")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

impl Manifest {
    /// Разбирает манифест из JSON и сразу валидирует его.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ManifestError> {
        let manifest: Manifest =
            serde_json::from_slice(bytes).map_err(|e| ManifestError::Parse(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Проверяет обязательные поля, `api_version` и разрешения.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.id.is_empty() {
            return Err(ManifestError::EmptyField("id"));
        }
        if !is_safe_id(&self.id) {
            return Err(ManifestError::InvalidId(self.id.clone()));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::EmptyField("name"));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::EmptyField("version"));
        }
        if self.api_version > HOST_API_VERSION {
            return Err(ManifestError::ApiVersionTooNew {
                found: self.api_version,
                host: HOST_API_VERSION,
            });
        }
        if !is_safe_entry(&self.entry) {
            return Err(ManifestError::InvalidEntry(self.entry.clone()));
        }
        for permission in &self.permissions {
            classify_permission(permission)?;
        }
        Ok(())
    }
}

/// Результат проверки разрешения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionClass {
    /// Поддержано хостом в H2.
    Implemented,
    /// Известно из §8, но отложено за H2.
    KnownOutOfH2,
    /// Известно из §8, но не реализуется (ФС/сеть).
    Unsupported,
    /// Совсем неизвестно набору §8.
    Unknown,
}

/// Классифицирует разрешение, не считая его ошибкой.
pub fn permission_class(permission: &str) -> PermissionClass {
    if H2_PERMISSIONS.contains(&permission) {
        PermissionClass::Implemented
    } else if KNOWN_OUT_OF_H2_PERMISSIONS.contains(&permission) {
        PermissionClass::KnownOutOfH2
    } else if is_unsupported_permission(permission) {
        PermissionClass::Unsupported
    } else {
        PermissionClass::Unknown
    }
}

/// Проверяет разрешение и возвращает ошибку, если оно недопустимо в H2.
pub fn classify_permission(permission: &str) -> Result<PermissionClass, ManifestError> {
    let class = permission_class(permission);
    match class {
        PermissionClass::Implemented => Ok(class),
        PermissionClass::KnownOutOfH2 => {
            Err(ManifestError::PermissionNotInH2(permission.to_string()))
        }
        PermissionClass::Unsupported => {
            Err(ManifestError::UnsupportedPermission(permission.to_string()))
        }
        PermissionClass::Unknown => Err(ManifestError::UnknownPermission(permission.to_string())),
    }
}

/// Проверяет размер `.lua`-исходника перед загрузкой (§10.1 DESIGN_DOC).
pub fn validate_source_len(len: u64) -> Result<(), ManifestError> {
    let max = crate::MAX_PLUGIN_SOURCE_BYTES;
    if len > max {
        return Err(ManifestError::SourceTooLarge { len, max });
    }
    Ok(())
}

/// `filesystem:*` и `network` — известные категории §8, которые H2 не предоставляет вообще.
fn is_unsupported_permission(permission: &str) -> bool {
    permission == "network" || permission.starts_with("filesystem:")
}

/// Безопасный id: строчные латинские буквы, цифры, `-`, `_`; непустой.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Безопасный entry: имя файла, оканчивающееся на `.lua`, без разделителей пути и `..`.
fn is_safe_entry(entry: &str) -> bool {
    !entry.is_empty()
        && entry.ends_with(".lua")
        && !entry.contains('/')
        && !entry.contains('\\')
        && entry != ".."
        && !entry.contains("..")
        && !entry.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal(extra: serde_json::Value) -> Vec<u8> {
        let mut base = json!({
            "id": "word-count",
            "name": "Word Count",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua"
        });
        if let serde_json::Value::Object(ref mut map) = base {
            for (k, v) in extra.as_object().unwrap() {
                map.insert(k.clone(), v.clone());
            }
        }
        serde_json::to_vec(&base).unwrap()
    }

    #[test]
    fn accepts_reference_manifest() {
        let bytes = serde_json::to_vec(&json!({
            "id": "word-count",
            "name": "Word Count",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua",
            "permissions": ["document:read", "ui:statusbar"],
            "contributes": {
                "views": [{"kind": "wordcount", "title": "Статистика", "tier": 1}],
                "commands": [{"id": "wordcount.count", "title": "Посчитать", "keybinding": "Ctrl+Alt+W"}]
            }
        }))
        .unwrap();
        let m = Manifest::from_json(&bytes).unwrap();
        assert_eq!(m.id, "word-count");
        assert_eq!(m.contributes.views[0].tier, 1);
        assert_eq!(m.permissions, vec!["document:read", "ui:statusbar"]);
    }

    #[test]
    fn rejects_api_version_too_new() {
        let err = Manifest::from_json(&minimal(json!({"api_version": 2}))).unwrap_err();
        assert_eq!(err, ManifestError::ApiVersionTooNew { found: 2, host: 1 });
    }

    #[test]
    fn rejects_unknown_permission() {
        let err = Manifest::from_json(&minimal(json!({"permissions": ["quantum:teleport"]})))
            .unwrap_err();
        assert_eq!(
            err,
            ManifestError::UnknownPermission("quantum:teleport".into())
        );
    }

    #[test]
    fn rejects_known_permission_out_of_h2() {
        let err = Manifest::from_json(&minimal(json!({"permissions": ["ui:menu"]}))).unwrap_err();
        assert_eq!(err, ManifestError::PermissionNotInH2("ui:menu".into()));
    }

    #[test]
    fn rejects_unsupported_permissions() {
        for p in ["network", "filesystem:read", "filesystem:write"] {
            let err = Manifest::from_json(&minimal(json!({"permissions": [p]}))).unwrap_err();
            assert_eq!(err, ManifestError::UnsupportedPermission(p.into()));
        }
    }

    #[test]
    fn rejects_unsafe_id_and_entry() {
        assert!(matches!(
            Manifest::from_json(&minimal(json!({"id": "../evil"}))).unwrap_err(),
            ManifestError::InvalidId(_)
        ));
        assert!(matches!(
            Manifest::from_json(&minimal(json!({"entry": "../evil.lua"}))).unwrap_err(),
            ManifestError::InvalidEntry(_)
        ));
        assert!(matches!(
            Manifest::from_json(&minimal(json!({"entry": "main.txt"}))).unwrap_err(),
            ManifestError::InvalidEntry(_)
        ));
    }

    #[test]
    fn rejects_empty_required_fields() {
        assert_eq!(
            Manifest::from_json(&minimal(json!({"name": "  "}))).unwrap_err(),
            ManifestError::EmptyField("name")
        );
        assert_eq!(
            Manifest::from_json(&minimal(json!({"version": ""}))).unwrap_err(),
            ManifestError::EmptyField("version")
        );
    }

    #[test]
    fn source_len_gate() {
        assert!(validate_source_len(1024).is_ok());
        let err = validate_source_len(crate::MAX_PLUGIN_SOURCE_BYTES + 1).unwrap_err();
        assert!(matches!(err, ManifestError::SourceTooLarge { .. }));
    }

    #[test]
    fn missing_permissions_defaults_to_empty() {
        let m = Manifest::from_json(&minimal(json!({}))).unwrap();
        assert!(m.permissions.is_empty());
        assert!(m.contributes.views.is_empty());
    }
}
