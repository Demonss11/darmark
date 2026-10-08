//! Протокол C-спайка: длина+payload, ручная кодировка (без serde).
//!
//! Транспорт: stdio дочернего процесса.
//! Parent → Child: `CMD_RUN`, `CMD_RUN_N`, `CMD_REPLY`.
//! Child → Parent: `EV_READY`, `EV_HOSTCALL`, `EV_LOG`, `EV_DONE`.
//!
//! **F36 (hardening).** Вывод child'а — недоверенный. `read_frame` ограничен
//! [`MAX_FRAME_BYTES`], а разбор (`Cur`, `parse_*`) не паникует, а возвращает `io::Error`.
//! Вызывающий обязан трактовать нарушение протокола как отказ плагина: родитель — это
//! GUI-хост под `panic = "abort"`, где паника = обход изоляции.

use std::io::{self, Read, Write};

// Job Object (лимиты RSS/CPU/время) — Windows-only, P9/M11.
#[cfg(windows)]
pub mod job;

pub const CMD_RUN: u8 = 0x01;
pub const CMD_REPLY: u8 = 0x02;
/// Загрузить N плагинов в один процесс (модель «общий child», P9/M12):
/// u32 count, затем count × (str name, bytes source).
pub const CMD_RUN_N: u8 = 0x03;
pub const EV_READY: u8 = 0x10;
pub const EV_HOSTCALL: u8 = 0x11;
pub const EV_LOG: u8 = 0x12;
pub const EV_DONE: u8 = 0x13;

/// Потолок размера **входящего в host** кадра (child → host). Child — недоверенный источник,
/// поэтому кадр сверх лимита отвергается **до** аллокации (F36). Легитимные входящие события
/// малы: `EV_LOG`, `EV_HOSTCALL` (аргумент range/`apply_edit`), `EV_DONE`.
pub const MAX_EVENT_FRAME_BYTES: usize = 1024 * 1024;

/// Потолок размера **входящего в child** кадра (host → child). Здесь крупные кадры легитимны:
/// ответ на `get_document_text` несёт документ ≤ `MAX_FILE_SIZE` (10 МБ).
pub const MAX_HOST_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Ошибка нарушения протокола (недоверенный/укороченный кадр).
fn protocol_error(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Пишет кадр: u32 LE длина + payload.
pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Читает кадр события (child → host) с потолком [`MAX_EVENT_FRAME_BYTES`] (F36).
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    read_frame_capped(r, MAX_EVENT_FRAME_BYTES)
}

/// Читает кадр host → child (команда/ответ) с потолком [`MAX_HOST_FRAME_BYTES`].
pub fn read_frame_host<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    read_frame_capped(r, MAX_HOST_FRAME_BYTES)
}

fn read_frame_capped<R: Read>(r: &mut R, max: usize) -> io::Result<Vec<u8>> {
    let mut lb = [0u8; 4];
    r.read_exact(&mut lb)?;
    let len = u32::from_le_bytes(lb) as usize;
    if len > max {
        return Err(protocol_error(format!(
            "кадр {len} Б превышает лимит {max} Б"
        )));
    }
    let mut buf = Vec::new();
    buf.try_reserve_exact(len)
        .map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "не выделить буфер кадра"))?;
    buf.resize(len, 0);
    r.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_u32(out, b.len() as u32);
    out.extend_from_slice(b);
}

pub fn put_str(out: &mut Vec<u8>, s: &str) {
    put_bytes(out, s.as_bytes());
}

/// Курсор чтения из payload. Все методы **fallible**: укороченный кадр → `io::Error`,
/// а не паника (F36).
pub struct Cur<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Cur<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, o: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.b.len().saturating_sub(self.o)
    }

    fn need(&self, n: usize) -> io::Result<()> {
        if self.o + n <= self.b.len() {
            Ok(())
        } else {
            Err(protocol_error(format!(
                "кадр короче: нужно {n} Б, осталось {}",
                self.remaining()
            )))
        }
    }

    pub fn try_u8(&mut self) -> io::Result<u8> {
        self.need(1)?;
        let v = self.b[self.o];
        self.o += 1;
        Ok(v)
    }

    pub fn try_u32(&mut self) -> io::Result<u32> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.b[self.o..self.o + 4].try_into().unwrap());
        self.o += 4;
        Ok(v)
    }

    pub fn try_bytes(&mut self) -> io::Result<&'a [u8]> {
        let n = self.try_u32()? as usize;
        self.need(n)?;
        let s = &self.b[self.o..self.o + n];
        self.o += n;
        Ok(s)
    }

    pub fn try_str(&mut self) -> io::Result<&'a str> {
        Ok(std::str::from_utf8(self.try_bytes()?).unwrap_or(""))
    }
}

/// Событие child → parent, разобранное без паники (F36).
#[derive(Debug)]
pub enum Event<'a> {
    Ready,
    Log {
        level: &'a str,
        message: &'a str,
    },
    HostCall {
        id: u32,
        method: &'a str,
        arg: &'a [u8],
    },
    Done {
        status: u8,
        summary: &'a str,
    },
    Unknown(u8),
}

