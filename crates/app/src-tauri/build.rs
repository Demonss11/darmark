fn main() {
    // Иконка приложения встраивается в exe как Windows-ресурс на этапе build-скрипта,
    // но `tauri-build` не просит cargo перезапускаться при смене `icon.ico`. Без этой
    // строки обновлённая иконка не попадала в пересобранный exe (оставалась старая).
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build()
}
