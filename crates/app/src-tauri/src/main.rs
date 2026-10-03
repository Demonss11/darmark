// Windows subsystem="windows" убирает консольное окно в релизе (как у Notepad++)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mdedit_lib::run()
}