/// Разбирает кадр события. Нарушение формата → `io::Error`, не panic.
pub fn parse_event(p: &[u8]) -> io::Result<Event<'_>> {
    let mut c = Cur::new(p);
    let tag = c.try_u8()?;
    Ok(match tag {
        EV_READY => Event::Ready,
        EV_LOG => Event::Log {
            level: c.try_str()?,
            message: c.try_str()?,
        },
        EV_HOSTCALL => Event::HostCall {
            id: c.try_u32()?,
            method: c.try_str()?,
            arg: c.try_bytes()?,
        },
        EV_DONE => Event::Done {
            status: c.try_u8()?,
            summary: c.try_str()?,
        },
        other => Event::Unknown(other),
    })
}

/// Команда parent → child.
pub enum Command {
    Run(Vec<u8>),
    RunN(Vec<(String, Vec<u8>)>),
}

/// Разбирает команду. Число плагинов ограничено, чтобы доверенный-формально, но
/// потенциально искажённый кадр не вызвал гигантскую аллокацию (F36).
pub fn parse_command(p: &[u8]) -> io::Result<Command> {
    const MAX_PLUGINS: usize = 1024;
    let mut c = Cur::new(p);
    let tag = c.try_u8()?;
    match tag {
        CMD_RUN => Ok(Command::Run(c.try_bytes()?.to_vec())),
        CMD_RUN_N => {
            let n = c.try_u32()? as usize;
            if n > MAX_PLUGINS {
                return Err(protocol_error(format!("плагинов {n} > {MAX_PLUGINS}")));
            }
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                let name = c.try_str()?.to_string();
                let src = c.try_bytes()?.to_vec();
                v.push((name, src));
            }
            Ok(Command::RunN(v))
        }
        other => Err(protocol_error(format!(
            "ожидалась CMD_RUN/CMD_RUN_N, получено {other}"
        ))),
    }
}

/// Разбирает ответ parent → child (`CMD_REPLY`): (id, status, payload).
pub fn parse_reply(p: &[u8]) -> io::Result<(u32, u8, Vec<u8>)> {
    let mut c = Cur::new(p);
    let tag = c.try_u8()?;
    if tag != CMD_REPLY {
        return Err(protocol_error(format!(
            "ожидался CMD_REPLY, получено {tag}"
        )));
    }
    let id = c.try_u32()?;
    let status = c.try_u8()?;
    let payload = c.try_bytes()?.to_vec();
    Ok((id, status, payload))
}

/// Заглушка документа на стороне parent: единый обработчик host-вызовов для harness'ов.
///
/// Введён под D16/ADR-0021 (range/delta host-API, F22/F30): `get_document_text` отдаёт весь
/// документ (каждый child держит полную копию), а `get_document_range`/`get_document_len`
/// позволяют плагину взять окно. `apply_edit` (дельта-запись) здесь — заглушка `ok`.
pub fn serve_host_call(method: &str, arg: &[u8], document: &[u8]) -> Vec<u8> {
    match method {
        "get_document_text" => document.to_vec(),
        "get_document_version" => b"1".to_vec(),
        "get_document_len" => document.len().to_string().into_bytes(),
        "get_document_range" => {
            let mut c = Cur::new(arg);
            // Аргумент недоверенный — на нарушении формата отвечаем пустым окном.
            let doc_id = c.try_str();
            let start = c.try_u32();
            let len = c.try_u32();
            let (Ok(_doc_id), Ok(start), Ok(len)) = (doc_id, start, len) else {
                return Vec::new();
            };
            let start = start as usize;
            if start >= document.len() {
                return Vec::new();
            }
            let end = start.saturating_add(len as usize).min(document.len());
            document[start..end].to_vec()
        }
        "apply_edit" => b"ok".to_vec(),
        _ => Vec::new(),
    }
}

/// Состояние карантина одного плагина (ADR-0021 §3, §10.2 DESIGN_DOC): после `threshold`
/// **подряд** неудач плагин автоотключается до ручного включения. Успешный запуск сбрасывает
/// счётчик. Состояние per-plugin живёт в хосте (плагин stateless, §6.2).
#[derive(Debug, Clone)]
pub struct Quarantine {
    threshold: u32,
    consecutive_failures: u32,
    disabled: bool,
}

impl Quarantine {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1),
            consecutive_failures: 0,
            disabled: false,
        }
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Успешный запуск: серия неудач прервана.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Неудача запуска (краш, зависание, отказ): при накоплении `threshold` — карантин.
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.threshold {
            self.disabled = true;
        }
    }

    /// Ручное включение после карантина: счётчик и флаг сбрасываются.
    pub fn reset(&mut self) {
        self.consecutive_failures = 0;
        self.disabled = false;
    }
}

