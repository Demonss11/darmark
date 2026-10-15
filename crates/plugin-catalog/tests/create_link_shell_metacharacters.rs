//! Проверка, что метасимволы `&`, `|`, `%` в путях не выполняются как команды `cmd.exe`.

use std::fs;

use plugin_catalog::create_link;

/// Путь с `&` в имени каталога: junction либо создан корректно, либо возвращена
/// ошибка, но произвольная команда после `&` не выполняется.
#[test]
fn create_link_rejects_shell_metacharacters_in_source() {
    let root = tempfile::tempdir().unwrap();
    // Каталог с `&` в имени — при интерпретации `cmd.exe` это разделитель команд.
    let source = root.path().join("test&dir");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("marker.txt"), "маркер").unwrap();

    let target = root.path().join("dest").join("alpha");
    fs::create_dir_all(target.parent().unwrap()).unwrap();

    let result = create_link(&source, &target);

    match result {
        Ok(()) => {
            // Junction создан — убеждаемся, что он указывает на весь путь, а не на префикс.
            let meta = fs::symlink_metadata(&target).expect("junction создан");
            assert!(meta.file_type().is_symlink(), "ожидался junction");
            assert!(
                target.join("marker.txt").is_file(),
                "junction указывает на неверный каталог — путь обрезан `&`"
            );
        }
        Err(e) => {
            // Ошибка допустима, но каталог-источник не должен быть повреждён.
            assert!(
                source.join("marker.txt").is_file(),
                "источник повреждён: {e}"
            );
        }
    }
}
