//! Child-процесс C-спайка: держит Lua 5.5 и ходит за host-функциями в parent
//! по stdio (честный round-trip). Никакого C-hook/`panic=abort`-трюка не нужно:
//! лимиты — забота parent (kill/Job Object).

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lua_rpc_spike::{
    put_bytes, put_str, put_u32, read_frame, write_frame, Cur, CMD_REPLY, CMD_RUN, EV_DONE,
    EV_HOSTCALL, EV_LOG, EV_READY,
};
use mlua::{Function, Lua, LuaOptions, Result as LuaResult, StdLib, Value};

static RPC_CALLS: AtomicU64 = AtomicU64::new(0);
static RPC_US_TOTAL: AtomicU64 = AtomicU64::new(0);
static RPC_US_MAX: AtomicU64 = AtomicU64::new(0);
static DOC_BYTES: AtomicU64 = AtomicU64::new(0);

struct Io {
    r: Box<dyn Read + Send>,
    w: Box<dyn Write + Send>,
}

/// Синхронный round-trip: EV_HOSTCALL → ждём CMD_REPLY.
fn rpc(io: &Arc<Mutex<Io>>, method: &str, arg: &[u8]) -> io::Result<(u8, Vec<u8>)> {
    let t = Instant::now();
    let mut guard = io.lock().unwrap();

    let mut msg = Vec::new();
    msg.push(EV_HOSTCALL);
    put_u32(&mut msg, 1);
    put_str(&mut msg, method);
    put_bytes(&mut msg, arg);
    write_frame(&mut guard.w, &msg)?;

    let resp = read_frame(&mut guard.r)?;
    drop(guard);

    let mut c = Cur::new(&resp);
    let tag = c.u8();
    if tag != CMD_REPLY {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected tag"));
    }
    let _id = c.u32();
    let status = c.u8();
    let payload = c.bytes().to_vec();

    let us = t.elapsed().as_micros() as u64;
    RPC_CALLS.fetch_add(1, Ordering::Relaxed);
    RPC_US_TOTAL.fetch_add(us, Ordering::Relaxed);
    RPC_US_MAX.fetch_max(us, Ordering::Relaxed);
    Ok((status, payload))
}

fn register_host(lua: &Lua, io: &Arc<Mutex<Io>>) -> LuaResult<()> {
    let host = lua.create_table()?;

    let io1 = Arc::clone(io);
    host.set(
        "get_document_text",
        lua.create_function(move |lua, doc_id: String| {
            let (_st, payload) = rpc(&io1, "get_document_text", doc_id.as_bytes())
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            DOC_BYTES.fetch_add(payload.len() as u64, Ordering::Relaxed);
            lua.create_string(&payload)
        })?,
    )?;

    let io2 = Arc::clone(io);
    host.set(
        "get_document_version",
        lua.create_function(move |_, doc_id: String| {
            let (_st, payload) = rpc(&io2, "get_document_version", doc_id.as_bytes())
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            let n = String::from_utf8_lossy(&payload)
                .trim()
                .parse::<i64>()
                .unwrap_or(0);
            Ok(n)
        })?,
    )?;

    let io3 = Arc::clone(io);
    host.set(
        "apply_edit",
        lua.create_function(
            move |_, (doc_id, start, stop, text): (String, i64, i64, String)| {
                let mut arg = Vec::new();
                put_str(&mut arg, &doc_id);
                put_u32(&mut arg, start as u32);
                put_u32(&mut arg, stop as u32);
                put_str(&mut arg, &text);
                let (status, _) = rpc(&io3, "apply_edit", &arg)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                Ok(status == 0)
            },
        )?,
    )?;

    let io4 = Arc::clone(io);
    host.set(
        "log",
        lua.create_function(move |_, (level, message): (String, String)| {
            let mut msg = Vec::new();
            msg.push(EV_LOG);
            put_str(&mut msg, &level);
            put_str(&mut msg, &message);
            let mut g = io4.lock().unwrap();
            write_frame(&mut g.w, &msg).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            Ok(())
        })?,
    )?;

    // Для проверки изоляции краха: child завершает сам себя.
    let io5 = Arc::clone(io);
    host.set(
        "crash",
        lua.create_function(move |_, ()| -> LuaResult<()> {
            let mut g = io5.lock().unwrap();
            let mut msg = Vec::new();
            msg.push(EV_LOG);
            put_str(&mut msg, "error");
            put_str(&mut msg, "child aborting (declared crash)");
            let _ = write_frame(&mut g.w, &msg);
            std::process::abort();
        })?,
    )?;

    lua.globals().set("host", host)?;
    Ok(())
}

fn main() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    let frame = match read_frame(&mut stdin) {
        Ok(f) => f,
        Err(_) => return,
    };
    let mut c = Cur::new(&frame);
    assert_eq!(c.u8(), CMD_RUN, "ожидалась CMD_RUN");
    let source = c.bytes().to_vec();

    let lua = Lua::new_with(
        StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )
    .expect("Lua::new_with");
    for name in ["load", "loadfile", "dofile", "collectgarbage"] {
        lua.globals().set(name, Value::Nil).expect("cleanup base");
    }

    let io_handle = Arc::new(Mutex::new(Io {
        r: Box::new(io::stdin()),
        w: Box::new(io::stdout()),
    }));
    register_host(&lua, &io_handle).expect("register host");

    write_frame(&mut stdout, &[EV_READY]).expect("ready");

    let started = Instant::now();
    let result: LuaResult<()> = (|| {
        let src = std::str::from_utf8(&source)
            .map_err(|e| mlua::Error::RuntimeError(format!("utf8: {e}")))?;
        lua.load(src).set_name("plugin").exec()?;
        let activate: Function = lua
            .globals()
            .get("on_activate")
            .map_err(|_| mlua::Error::RuntimeError("нет on_activate".into()))?;
        let ctx = lua.create_table()?;
        activate.call::<()>(ctx)?;
        Ok(())
    })();
    let elapsed = started.elapsed();

    let calls = RPC_CALLS.load(Ordering::Relaxed);
    let avg_us = if calls > 0 {
        RPC_US_TOTAL.load(Ordering::Relaxed) / calls
    } else {
        0
    };
    let summary = format!(
        "ok={} elapsed_ms={:.3} rpc_calls={} rpc_avg_us={} rpc_max_us={} doc_bytes={} err={}",
        result.is_ok(),
        elapsed.as_secs_f64() * 1000.0,
        calls,
        avg_us,
        RPC_US_MAX.load(Ordering::Relaxed),
        DOC_BYTES.load(Ordering::Relaxed),
        result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default()
    );

    let mut msg = vec![EV_DONE];
    msg.push(if result.is_ok() { 0 } else { 1 });
    put_str(&mut msg, &summary);
    let _ = write_frame(&mut stdout, &msg);
    // Дать parent прочитать кадр перед выходом.
    std::thread::sleep(Duration::from_millis(20));
}
