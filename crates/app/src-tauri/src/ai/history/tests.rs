use super::*;

/// Временный каталог для теста (уникальный на процесс + имя).
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("darmark-history-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_messages(count: usize) -> Vec<AiMessage> {
    (0..count)
        .map(|i| AiMessage {
            role: if i % 2 == 0 {
                "user".to_string()
            } else {
                "assistant".to_string()
            },
            content: format!("сообщение {i}"),
        })
        .collect()
}

#[test]
fn history_roundtrip_trims_to_limit() {
    let dir = temp_dir("roundtrip");

    let history = HistoryFile {
        schema_version: SCHEMA_VERSION,
        session_id: "test-session".to_string(),
        updated_at: 1234567890,
        messages: make_messages(41),
    };
    save(Some(&dir), &history).expect("сохранение");

    let (loaded, warn) = load(Some(&dir));
    assert!(warn.is_none(), "предупреждения быть не должно: {warn:?}");
    assert_eq!(
        loaded.messages.len(),
        MAX_HISTORY_MESSAGES,
        "обрезано до лимита"
    );
    assert_eq!(
        loaded.messages.last().unwrap().content,
        "сообщение 40",
        "последнее сообщение сохранено"
    );
    assert_eq!(loaded.session_id, "test-session");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_survives_corrupt_file() {
    let dir = temp_dir("corrupt");
    let _path = history_path(Some(&dir)).unwrap();
    std::fs::write(&_path, b"{ not json").unwrap();

    let (loaded, warn) = load(Some(&dir));
    assert!(warn.is_some(), "должно быть предупреждение");
    assert!(loaded.messages.is_empty(), "пустая сессия");
    assert_eq!(loaded.session_id, "");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_rejects_oversized_file() {
    let dir = temp_dir("oversized");
    let path = history_path(Some(&dir)).unwrap();
    let f = std::fs::File::create(&path).unwrap();
    f.set_len(MAX_HISTORY_BYTES + 1).unwrap();
    drop(f);

    let (loaded, warn) = load(Some(&dir));
    assert!(warn.is_some(), "должно быть предупреждение");
    assert!(loaded.messages.is_empty(), "пустая сессия");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_write_is_atomic() {
    let dir = temp_dir("atomic");
    let path = history_path(Some(&dir)).unwrap();
    let tmp_path = path.with_extension("json.tmp");

    let history = HistoryFile {
        schema_version: SCHEMA_VERSION,
        session_id: "atomic".to_string(),
        updated_at: 0,
        messages: make_messages(3),
    };
    save(Some(&dir), &history).expect("сохранение");

    assert!(path.exists(), "файл истории создан");
    assert!(!tmp_path.exists(), "временный файл убран после rename");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_path_follows_data_dir() {
    let _guard = env_lock();

    let dir = temp_dir("path");
    let config_path = dir.join("config.json");
    std::env::set_var("DARMARK_CONFIG_PATH", &config_path);

    let data_dir =
        crate::plugins::settings::SettingsStore::data_dir().expect("data_dir через override");
    let history = history_path(Some(&data_dir)).expect("путь истории");
    std::env::remove_var("DARMARK_CONFIG_PATH");

    assert_eq!(
        history,
        dir.join("ai-history.json"),
        "путь истории — в каталоге данных"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_filters_disallowed_roles() {
    let dir = temp_dir("roles");

    let history = HistoryFile {
        schema_version: SCHEMA_VERSION,
        session_id: "roles".to_string(),
        updated_at: 0,
        messages: vec![
            AiMessage {
                role: "user".to_string(),
                content: "ok".to_string(),
            },
            AiMessage {
                role: "evil".to_string(),
                content: "bad".to_string(),
            },
            AiMessage {
                role: "assistant".to_string(),
                content: "ok2".to_string(),
            },
        ],
    };
    save(Some(&dir), &history).expect("сохранение");

    let (loaded, _) = load(Some(&dir));
    assert_eq!(loaded.messages.len(), 2, "роль вне списка отброшена");
    assert!(loaded.messages.iter().all(|m| m.role != "evil"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_unknown_schema_returns_empty() {
    let dir = temp_dir("schema");
    let path = history_path(Some(&dir)).unwrap();

    let raw = serde_json::json!({
        "schema_version": 999,
        "session_id": "test",
        "updated_at": 0,
        "messages": [{"role": "user", "content": "hi"}]
    });
    std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();

    let (loaded, warn) = load(Some(&dir));
    assert!(warn.is_some(), "должно быть предупреждение");
    assert!(loaded.messages.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_fits_within_byte_limit() {
    let dir = temp_dir("bytes");
    let chunk = "я".repeat(400_000);
    let history = HistoryFile {
        schema_version: SCHEMA_VERSION,
        session_id: "bytes".to_string(),
        updated_at: 0,
        messages: vec![
            AiMessage {
                role: "user".to_string(),
                content: chunk.clone(),
            },
            AiMessage {
                role: "assistant".to_string(),
                content: chunk.clone(),
            },
            AiMessage {
                role: "user".to_string(),
                content: format!("{chunk}последнее"),
            },
        ],
    };
    save(Some(&dir), &history).expect("сохранение");

    let path = history_path(Some(&dir)).unwrap();
    let size = std::fs::metadata(&path).expect("метаданные").len();
    assert!(
        size <= MAX_HISTORY_BYTES,
        "файл уложен в лимит байт: {size} > {MAX_HISTORY_BYTES}"
    );
    let (loaded, warn) = load(Some(&dir));
    assert!(
        warn.is_none(),
        "файл в лимите грузится без предупреждения: {warn:?}"
    );
    assert!(!loaded.messages.is_empty(), "хвост сохранён");
    assert!(
        loaded
            .messages
            .last()
            .unwrap()
            .content
            .ends_with("последнее"),
        "последнее сообщение сохраняется всегда"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_filters_disallowed_roles() {
    let dir = temp_dir("save-roles");

    let history = HistoryFile {
        schema_version: SCHEMA_VERSION,
        session_id: "save-roles".to_string(),
        updated_at: 0,
        messages: vec![
            AiMessage {
                role: "system".to_string(),
                content: "ok".to_string(),
            },
            AiMessage {
                role: "root".to_string(),
                content: "не должно попасть на диск".to_string(),
            },
        ],
    };
    save(Some(&dir), &history).expect("сохранение");
    let path = history_path(Some(&dir)).unwrap();
    let raw = std::fs::read_to_string(&path).expect("файл истории");
    assert!(
        !raw.contains("не должно попасть"),
        "роль отброшена при сохранении"
    );

    let (loaded, _) = load(Some(&dir));
    assert_eq!(loaded.messages.len(), 1);
    assert_eq!(loaded.messages[0].role, "system");

    let _ = std::fs::remove_dir_all(&dir);
}
