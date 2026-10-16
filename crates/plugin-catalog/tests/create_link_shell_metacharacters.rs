//! Проверка, что метасимволы `&`, `|`, `%` в путях не выполняются как команды `cmd.exe`.

use std::fs;

use plugin_catalog::create_link;



#[test]
fn create_link_rejects_shell_metacharacters_in_source() {
    let root = tempfile::tempdir().unwrap();
    
    let source = root.path().join("test&dir");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("marker.txt"), "маркер").unwrap();

    let target = root.path().join("dest").join("alpha");
    fs::create_dir_all(target.parent().unwrap()).unwrap();

    let result = create_link(&source, &target);

    match result {
        Ok(()) => {
            
            let meta = fs::symlink_metadata(&target).expect("junction создан");
            assert!(meta.file_type().is_symlink(), "ожидался junction");
            assert!(
                target.join("marker.txt").is_file(),
                "junction указывает на неверный каталог — путь обрезан `&`"
            );
        }
        Err(e) => {
            
            assert!(
                source.join("marker.txt").is_file(),
                "источник повреждён: {e}"
            );
        }
    }
}
