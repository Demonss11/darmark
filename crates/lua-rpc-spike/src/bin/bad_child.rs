//! Враждебный child для гейта F36: на `CMD_RUN` отвечает **корректным по длине**, но
//! неполным по схеме кадром события (`EV_HOSTCALL` без полей). Host обязан разобрать это
//! как нарушение протокола (отказ плагина), а не паниковать: parent — GUI-хост под
//! `panic = "abort"`, где паника означала бы обход изоляции.

use std::io;
use std::time::Duration;

use lua_rpc_spike::{read_frame_host, write_frame, EV_HOSTCALL};

fn main() {
    let mut stdin = io::stdin();
    // Ждём команду от host'а (содержимое не важно).
    let _ = read_frame_host(&mut stdin);

    let mut stdout = io::stdout();
    // Кадр длиной ровно 1 Б — тег без обязательных полей события.
    let _ = write_frame(&mut stdout, &[EV_HOSTCALL]);

    // Дать host'у прочитать кадр, обнаружить нарушение и снять job.
    std::thread::sleep(Duration::from_millis(1500));
}
