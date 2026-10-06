//! Протокол C-спайка: длина+payload, ручная кодировка (без serde).
//!
//! Транспорт: stdio дочернего процесса.
//! Parent → Child: `CMD_RUN`, `CMD_REPLY`.
//! Child → Parent: `EV_READY`, `EV_HOSTCALL`, `EV_LOG`, `EV_DONE`.

use std::io::{self, Read, Write};

pub const CMD_RUN: u8 = 0x01;
pub const CMD_REPLY: u8 = 0x02;
pub const EV_READY: u8 = 0x10;
pub const EV_HOSTCALL: u8 = 0x11;
pub const EV_LOG: u8 = 0x12;
pub const EV_DONE: u8 = 0x13;

/// Пишет кадр: u32 LE длина + payload.
pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Читает один кадр.
pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut lb = [0u8; 4];
    r.read_exact(&mut lb)?;
    let len = u32::from_le_bytes(lb) as usize;
    let mut buf = vec![0u8; len];
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

/// Курсор чтения из payload.
pub struct Cur<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Cur<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, o: 0 }
    }

    pub fn u8(&mut self) -> u8 {
        let v = self.b[self.o];
        self.o += 1;
        v
    }

    pub fn u32(&mut self) -> u32 {
        let v = u32::from_le_bytes(self.b[self.o..self.o + 4].try_into().unwrap());
        self.o += 4;
        v
    }

    pub fn bytes(&mut self) -> &'a [u8] {
        let n = self.u32() as usize;
        let s = &self.b[self.o..self.o + n];
        self.o += n;
        s
    }

    pub fn str(&mut self) -> &'a str {
        std::str::from_utf8(self.bytes()).unwrap_or("")
    }
}
