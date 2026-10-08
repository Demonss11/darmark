//! Формулировки границы безопасности (§11.4 DESIGN_DOC, ADR-0021 §2.6).
//!
//! Единый источник текстов для UI и будущего магазина плагинов: изоляция ограничена
//! **отказами**, защита данных не обеспечивается.
//!
//! Перенесено из `lua-rpc-spike/src/lib.rs` (F38).

/// Общая памятка об изоляции. Показывается у каждого плагина.
pub const ISOLATION_NOTICE: &str =
    "Плагин исполняется в отдельном процессе: его сбой или зависание не затрагивают редактор. \
     Это изоляция отказов, а не защита данных.";

/// Предупреждение для плагинов с доступом к документу: текст читается полностью.
pub const DOCUMENT_ACCESS_NOTICE: &str =
    "Плагин с доступом к документу (document) читает его содержимое полностью, включая \
     конфиденциальный текст.";

/// Предупреждения по конкретным разрешениям. Рендеринг — задача UI (H2 Фаза 5).
pub fn permission_notices<P: AsRef<str>>(permissions: &[P]) -> Vec<&'static str> {
    let mut notices = vec![ISOLATION_NOTICE];
    if permissions
        .iter()
        .any(|p| is_document_permission(p.as_ref()))
    {
        notices.push(DOCUMENT_ACCESS_NOTICE);
    }
    notices
}

/// `document`, `document:read`, `document:write` — всё это доступ к содержимому документа.
fn is_document_permission(permission: &str) -> bool {
    permission == "document" || permission.starts_with("document:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_plugin_gets_only_isolation_notice() {
        assert_eq!(
            permission_notices(&["ui:statusbar"]),
            vec![ISOLATION_NOTICE]
        );
    }

    #[test]
    fn document_permission_adds_access_notice() {
        let doc = permission_notices(&["document:read"]);
        assert!(doc.contains(&ISOLATION_NOTICE));
        assert!(doc.contains(&DOCUMENT_ACCESS_NOTICE));
    }

    #[test]
    fn accepts_owned_permissions() {
        let owned = vec!["document:write".to_string()];
        assert!(permission_notices(&owned).contains(&DOCUMENT_ACCESS_NOTICE));
    }
}
