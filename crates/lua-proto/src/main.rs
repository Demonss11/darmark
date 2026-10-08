//! CLI измерительного стенда Lua-плагинного хоста (P1).
//!
//! Запуск: `cargo run -p lua-proto -- --plugin plugins/hello.lua`
//! Порядок жизненного цикла: создать песочницу → лимиты → host-функции →
//! `load` → `on_activate` → `on_deactivate`.

mod host;
mod limits;
mod sandbox;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use mlua::Function;

/// Лимит размера `.lua` перед загрузкой (DESIGN_DOC §10.1).
const MAX_PLUGIN_SIZE: u64 = 1024 * 1024;
const DEFAULT_MEM_LIMIT_MB: u64 = 64;
const DEFAULT_INSN_LIMIT: u64 = 5_000_000;

struct Args {
    plugin: PathBuf,
    limit_mem_mb: u64,
    limit_insns: u64,
    /// Спайк: лимит инструкций через нативный C-hook вместо `mlua::set_hook`.
    c_hook: bool,
    /// Шаг счётчика инструкций hook (M5/M6).
    hook_step: u32,
}

fn parse_args() -> Result<Args, String> {
    let mut plugin = None;
    let mut limit_mem_mb = DEFAULT_MEM_LIMIT_MB;
    let mut limit_insns = DEFAULT_INSN_LIMIT;
    let mut c_hook = false;
    let mut hook_step = limits::DEFAULT_HOOK_STEP;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plugin" => {
                plugin = Some(PathBuf::from(args.next().ok_or("--plugin требует путь")?));
            }
            "--limit-mem" => {
                limit_mem_mb = args
                    .next()
                    .ok_or("--limit-mem требует число (МБ)")?
                    .parse()
                    .map_err(|_| "--limit-mem: ожидается целое число".to_string())?;
            }
            "--limit-insns" => {
                limit_insns = args
                    .next()
                    .ok_or("--limit-insns требует число")?
                    .parse()
                    .map_err(|_| "--limit-insns: ожидается целое число".to_string())?;
            }
            "--c-hook" => {
                c_hook = true;
            }
            "--hook-step" => {
                hook_step = args
                    .next()
                    .ok_or("--hook-step требует число")?
                    .parse()
                    .map_err(|_| "--hook-step: ожидается целое число".to_string())?;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("неизвестный аргумент: {other}")),
        }
    }

    Ok(Args {
        plugin: plugin.ok_or("не указан --plugin <path>")?,
        limit_mem_mb,
        limit_insns,
        c_hook,
        hook_step,
    })
}

fn print_usage() {
    eprintln!("lua-proto — стенд Lua-плагинного хоста");
    eprintln!("  --plugin <path>       путь к .lua-плагину (обязательно)");
    eprintln!("  --limit-mem <mb>      лимит памяти Lua-кучи, 0 = без лимита (по умолчанию {DEFAULT_MEM_LIMIT_MB})");
    eprintln!("  --limit-insns <n>     лимит инструкций на вызов, 0 = без лимита (по умолчанию {DEFAULT_INSN_LIMIT})");
    eprintln!("  --c-hook              спайк: лимит инструкций через нативный C-hook");
    eprintln!(
        "  --hook-step <n>       шаг счётчика инструкций hook (по умолчанию {})",
        limits::DEFAULT_HOOK_STEP
    );
}

fn run(args: &Args) -> Result<(), String> {
    let metadata = std::fs::metadata(&args.plugin)
        .map_err(|e| format!("не читается {}: {e}", args.plugin.display()))?;
    if metadata.len() > MAX_PLUGIN_SIZE {
        return Err(format!(
            "плагин больше {} КБ ({} байт)",
            MAX_PLUGIN_SIZE / 1024,
            metadata.len()
        ));
    }
    let source = std::fs::read_to_string(&args.plugin)
        .map_err(|e| format!("не прочитать {}: {e}", args.plugin.display()))?;

    let plugin_id = args
        .plugin
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "plugin".to_string());

    let lua = sandbox::create().map_err(|e| format!("не создать Lua-состояние: {e}"))?;
    limits::set_memory_limit_mb(&lua, args.limit_mem_mb)
        .map_err(|e| format!("не поставить лимит памяти: {e}"))?;
    // RAII-гвард: hook взведён на всё время работы плагина и снимается на выходе.
    let _limit = if args.c_hook {
        limits::InstructionLimitGuard::arm_c(&lua, args.limit_insns, args.hook_step)
            .map_err(|e| format!("не поставить C-hook: {e}"))?
    } else {
        limits::InstructionLimitGuard::arm_mlua(&lua, args.limit_insns, args.hook_step)
            .map_err(|e| format!("не поставить лимит инструкций: {e}"))?
    };
    host::register(&lua, &plugin_id).map_err(|e| format!("не зарегистрировать host: {e}"))?;

    let started = Instant::now();

    lua.load(source.as_str())
        .set_name(plugin_id.as_str())
        .exec()
        .map_err(|e| format!("ошибка загрузки {plugin_id}: {e}"))?;

    let activate: Function = lua
        .globals()
        .get("on_activate")
        .map_err(|_| format!("{plugin_id}: отсутствует on_activate"))?;
    let ctx = lua
        .create_table()
        .map_err(|e| format!("не создать ctx: {e}"))?;
    ctx.set("plugin_id", plugin_id.as_str())
        .map_err(|e| format!("не заполнить ctx: {e}"))?;
    ctx.set("api_version", 1)
        .map_err(|e| format!("не заполнить ctx: {e}"))?;
    activate
        .call::<()>(ctx)
        .map_err(|e| format!("{plugin_id}: on_activate упал: {e}"))?;

    eprintln!(
        "[xost] {plugin_id}: активация за {:.2} мс, память Lua {:.1} КБ",
        started.elapsed().as_secs_f64() * 1000.0,
        limits::used_memory_kb(&lua)
    );

    if let Ok(deactivate) = lua.globals().get::<Function>("on_deactivate") {
        deactivate
            .call::<()>(())
            .map_err(|e| format!("{plugin_id}: on_deactivate упал: {e}"))?;
    }

    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("[xost] ошибка аргументов: {message}");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("[xost] ошибка: {message}");
            ExitCode::FAILURE
        }
    }
}
