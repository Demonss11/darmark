//! Кадрирование транспорта: `u32 LE длина || payload` (F36).
//!
//! Вывод child'а — недоверенный. `read_frame` ограничен [`MAX_EVENT_FRAME_BYTES`], а кадр
//! сверх лимита отвергается **до** аллокации. Нарушение протокола вызывающий обязан трактовать
//! как отказ плагина: GUI-хост собирается под `panic = "abort"`, где паника = обход изоляции.
//!
//! Перенесено из `lua-rpc-spike/src/lib.rs` как есть с тестами.

use std::io::{self, Read, Write};

/// Потолок размера **входящего в host** кадра (child → host). Легитимные входящие события малы:
/// лог, host-call (аргумент range/`apply_edit`), завершение.
pub const MAX_EVENT_FRAME_BYTES: usize = 1024 * 1024;

/// Потолок размера **входящего в child** кадра (host → child). Здесь крупные кадры легитимны:
/// ответ на `get_document_text` несёт документ ≤ `MAX_FILE_SIZE` (10 МБ).
pub const MAX_HOST_FRAME_BYTES: usize = 16 * 1024 * 1024;

fn protocol_error(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Пишет кадр: `u32 LE длина` + payload, затем flush.
pub fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Читает кадр события (child → host) с потолком [`MAX_EVENT_FRAME_BYTES`].
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
    fn read_frame_empty_payload_ok() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"").unwrap();
        assert_eq!(read_frame(&mut buf.as_slice()).unwrap(), Vec::<u8>::new());
    }
}
