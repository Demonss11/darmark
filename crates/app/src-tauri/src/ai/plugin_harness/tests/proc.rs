use std::process::Command;
use std::time::Duration;

use super::super::proc::run_with_timeout;

/// «Болтливый» процесс: вывод больше буфера пайпа. Без параллельного чтения процесс
/// заблокировался бы на записи и таймаут сработал бы ложно.
#[test]
fn run_with_timeout_reads_large_output_without_false_timeout() {
    let mut command = Command::new("cmd");
    command.args(["/C", "for /L %i in (1,1,20000) do @echo line-%i"]);
    let output = run_with_timeout(command, Duration::from_secs(30)).expect("процесс завершился");
    assert!(output.success, "процесс успешен");
    assert!(
        output.stdout.len() > 64 * 1024,
        "вывод прочитан полностью: {} байт",
        output.stdout.len()
    );
}
