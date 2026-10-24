//! Reader for the PS4 `param.sfo` metadata file.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use indexmap::IndexMap;

/// A `param.sfo` is a few KiB; whatever lies beyond this under that name is not one.
pub const SFO_LIMIT: usize = 1 << 20;

const HEADER_LEN: usize = 20;
const ENTRY_LEN: usize = 16;
const FMT_INT: u16 = 0x404;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SfoValue {
    Int(u32),
    Text(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SfoError {
    #[error("param.sfo is empty (0 bytes): the game files are incomplete")]
    Empty,
    #[error("bad SFO signature")]
    BadSignature,
    #[error("bad SFO: truncated")]
    Truncated,
}

#[derive(Debug, thiserror::Error)]
pub enum SfoReadError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Invalid(#[from] SfoError),
}

fn span(data: &[u8], offset: usize, size: usize) -> Result<&[u8], SfoError> {
    let end = offset.checked_add(size).ok_or(SfoError::Truncated)?;
    data.get(offset..end).ok_or(SfoError::Truncated)
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, SfoError> {
    let bytes: [u8; 2] = span(data, offset, 2)?
        .try_into()
        .map_err(|_| SfoError::Truncated)?;
    Ok(u16::from_le_bytes(bytes))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, SfoError> {
    let bytes: [u8; 4] = span(data, offset, 4)?
        .try_into()
        .map_err(|_| SfoError::Truncated)?;
    Ok(u32::from_le_bytes(bytes))
}

fn usize_at(data: &[u8], offset: usize) -> Result<usize, SfoError> {
    usize::try_from(u32_at(data, offset)?).map_err(|_| SfoError::Truncated)
}

/// The key/value pairs of a `param.sfo`. Never panics on malformed input.
pub fn parse_sfo(data: &[u8]) -> Result<IndexMap<String, SfoValue>, SfoError> {
    if data.is_empty() {
        return Err(SfoError::Empty);
    }
    let header = span(data, 0, HEADER_LEN)?;
    if header.get(..4) != Some(b"\0PSF".as_slice()) {
        return Err(SfoError::BadSignature);
    }
    let keys = usize_at(data, 8)?;
    let values = usize_at(data, 12)?;
    let count = usize_at(data, 16)?;
    let mut result = IndexMap::new();
    for i in 0..count {
        let entry = i
            .checked_mul(ENTRY_LEN)
            .and_then(|n| n.checked_add(HEADER_LEN))
            .ok_or(SfoError::Truncated)?;
        let key_off = usize::from(u16_at(data, entry)?);
        let fmt = u16_at(data, entry + 2)?;
        let size = usize_at(data, entry + 4)?;
        let value_off = usize_at(data, entry + 12)?;

        let key_start = keys.checked_add(key_off).ok_or(SfoError::Truncated)?;
        let tail = data.get(key_start..).ok_or(SfoError::Truncated)?;
        let key_len = tail
            .iter()
            .position(|&b| b == 0)
            .ok_or(SfoError::Truncated)?;
        let key = String::from_utf8_lossy(tail.get(..key_len).ok_or(SfoError::Truncated)?);

        let value_start = values.checked_add(value_off).ok_or(SfoError::Truncated)?;
        let raw = span(data, value_start, size)?;
        let value = if fmt == FMT_INT {
            SfoValue::Int(u32_at(raw, 0)?)
        } else {
            let end = raw.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
            let trimmed = raw.get(..end).ok_or(SfoError::Truncated)?;
            SfoValue::Text(String::from_utf8_lossy(trimmed).into_owned())
        };
        result.insert(key.into_owned(), value);
    }
    Ok(result)
}

/// The values of a `param.sfo` file, reading at most [`SFO_LIMIT`] bytes.
pub fn load_sfo(path: &Path) -> Result<IndexMap<String, SfoValue>, SfoReadError> {
    let mut data = Vec::new();
    File::open(path)?
        .take(SFO_LIMIT as u64)
        .read_to_end(&mut data)?;
    Ok(parse_sfo(&data)?)
}

/// The values of a `param.sfo`, or `None` when it is missing, unreadable, empty or not one.
pub fn read_sfo(path: &Path) -> Option<IndexMap<String, SfoValue>> {
    load_sfo(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assembles an SFO: header, entry table, key table, value table.
    fn build(entries: &[(&str, u16, Vec<u8>)]) -> Vec<u8> {
        let keys_off = 20 + 16 * entries.len();
        let mut keys = Vec::new();
        let mut values = Vec::new();
        let mut table = Vec::new();
        for (key, fmt, value) in entries {
            table.extend_from_slice(&(keys.len() as u16).to_le_bytes());
            table.extend_from_slice(&fmt.to_le_bytes());
            table.extend_from_slice(&(value.len() as u32).to_le_bytes());
            table.extend_from_slice(&(value.len() as u32).to_le_bytes());
            table.extend_from_slice(&(values.len() as u32).to_le_bytes());
            keys.extend_from_slice(key.as_bytes());
            keys.push(0);
            values.extend_from_slice(value);
        }
        let values_off = keys_off + keys.len();
        let mut out = b"\0PSF".to_vec();
        out.extend_from_slice(&0x101u32.to_le_bytes());
        out.extend_from_slice(&(keys_off as u32).to_le_bytes());
        out.extend_from_slice(&(values_off as u32).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        out.extend(table);
        out.extend(keys);
        out.extend(values);
        out
    }

    fn text(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    }

    fn sample() -> Vec<u8> {
        build(&[
            ("ATTRIBUTE", 0x404, 2u32.to_le_bytes().to_vec()),
            ("TITLE_ID", 0x204, text("CUSA01623")),
            ("APP_VER", 0x204, text("01.02")),
            ("TITLE", 0x204, text("God of War® III Remastered")),
        ])
    }

    #[test]
    fn parses_the_keys_of_a_small_sfo() {
        let map = parse_sfo(&sample()).unwrap();
        assert_eq!(map.len(), 4);
        assert_eq!(map["ATTRIBUTE"], SfoValue::Int(2));
        assert_eq!(map["TITLE_ID"], SfoValue::Text("CUSA01623".into()));
        assert_eq!(map["APP_VER"], SfoValue::Text("01.02".into()));
        assert_eq!(
            map["TITLE"],
            SfoValue::Text("God of War® III Remastered".into())
        );
    }

    #[test]
    fn empty_is_an_error() {
        assert!(matches!(parse_sfo(&[]), Err(SfoError::Empty)));
    }

    #[test]
    fn wrong_magic_is_an_error() {
        let mut data = sample();
        data[1] = b'X';
        assert!(matches!(parse_sfo(&data), Err(SfoError::BadSignature)));
    }

    #[test]
    fn truncated_entry_table_is_an_error_not_a_panic() {
        let mut huge = b"\0PSF".to_vec();
        for n in [1u32, 40, 40, 100_000] {
            huge.extend_from_slice(&n.to_le_bytes());
        }
        huge.resize(40, 0);
        assert!(matches!(parse_sfo(&huge), Err(SfoError::Truncated)));

        let mut far_keys = sample();
        far_keys[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(parse_sfo(&far_keys), Err(SfoError::Truncated)));

        let short = &sample()[..10];
        assert!(matches!(parse_sfo(short), Err(SfoError::Truncated)));
        let cut = &sample()[..sample().len() - 5];
        assert!(matches!(parse_sfo(cut), Err(SfoError::Truncated)));
    }

    #[test]
    fn int_smaller_than_four_bytes_is_truncated() {
        let data = build(&[("X", 0x404, vec![1, 0])]);
        assert!(matches!(parse_sfo(&data), Err(SfoError::Truncated)));
    }

    #[test]
    fn load_and_read_use_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("none.sfo");
        assert!(matches!(load_sfo(&missing), Err(SfoReadError::Io(_))));
        assert_eq!(read_sfo(&missing), None);

        let path = dir.path().join("param.sfo");
        std::fs::write(&path, sample()).unwrap();
        assert_eq!(
            load_sfo(&path).unwrap()["TITLE_ID"],
            SfoValue::Text("CUSA01623".into())
        );
        assert_eq!(read_sfo(&path).unwrap()["ATTRIBUTE"], SfoValue::Int(2));

        let bad = dir.path().join("bad.sfo");
        std::fs::write(&bad, b"not an sfo at all, really").unwrap();
        assert!(matches!(
            load_sfo(&bad),
            Err(SfoReadError::Invalid(SfoError::BadSignature))
        ));
        assert_eq!(read_sfo(&bad), None);
    }
}
