//! Read-only reader of a PlayStation Archive (PSARC): the file names and their contents.
//! A port of the `Psarc` class of `tools/xvag_voice_swap.py`.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use indexmap::IndexMap;
use md5::{Digest, Md5};

pub struct Psarc {
    file: File,
    compression: [u8; 4],
    block_size: u32,
    /// (first block, size, offset) of each entry.
    entries: Vec<(u32, u64, u64)>,
    blocks: Vec<u32>,
    names: IndexMap<String, usize>,
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |acc, b| (acc << 8) | u64::from(*b))
}

impl Psarc {
    pub fn open(path: &Path) -> io::Result<Psarc> {
        let mut file = File::open(path)?;
        let mut header = [0u8; 32];
        let mut got = 0;
        while got < header.len() {
            let n = file.read(&mut header[got..])?;
            if n == 0 {
                break;
            }
            got += n;
        }
        let not_psarc = || invalid(format!("{}: not a PSARC archive", path.display()));
        if got < header.len() || &header[..4] != b"PSAR" {
            return Err(not_psarc());
        }
        let compression = [header[8], header[9], header[10], header[11]];
        let field = |i: usize| be(&header[12 + 4 * i..16 + 4 * i]);
        let (toc_length, entry_size, count, block_size) = (
            field(0),
            field(1) as usize,
            field(2) as usize,
            field(3) as u32,
        );
        if toc_length < 32 || entry_size < 30 || block_size == 0 {
            return Err(not_psarc());
        }
        let mut toc = Vec::new();
        file.by_ref().take(toc_length - 32).read_to_end(&mut toc)?;
        let table_end = count
            .checked_mul(entry_size)
            .filter(|end| *end <= toc.len())
            .ok_or_else(not_psarc)?;
        let mut entries = Vec::with_capacity(count);
        let mut by_hash = std::collections::HashMap::new();
        for (i, e) in toc[..table_end].chunks(entry_size).enumerate() {
            by_hash.insert(e[..16].to_vec(), i);
            entries.push((be(&e[16..20]) as u32, be(&e[20..25]), be(&e[25..30])));
        }
        let step = if block_size <= 0x10000 {
            2
        } else if block_size <= 0x100_0000 {
            3
        } else {
            4
        };
        let blocks = toc[table_end..]
            .chunks_exact(step)
            .map(|c| be(c) as u32)
            .collect();
        let mut psarc = Psarc {
            file,
            compression,
            block_size,
            entries,
            blocks,
            names: IndexMap::new(),
        };
        // Entry 0 is the list of names; every other entry is found by the MD5 of its name (the
        // order differs).
        if psarc.entries.is_empty() {
            return Err(not_psarc());
        }
        let list = psarc.read(0)?;
        for name in String::from_utf8_lossy(&list).split('\n') {
            if name.is_empty() {
                continue;
            }
            let rooted = format!("/{}", name.trim_start_matches('/'));
            for spelled in [name, rooted.as_str()] {
                let digest = Md5::digest(spelled.as_bytes());
                if let Some(index) = by_hash.get(digest.as_slice()) {
                    let base = name.rsplit('/').next().unwrap_or(name).to_uppercase();
                    psarc.names.insert(base, *index);
                    break;
                }
            }
        }
        Ok(psarc)
    }

    /// The upper-case base name of each file, mapped to its entry index.
    pub fn names(&self) -> &IndexMap<String, usize> {
        &self.names
    }

    fn inflate(&self, data: Vec<u8>, size: usize) -> io::Result<Vec<u8>> {
        if data.len() == size {
            // a block stored as it is
            return Ok(data);
        }
        if &self.compression != b"zlib" {
            return Err(invalid(format!(
                "PSARC compression '{}' is not supported (zlib only)",
                String::from_utf8_lossy(&self.compression)
            )));
        }
        let mut out = Vec::with_capacity(size);
        flate2::read::ZlibDecoder::new(data.as_slice()).read_to_end(&mut out)?;
        if out.len() > size {
            return Err(invalid("PSARC block larger than expected".to_string()));
        }
        Ok(out)
    }