/// Формулировка границы безопасности (§11.4 DESIGN_DOC, ADR-0021 §2.6). Показывается в UI
/// и в магазине плагинов: изоляция ограничена **отказами**, защита данных не обеспечивается.
pub const ISOLATION_NOTICE: &str =
    "Плагин исполняется в отдельном процессе: его сбой или зависание не затрагивают редактор. \
     Это изоляция отказов, а не защита данных.";

/// Предупреждение для плагинов с доступом к документу (§11.4: «плагин с permission document
/// видит содержимое документа полностью»).
pub const DOCUMENT_ACCESS_NOTICE: &str =
    "Плагин с доступом к документу (document) читает его содержимое полностью, включая \
     конфиденциальный текст.";

/// Предупреждения для UI установки/магазина по разрешениям плагина (`contributes`/`permissions`).
/// Единый источник формулировок; рендеринг — задача UI (H2).
pub fn permission_notices(permissions: &[&str]) -> Vec<&'static str> {
    let mut notices = vec![ISOLATION_NOTICE];
    if permissions
        .iter()
        .any(|p| *p == "document" || p.starts_with("document:"))
    {
        notices.push(DOCUMENT_ACCESS_NOTICE);
    }
    notices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_frame_roundtrip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"hi").unwrap();
        assert_eq!(read_frame(&mut buf.as_slice()).unwrap(), b"hi");
    }

    #[test]
    fn read_frame_rejects_oversized_before_alloc() {
        let mut data = ((MAX_EVENT_FRAME_BYTES as u32) + 1).to_le_bytes().to_vec();
        data.extend_from_slice(&[0u8; 8]);
        let err = read_frame(&mut data.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_frame_host_allows_large_document_frame() {
        // Кадр host → child размером 2 МиБ проходит (документ ≤ MAX_FILE_SIZE), а как
        // событие (child → host) он был бы отвергнут лимитом MAX_EVENT_FRAME_BYTES.
        let mut buf = Vec::new();
        write_frame(&mut buf, &vec![7u8; 2 * 1024 * 1024]).unwrap();
        assert!(read_frame(&mut buf.as_slice()).is_err());
        assert!(read_frame_host(&mut buf.as_slice()).is_ok());
    }

    #[test]
    fn read_frame_truncated_body_is_error() {
        let mut data = 100u32.to_le_bytes().to_vec();
        data.extend_from_slice(&[1, 2, 3]);
        assert!(read_frame(&mut data.as_slice()).is_err());
    }

    #[test]
    fn parse_event_truncated_is_error_not_panic() {
        assert!(parse_event(&[EV_LOG]).is_err());
        let mut v = vec![EV_HOSTCALL];
        put_u32(&mut v, 1);
        assert!(parse_event(&v).is_err());
        assert!(parse_event(&[]).is_err());
    }

    #[test]
    fn parse_event_valid() {
        let mut v = vec![EV_DONE, 0];
        put_str(&mut v, "ok");
        match parse_event(&v).unwrap() {
            Event::Done { status, summary } => {
                assert_eq!(status, 0);
                assert_eq!(summary, "ok");
            }
            other => panic!("ожидался Done, получено {other:?}"),
        }
    }

    #[test]
    fn parse_command_rejects_huge_count() {
        let mut v = vec![CMD_RUN_N];
        put_u32(&mut v, u32::MAX);
        assert!(parse_command(&v).is_err());
    }

    #[test]
    fn cur_str_truncated_is_error() {
        let mut v = Vec::new();
        put_u32(&mut v, 10);
        v.extend_from_slice(b"abc");
        let mut c = Cur::new(&v);
        assert!(c.try_str().is_err());
    }

    #[test]
    fn quarantine_disables_after_threshold() {
        let mut q = Quarantine::new(3);
        q.record_failure();
        q.record_failure();
        assert!(!q.is_disabled());
        assert_eq!(q.consecutive_failures(), 2);
        q.record_failure();
        assert!(q.is_disabled(), "после 3 неудач подряд — карантин");
    }

    #[test]
    fn quarantine_success_resets_series() {
        let mut q = Quarantine::new(3);
        q.record_failure();
        q.record_failure();
        q.record_success();
        assert_eq!(q.consecutive_failures(), 0);
        q.record_failure();
        assert!(!q.is_disabled(), "серия прервана успехом");
    }

    #[test]
    fn quarantine_manual_reset_reenables() {
        let mut q = Quarantine::new(2);
        q.record_failure();
        q.record_failure();
        assert!(q.is_disabled());
        q.reset();
        assert!(!q.is_disabled());
        assert_eq!(q.consecutive_failures(), 0);
    }

    #[test]
    fn permission_notices_include_isolation_and_document() {
        let plain = permission_notices(&["ui:statusbar"]);
        assert_eq!(plain, vec![ISOLATION_NOTICE]);
        let doc = permission_notices(&["document:read"]);
        assert!(doc.contains(&ISOLATION_NOTICE));
        assert!(doc.contains(&DOCUMENT_ACCESS_NOTICE));
    }
}