    pub fn read(&mut self, index: usize) -> io::Result<Vec<u8>> {
        let &(first, size, offset) = self
            .entries
            .get(index)
            .ok_or_else(|| invalid(format!("PSARC entry {index} does not exist")))?;
        self.file.seek(SeekFrom::Start(offset))?;
        let block_size = u64::from(self.block_size);
        let mut out: Vec<u8> = Vec::new();
        for b in 0..size.div_ceil(block_size) as usize {
            let want = block_size.min(size - out.len() as u64) as usize;
            let stored = *self
                .blocks
                .get(first as usize + b)
                .ok_or_else(|| invalid("PSARC block table is truncated".to_string()))?;
            let len = if stored == 0 {
                block_size
            } else {
                u64::from(stored)
            };
            let mut raw = Vec::new();
            self.file.by_ref().take(len).read_to_end(&mut raw)?;
            out.extend(self.inflate(raw, want)?);
        }
        Ok(out)
    }
}

/// A zlib PSARC holding `files` (`("/DIR/NAME", bytes)`); with `shuffle` the TOC entries are
/// stored in reverse to the name list. Entries are split into `block_size` blocks, each stored
/// compressed when that is shorter; a stored full block is 0 in the table, as Sony writes it.
#[cfg(test)]
pub(crate) fn build(files: &[(&str, &[u8])], shuffle: bool, block_size: u32) -> Vec<u8> {
    use std::io::Write;

    let names: Vec<&str> = files.iter().map(|(n, _)| *n).collect();
    let list = names.join("\n").into_bytes();
    let mut entries: Vec<([u8; 16], Vec<u8>)> = vec![([0; 16], list)];
    let ordered: Vec<&(&str, &[u8])> = if shuffle {
        files.iter().rev().collect()
    } else {
        files.iter().collect()
    };
    for (name, data) in ordered {
        let digest: [u8; 16] = Md5::digest(name.as_bytes()).into();
        entries.push((digest, data.to_vec()));
    }
    let (mut blocks, mut first, mut stored) =
        (Vec::<u32>::new(), Vec::new(), Vec::<Vec<u8>>::new());
    for (_, data) in &entries {
        first.push(blocks.len() as u32);
        for chunk in data.chunks(block_size as usize) {
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            z.write_all(chunk).unwrap();
            let packed = z.finish().unwrap();
            let packed = if packed.len() < chunk.len() {
                packed
            } else {
                chunk.to_vec()
            };
            blocks.push(if packed.len() as u32 == block_size {
                0
            } else {
                packed.len() as u32
            });
            stored.push(packed);
        }
    }
    let toc_length = 32 + 30 * entries.len() + 2 * blocks.len();
    let mut toc = Vec::new();
    let mut offset = toc_length as u64;
    let mut next = 0;
    for ((digest, data), index) in entries.iter().zip(&first) {
        toc.extend_from_slice(digest);
        toc.extend_from_slice(&index.to_be_bytes());
        toc.extend_from_slice(&(data.len() as u64).to_be_bytes()[3..]);
        toc.extend_from_slice(&offset.to_be_bytes()[3..]);
        for _ in 0..data.len().div_ceil(block_size as usize) {
            offset += stored[next].len() as u64;
            next += 1;
        }
    }
    let mut out = b"PSAR".to_vec();
    out.extend_from_slice(&[0, 1, 0, 4]);
    out.extend_from_slice(b"zlib");
    for v in [toc_length as u32, 30, entries.len() as u32, block_size, 2] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend(toc);
    for b in &blocks {
        out.extend_from_slice(&(*b as u16).to_be_bytes());
    }
    out.extend(stored.concat());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(len: usize) -> Vec<u8> {
        let mut x: u32 = 0x1234_5678;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })
            .collect()
    }

    fn open_bytes(bytes: &[u8]) -> (tempfile::TempDir, io::Result<Psarc>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.psarc");
        std::fs::write(&path, bytes).unwrap();
        let result = Psarc::open(&path);
        (dir, result)
    }

    #[test]
    fn names_are_found_by_their_md5_whatever_the_order() {
        let (a, b, z) = (b"aaaa".repeat(50), b"bbbb".repeat(70), b"zzzz".repeat(90));
        let files: [(&str, &[u8]); 3] = [
            ("/SOUNDS/STREAMS/A.XVAG", &a),
            ("/SOUNDS/STREAMS/B.XVAG", &b),
            ("/SOUNDS/STREAMS/CINEMATICS/Z.XVAG", &z),
        ];
        for shuffle in [true, false] {
            let (_dir, psarc) = open_bytes(&build(&files, shuffle, 65536));
            let mut psarc = psarc.unwrap();
            let names: Vec<String> = psarc.names().keys().cloned().collect();
            assert_eq!(names, ["A.XVAG", "B.XVAG", "Z.XVAG"]);
            for (name, data) in [("A.XVAG", &a), ("B.XVAG", &b), ("Z.XVAG", &z)] {
                let index = psarc.names()[name];
                assert_eq!(&psarc.read(index).unwrap(), data);
            }
        }
    }

    #[test]
    fn stored_blocks_are_read_as_they_are() {
        let data = noise(3000);
        let (_dir, psarc) = open_bytes(&build(&[("/S/R.XVAG", &data)], true, 65536));
        let mut psarc = psarc.unwrap();
        let index = psarc.names()["R.XVAG"];
        assert_eq!(psarc.read(index).unwrap(), data);
    }

    #[test]
    fn a_file_spans_blocks_and_a_full_stored_block_is_zero() {
        let data = noise(150_000);
        let bytes = build(&[("/S/BIG.XVAG", &data)], false, 65536);
        // entry 0 (names) has one block, then BIG has three: two full stored (0) and a tail.
        let table = 32 + 30 * 2;
        assert_eq!(&bytes[table + 2..table + 6], &[0, 0, 0, 0]);
        let (_dir, psarc) = open_bytes(&bytes);
        let mut psarc = psarc.unwrap();
        let index = psarc.names()["BIG.XVAG"];
        assert_eq!(psarc.read(index).unwrap(), data);
    }

    #[test]
    fn not_a_psarc_is_refused() {
        let (dir, result) = open_bytes(b"this is not an archive at all, really");
        let message = result.err().unwrap().to_string();
        assert_eq!(
            message,
            format!(
                "{}: not a PSARC archive",
                dir.path().join("a.psarc").display()
            )
        );
    }

    #[test]
    fn a_block_that_inflates_past_its_size_is_refused() {
        let data = b"a".repeat(2000);
        let mut bytes = build(&[("/S/A.XVAG", &data)], true, 2000);
        // The header now says 1000-byte blocks: the first block inflates to 2000.
        bytes[24..28].copy_from_slice(&1000u32.to_be_bytes());
        let (_dir, psarc) = open_bytes(&bytes);
        let mut psarc = psarc.unwrap();
        let index = psarc.names()["A.XVAG"];
        let error = psarc.read(index).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "PSARC block larger than expected");
    }

    #[test]
    fn other_compression_is_refused() {
        let data = b"abcd".repeat(100);
        let mut bytes = build(&[("/S/A.XVAG", &data)], true, 65536);
        bytes[8..12].copy_from_slice(b"lzma");
        // The name list is stored as it is, so the archive opens; a compressed block is refused.
        let (_dir, psarc) = open_bytes(&bytes);
        let mut psarc = psarc.unwrap();
        let index = psarc.names()["A.XVAG"];
        assert_eq!(
            psarc.read(index).err().unwrap().to_string(),
            "PSARC compression 'lzma' is not supported (zlib only)"
        );
    }
}
