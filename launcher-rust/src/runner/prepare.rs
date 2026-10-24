//! The SELF loader (Python `scripts/prepare.py`): turns the game's plaintext `eboot.bin` into the
//! probe's loader image (`boot.bin`, `G3PROBE2`), the unpacked `eboot.elf`, `entry.bin` and
//! `analysis.json`. No decryption; every offset read from a file is checked before use.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use indexmap::{IndexMap, IndexSet};
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::core::fs::atomic_write;
use crate::core::sfo::{load_sfo, SfoError, SfoReadError, SfoValue};

const SELF_MAGIC: &[u8] = b"O\x15=\x1d";
const ELF_MAGIC: &[u8] = b"\x7fELF";
/// `e_ident[0..7]` of a 64-bit, little-endian, version 1 ELF.
const ELF_IDENT: &[u8] = b"\x7fELF\x02\x01\x01";
const EM_X86_64: u16 = 62;
/// Neither the unpacked ELF nor the memory image may be larger.
pub(super) const IMAGE_LIMIT: u64 = 512 * 1024 * 1024;

/// SELF segment flags: the entry carries a program header's bytes (its index in bits 20..32)…
const SELF_HOLDS_SEGMENT: u64 = 0x800;
/// …which are encrypted (2) or compressed (8).
const SELF_ENCRYPTED_OR_COMPRESSED: u64 = 0x2 | 0x8;

pub(super) const PT_LOAD: u32 = 1;
pub(super) const PT_DYNAMIC: u32 = 2;
pub(super) const PT_SCE_DYNLIBDATA: u32 = 0x6100_0000;
const PT_SCE_RELRO: u32 = 0x6100_0010;
pub(super) const PF_X: u32 = 1;

pub(super) const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
pub(super) const DT_SCE_JMPREL: u64 = 0x6100_0029;
pub(super) const DT_SCE_PLTRELSZ: u64 = 0x6100_002d;
pub(super) const DT_SCE_RELA: u64 = 0x6100_002f;
pub(super) const DT_SCE_RELASZ: u64 = 0x6100_0031;
pub(super) const DT_SCE_STRTAB: u64 = 0x6100_0035;
pub(super) const DT_SCE_STRSZ: u64 = 0x6100_0037;
pub(super) const DT_SCE_SYMTAB: u64 = 0x6100_0039;
pub(super) const DT_SCE_SYMTABSZ: u64 = 0x6100_003f;

pub(super) const R_X86_64_64: u32 = 1;
pub(super) const R_X86_64_GLOB_DAT: u32 = 6;
pub(super) const R_X86_64_JUMP_SLOT: u32 = 7;
pub(super) const R_X86_64_RELATIVE: u32 = 8;
pub(super) const SHN_ABS: u16 = 0xfff1;
pub(super) const STT_OBJECT: u8 = 1;
pub(super) const STT_FUNC: u8 = 2;

/// `Elf64_Sym` and `Elf64_Rela` are both 24 bytes.
pub(super) const SYMBOL_SIZE: u64 = 24;
pub(super) const RELOCATION_SIZE: u64 = 24;
/// `boot.bin` stores each import name NUL-padded to this size.
pub(super) const IMPORT_NAME_SIZE: usize = 128;
const RET: u8 = 0xc3;
const IOS_BASE_INIT: &str = "_ZNSt8ios_base4InitC1Ev";
const STATUS: &str = "Prepared only; execution and Vulkan are tested separately.";

/// The imports `analysis.json` names when the eboot uses them (`import_name_hints`).
const KNOWN_IMPORTS: &[&str] = &[
    "_init_env",
    "atexit",
    "exit",
    "_exit",
    "__cxa_atexit",
    "__cxa_finalize",
    "__cxa_guard_acquire",
    "__cxa_guard_release",
    "__cxa_guard_abort",
    "__stack_chk_guard",
    "__stack_chk_fail",
    "malloc",
    "free",
    "memcpy",
    "memmove",
    "memset",
    "memcmp",
    "strlen",
    "printf",
    IOS_BASE_INIT,
    "sceKernelLoadStartModule",
    "sceVideoOutOpen",
    "sceKernelCreateSema",
    "sceKernelWaitSema",
    "sceKernelPollSema",
    "sceKernelSignalSema",
    "sceKernelCancelSema",
    "sceKernelDeleteSema",
    "sceAppContentInitialize",
    "sceAppContentAppParamGetInt",
    "sceAppContentGetAddcontInfoList",
    "pthread_cond_init",
    "sceSysmoduleLoadModule",
    "pthread_mutexattr_init",
    "gettimeofday",
    "scePthreadMutexattrInit",
    "scePthreadMutexattrSettype",
    "scePthreadMutexattrDestroy",
    "scePthreadMutexInit",
    "scePthreadMutexLock",
    "scePthreadMutexTrylock",
    "scePthreadMutexUnlock",
    "scePthreadMutexDestroy",
    "scePthreadRwlockInit",
    "scePthreadRwlockDestroy",
    "scePthreadRwlockRdlock",
    "scePthreadRwlockWrlock",
    "scePthreadRwlockTryrdlock",
    "scePthreadRwlockTrywrlock",
    "scePthreadRwlockUnlock",
    "scePthreadRwlockTimedrdlock",
    "scePthreadRwlockTimedwrlock",
    "sceKernelGetDirectMemorySize",
    "sceKernelAllocateDirectMemory",
    "sceKernelMapDirectMemory",
    "sceKernelReleaseDirectMemory",
    "sceKernelMunmap",
];

/// Why a SELF could not be loaded. The texts are the Python's, except where the Python stopped on a
/// built-in error (a missing segment, tag or symbol, a bad name, a value out of range).
#[derive(Debug, thiserror::Error)]
pub enum PrepareError {
    #[error("empty SELF (0 bytes)")]
    EmptySelf,
    #[error("{} is empty (0 bytes): the game files are incomplete", file_name(.0))]
    EmptyFile(PathBuf),
    #[error("out of bounds: {offset:#x}+{size:#x} / {len:#x}")]
    Truncated { offset: u64, size: u64, len: u64 },
    #[error("expected PS4 SELF or ELF")]
    NotSelf,
    #[error("expected little-endian x86-64 ELF")]
    NotX86_64,
    #[error("expected little-endian x86-64 ELF")]
    BigEndian,
    #[error("unsupported program headers")]
    BadProgramHeader,
    #[error("probe image exceeds 512 MiB limit")]
    ImageTooLarge,
    #[error("encrypted/compressed SELF segment is unsupported")]
    Encrypted,
    #[error("invalid segment index")]
    BadSegmentIndex,
    #[error("unsupported blocked segment layout")]
    BlockedSegment,
    #[error("required segment {0} is unavailable")]
    RequiredSegment(usize),
    /// A program header the loader needs is absent (the Python stops with an empty message).
    #[error("no {0} segment")]
    MissingSegment(&'static str),
    #[error("no dynamic tag {0:#x}")]
    MissingTag(u64),
    #[error("bad name at {0:#x} of the string table")]
    BadName(u64),
    #[error("invalid memory image")]
    InvalidImage,
    #[error("segment file size exceeds memory size")]
    SegmentTooLarge,
    #[error("bad symbol table size")]
    BadSymbolTable,
    #[error("bad relocation table size")]
    BadRelocationTable,
    #[error("relocation outside mapped segments: {0:#x}")]
    RelocationOutside(u64),
    #[error("relocation symbol {0} does not exist")]
    BadSymbolIndex(u64),
    #[error("relocation value out of range")]
    RelocationValue,
    #[error("unsupported import addend")]
    ImportAddend,
    #[error("unsupported relocation {0}")]
    UnsupportedRelocation(u32),
    #[error("import name too long")]
    ImportNameTooLong,
    #[error(transparent)]
    Sfo(#[from] SfoError),
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// A segment as the loaders map it: where its bytes sit in the ELF and where they go in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub offset: u64,
    pub vaddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub flags: u32,
}

/// One ELF program header (`<IIQQQQQQ`), serialized in `analysis.json`'s key order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgramHeader {
    #[serde(rename = "type")]
    pub kind: u32,
    pub flags: u32,
    pub offset: u64,
    pub vaddr: u64,
    pub paddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

impl ProgramHeader {
    fn segment(&self) -> Segment {
        Segment {
            offset: self.offset,
            vaddr: self.vaddr,
            filesz: self.filesz,
            memsz: self.memsz,
            flags: self.flags,
        }
    }
}

/// One entry of a SELF's segment table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelfSegment {
    #[serde(serialize_with = "hex_number")]
    pub flags: u64,
    pub offset: u64,
    pub size: u64,
}

/// A parsed SELF or unpacked ELF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfInfo {
    /// The ELF with its headers and every segment at its program header's file offset.
    pub elf: Vec<u8>,
    /// `e_entry`.
    pub entry: u64,
    /// The loadable segments (`PT_LOAD`, `PT_SCE_RELRO`) in header order: what the probe maps.
    pub segments: Vec<Segment>,
    pub program_headers: Vec<ProgramHeader>,
    /// The SELF's segment table; empty for an unpacked ELF.
    pub self_segments: Vec<SelfSegment>,
    /// The program headers whose bytes the SELF does not carry (non-loadable metadata only).
    pub missing: Vec<usize>,
}

/// `_init_env` in the game's `libc.prx`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitEnv {
    pub symbol: String,
    pub address: u64,
    pub size: u64,
    /// Its code: the port relies on it being one `RET`.
    pub bytes: Vec<u8>,
}

/// `std::ios_base::Init::Init()` in the game's `libc.prx`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IosBaseInit {
    pub symbol: String,
    #[serde(serialize_with = "hex_number")]
    pub address: u64,
    pub size: u64,
    pub code_sha256: String,
    pub name: &'static str,
}

/// What `inspect_libc` proved (`analysis.json`'s `libc_evidence`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibcReport {
    /// SHA-256 of the file, hex.
    pub sha256: String,
    /// `_init_env` exists and its code is exactly `RET`.
    pub init_env_is_ret: bool,
    pub init_env: Option<InitEnv>,
    pub ios_base_init: Option<IosBaseInit>,
    /// The Python's dict keeps the order the symbols came in: `ios_base_init` before the keys of
    /// `_init_env` when its symbol came first.
    ios_base_init_first: bool,
}

impl Serialize for LibcReport {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let ios_base_init = self.ios_base_init.as_ref();
        let (before, after) = if self.ios_base_init_first {
            (ios_base_init, None)
        } else {
            (None, ios_base_init)
        };
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("sha256", &self.sha256)?;
        map.serialize_entry("init_env_is_ret", &self.init_env_is_ret)?;
        if let Some(ios) = before {
            map.serialize_entry("ios_base_init", ios)?;
        }
        if let Some(init) = &self.init_env {
            map.serialize_entry("symbol", &init.symbol)?;
            map.serialize_entry("address", &format!("{:#x}", init.address))?;
            map.serialize_entry("size", &init.size)?;
            map.serialize_entry("bytes", &hex(&init.bytes))?;
        }
        if let Some(ios) = after {
            map.serialize_entry("ios_base_init", ios)?;
        }
        map.end()
    }
}

/// What `prepare` found: `analysis.json`, in the Python's key order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub source_sha256: String,
    pub source_bytes: u64,
    #[serde(serialize_with = "sfo_values")]
    pub sfo: IndexMap<String, SfoValue>,
    #[serde(serialize_with = "hex_number")]
    pub entry: u64,
    pub image_bytes: u64,
    pub program_headers: Vec<ProgramHeader>,
    pub self_segments: Vec<SelfSegment>,
    pub unavailable_metadata_headers: Vec<usize>,
    pub needed: Vec<String>,
    /// Relocations by type, in the order the types first appear.
    pub relocation_counts: IndexMap<u32, u64>,
    pub import_count: usize,
    pub imports: Vec<String>,
    pub import_name_hints: IndexMap<String, &'static str>,
    pub libc_evidence: LibcReport,
    pub bundled_modules: Vec<String>,
    /// Files under `dvdroot_ps4` by their first path component.
    pub resources: IndexMap<String, u64>,
    pub resource_bytes: u64,
    pub status: &'static str,
}

/// A relocation as `boot.bin` stores it (`<QQqq`): kind 0 stores the load base plus `value`; 1 and
/// 2 store the address of import number `value` (a function, an object) plus `addend`.
struct Relocation {
    target: u64,
    kind: u64,
    value: i64,
    addend: i64,
}

/// A symbol of the library blob, with its name read from the string table.
struct Symbol {
    name: String,
    kind: u8,
    section: u16,
    value: u64,
    size: u64,
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> PrepareError + '_ {
    move |source| PrepareError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn hex_number<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("{value:#x}"))
}

fn sfo_values<S: Serializer>(
    values: &IndexMap<String, SfoValue>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(values.len()))?;
    for (key, value) in values {
        match value {
            SfoValue::Int(number) => map.serialize_entry(key, number)?,
            SfoValue::Text(text) => map.serialize_entry(key, text)?,
        }
    }
    map.end()
}

fn out_of_bounds(data: &[u8], offset: u64, size: u64) -> PrepareError {
    PrepareError::Truncated {
        offset,
        size,
        len: data.len() as u64,
    }
}

/// `data[offset..offset + size]`, or the out-of-bounds error.
pub(super) fn span(data: &[u8], offset: u64, size: u64) -> Result<&[u8], PrepareError> {
    let range = usize::try_from(offset).ok().zip(
        offset
            .checked_add(size)
            .and_then(|end| usize::try_from(end).ok()),
    );
    range
        .and_then(|(start, end)| data.get(start..end))
        .ok_or_else(|| out_of_bounds(data, offset, size))
}

fn array<const N: usize>(data: &[u8], offset: u64) -> Result<[u8; N], PrepareError> {
    span(data, offset, N as u64)?
        .try_into()
        .map_err(|_| out_of_bounds(data, offset, N as u64))
}

pub(super) fn u8_at(data: &[u8], offset: u64) -> Result<u8, PrepareError> {
    Ok(u8::from_le_bytes(array(data, offset)?))
}

pub(super) fn u16_at(data: &[u8], offset: u64) -> Result<u16, PrepareError> {
    Ok(u16::from_le_bytes(array(data, offset)?))
}

pub(super) fn u32_at(data: &[u8], offset: u64) -> Result<u32, PrepareError> {
    Ok(u32::from_le_bytes(array(data, offset)?))
}

pub(super) fn u64_at(data: &[u8], offset: u64) -> Result<u64, PrepareError> {
    Ok(u64::from_le_bytes(array(data, offset)?))
}

pub(super) fn i64_at(data: &[u8], offset: u64) -> Result<i64, PrepareError> {
    Ok(i64::from_le_bytes(array(data, offset)?))
}

/// Copies `bytes` to offset `at` of `target`, or the out-of-bounds error when they do not fit.
pub(super) fn place(target: &mut [u8], at: u64, bytes: &[u8]) -> Result<(), PrepareError> {
    let error = out_of_bounds(target, at, bytes.len() as u64);
    let start = usize::try_from(at).ok();
    let end = start.and_then(|start| start.checked_add(bytes.len()));
    let slot = start
        .zip(end)
        .and_then(|(start, end)| target.get_mut(start..end))
        .ok_or(error)?;
    slot.copy_from_slice(bytes);
    Ok(())
}

fn read_program_header(data: &[u8], offset: u64) -> Result<ProgramHeader, PrepareError> {
    let record = span(data, offset, 56)?;
    Ok(ProgramHeader {
        kind: u32_at(record, 0)?,
        flags: u32_at(record, 4)?,
        offset: u64_at(record, 8)?,
        vaddr: u64_at(record, 16)?,
        paddr: u64_at(record, 24)?,
        filesz: u64_at(record, 32)?,
        memsz: u64_at(record, 40)?,
        align: u64_at(record, 48)?,
    })
}

/// Parses a plaintext SELF or an unpacked ELF (x86-64, little-endian) and lays its segments out at
/// their program headers' file offsets. Encrypted or compressed segments are refused.
pub fn parse_self(data: &[u8]) -> Result<SelfInfo, PrepareError> {
    if data.is_empty() {
        // read_self names the file; this covers every other way in.
        return Err(PrepareError::EmptySelf);
    }
    let magic = span(data, 0, 4)?;
    let plain_elf = magic == ELF_MAGIC;
    let (count, base) = if plain_elf {
        (0, 0)
    } else if magic == SELF_MAGIC {
        let count = u64::from(u16_at(data, 24)?);
        (count, 32 + count * 32)
    } else {
        return Err(PrepareError::NotSelf);
    };
    let header = span(data, base, 64)?;
    let ident = span(header, 0, 7)?;
    if ident.get(5) != Some(&1) {
        return Err(PrepareError::BigEndian);
    }
    if ident != ELF_IDENT || u16_at(header, 18)? != EM_X86_64 {
        return Err(PrepareError::NotX86_64);
    }
    let entry = u64_at(header, 24)?;
    let phoff = u64_at(header, 32)?;
    let phnum = u16_at(header, 56)?;
    if u16_at(header, 54)? != 56 || !(1..256).contains(&phnum) {
        return Err(PrepareError::BadProgramHeader);
    }
    let program_headers = (0..u64::from(phnum))
        .map(|i| read_program_header(data, base.saturating_add(phoff).saturating_add(i * 56)))
        .collect::<Result<Vec<_>, _>>()?;
    let header_end = phoff.saturating_add(u64::from(phnum) * 56);
    let end = program_headers
        .iter()
        .map(|p| p.offset.saturating_add(p.filesz))
        .fold(header_end, u64::max);
    if end > IMAGE_LIMIT {
        return Err(PrepareError::ImageTooLarge);
    }
    let segments = program_headers
        .iter()
        .filter(|p| p.kind == PT_LOAD || p.kind == PT_SCE_RELRO)
        .map(ProgramHeader::segment)
        .collect();
    if plain_elf {
        // An unpacked ELF already holds its segments at their file offsets.
        return Ok(SelfInfo {
            elf: span(data, 0, end)?.to_vec(),
            entry,
            segments,
            program_headers,
            self_segments: Vec::new(),
            missing: Vec::new(),
        });
    }
    let mut elf = vec![0; usize::try_from(end).map_err(|_| PrepareError::ImageTooLarge)?];
    place(&mut elf, 0, span(data, base, header_end)?)?;
    let mut covered = vec![(0, header_end)];
    let mut self_segments = Vec::new();
    for i in 0..count {
        let record = span(data, 32 + i * 32, 32)?;
        let segment = SelfSegment {
            flags: u64_at(record, 0)?,
            offset: u64_at(record, 8)?,
            size: u64_at(record, 16)?,
        };
        let memory = u64_at(record, 24)?;
        let (flags, offset, size) = (segment.flags, segment.offset, segment.size);
        self_segments.push(segment);
        if flags & SELF_HOLDS_SEGMENT == 0 {
            continue;
        }
        if flags & SELF_ENCRYPTED_OR_COMPRESSED != 0 {
            return Err(PrepareError::Encrypted);
        }
        let header = usize::try_from((flags >> 20) & 0xfff)
            .ok()
            .and_then(|index| program_headers.get(index))
            .ok_or(PrepareError::BadSegmentIndex)?;
        if size != header.filesz || size != memory {
            return Err(PrepareError::BlockedSegment);
        }
        place(&mut elf, header.offset, span(data, offset, size)?)?;
        covered.push((header.offset, header.offset.saturating_add(size)));
    }
    let mut missing = Vec::new();
    for (i, p) in program_headers.iter().enumerate() {
        let stop = p.offset.saturating_add(p.filesz);
        if p.filesz != 0 && !covered.iter().any(|&(a, z)| a <= p.offset && stop <= z) {
            missing.push(i);
            if matches!(
                p.kind,
                PT_LOAD | PT_DYNAMIC | PT_SCE_DYNLIBDATA | PT_SCE_RELRO
            ) {
                return Err(PrepareError::RequiredSegment(i));
            }
        }
    }
    Ok(SelfInfo {
        elf,
        entry,
        segments,
        program_headers,
        self_segments,
        missing,
    })
}

/// The bytes of a SELF file, which must hold something: an empty `eboot.bin` or prx (a copy that
/// failed, a placeholder made by hand) is named here instead of failing deep in the parser.
pub fn read_self_bytes(path: &Path) -> Result<Vec<u8>, PrepareError> {
    let data = fs::read(path).map_err(io_error(path))?;
    if data.is_empty() {
        return Err(PrepareError::EmptyFile(path.to_path_buf()));
    }
    Ok(data)
}

/// The SELF or ELF in `path`, parsed.
pub fn read_self(path: &Path) -> Result<SelfInfo, PrepareError> {
    parse_self(&read_self_bytes(path)?)
}

/// The NID of a symbol name: the first 8 bytes of its salted SHA-1, reversed, in 11 characters of
/// base64 whose `/` is `-`.
pub fn nid(name: &str) -> String {
    const SALT: [u8; 16] = [
        0x51, 0x8d, 0x64, 0xa6, 0x35, 0xde, 0xd8, 0xc1, 0xe6, 0xb0, 0x39, 0xb1, 0xc3, 0xe5, 0x52,
        0x30,
    ];
    let mut input = name.as_bytes().to_vec();
    input.extend_from_slice(&SALT);
    let head: Vec<u8> = sha1(&input).iter().take(8).rev().copied().collect();
    base64_unpadded(&head).replace('/', "-")
}

/// The part of a symbol name before its first `#`: the NID without the library and module ids.
fn nid_part(name: &str) -> &str {
    name.split_once('#').map_or(name, |(head, _)| head)
}

fn base64_unpadded(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let byte = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
        let group = byte(0) << 16 | byte(1) << 8 | byte(2);
        for i in 0..=chunk.len() {
            let digit = (group >> (18 - 6 * i)) & 63;
            text.push(char::from(ALPHABET[digit as usize]));
        }
    }
    text
}

/// SHA-1 (FIPS 180-4), for the NIDs only.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [
        0x6745_2301,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&(data.len() as u64).wrapping_mul(8).to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes(bytes.try_into().unwrap_or_default());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (i, word) in w.into_iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (h, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *h = h.wrapping_add(value);
        }
    }
    let mut digest = [0; 20];
    for (bytes, h) in digest.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&h.to_be_bytes());
    }
    digest
}

/// The first program header of `kind`.
fn header_of<'a>(
    info: &'a SelfInfo,
    kind: u32,
    name: &'static str,
) -> Result<&'a ProgramHeader, PrepareError> {
    info.program_headers
        .iter()
        .find(|p| p.kind == kind)
        .ok_or(PrepareError::MissingSegment(name))
}

fn tag(tags: &HashMap<u64, u64>, key: u64) -> Result<u64, PrepareError> {
    tags.get(&key).copied().ok_or(PrepareError::MissingTag(key))
}

/// The table of the library blob that the dynamic tags `offset_tag` and `size_tag` locate.
pub(super) fn table<'a>(
    blob: &'a [u8],
    tags: &HashMap<u64, u64>,
    offset_tag: u64,
    size_tag: u64,
) -> Result<&'a [u8], PrepareError> {
    let offset = tag(tags, offset_tag)?;
    span(blob, offset, tag(tags, size_tag)?)
}

/// The NUL-terminated ASCII name at `offset` of the string table.
pub(super) fn name_at(strings: &[u8], offset: u64) -> Result<String, PrepareError> {
    span(strings, offset, 1)?;
    let bad = || PrepareError::BadName(offset);
    let tail = usize::try_from(offset)
        .ok()
        .and_then(|start| strings.get(start..))
        .ok_or_else(bad)?;
    let len = tail.iter().position(|&b| b == 0).ok_or_else(bad)?;
    let name = tail
        .get(..len)
        .filter(|name| name.is_ascii())
        .ok_or_else(bad)?;
    String::from_utf8(name.to_vec()).map_err(|_| bad())
}

/// The `Elf64_Sym` (`<IBBHQQ`) at `pos` of the symbol table, with its name.
fn symbol_at(syms: &[u8], strings: &[u8], pos: u64) -> Result<Symbol, PrepareError> {
    let record = span(syms, pos, SYMBOL_SIZE)?;
    let name = u32_at(record, 0)?;
    let info = u8_at(record, 4)?;
    let section = u16_at(record, 6)?;
    let value = u64_at(record, 8)?;
    let size = u64_at(record, 16)?;
    Ok(Symbol {
        name: name_at(strings, u64::from(name))?,
        kind: info & 15,
        section,
        value,
        size,
    })
}

/// Whether `size` bytes at `address` lie inside one loadable segment.
pub(super) fn mapped(loads: &[Segment], address: u64, size: u64) -> bool {
    loads.iter().any(|p| {
        p.vaddr <= address
            && u128::from(address) + u128::from(size) <= u128::from(p.vaddr) + u128::from(p.memsz)
    })
}

/// Proves that this dump's `_init_env` is exactly `RET`; never assumes it.
pub fn inspect_libc(path: &Path) -> Result<LibcReport, PrepareError> {
    let source = read_self_bytes(path)?;
    let info = parse_self(&source)?;
    let elf = &info.elf;
    let dynamic = header_of(&info, PT_DYNAMIC, "dynamic")?;
    let mut tags = HashMap::new();
    for pos in (dynamic.offset..dynamic.offset.saturating_add(dynamic.filesz)).step_by(16) {
        let record = span(elf, pos, 16)?;
        tags.insert(u64_at(record, 0)?, u64_at(record, 8)?);
    }
    let library = header_of(&info, PT_SCE_DYNLIBDATA, "SCE dynlib data")?;
    let blob = span(elf, library.offset, library.filesz)?;
    let strings = table(blob, &tags, DT_SCE_STRTAB, DT_SCE_STRSZ)?;
    let syms = table(blob, &tags, DT_SCE_SYMTAB, DT_SCE_SYMTABSZ)?;
    let mut report = LibcReport {
        sha256: hex(&Sha256::digest(&source)),
        init_env_is_ret: false,
        init_env: None,
        ios_base_init: None,
        ios_base_init_first: false,
    };
    let init_env = nid("_init_env");
    let ios_base_init = nid(IOS_BASE_INIT);
    for pos in (0..syms.len() as u64).step_by(SYMBOL_SIZE as usize) {
        let symbol = symbol_at(syms, strings, pos)?;
        let key = nid_part(&symbol.name);
        if (key != init_env && key != ios_base_init)
            || symbol.section == 0
            || symbol.kind != STT_FUNC
        {
            continue;
        }
        let code_segment = info
            .program_headers
            .iter()
            .find(|p| {
                p.kind == PT_LOAD
                    && p.flags & PF_X != 0
                    && p.vaddr <= symbol.value
                    && symbol.value - p.vaddr < p.filesz
            })
            .ok_or(PrepareError::MissingSegment("executable"))?;
        let start = code_segment
            .offset
            .saturating_add(symbol.value - code_segment.vaddr);
        let code = span(elf, start, symbol.size)?;
        if key == init_env {
            report.init_env_is_ret = code == [RET];
            report.init_env = Some(InitEnv {
                symbol: symbol.name,
                address: symbol.value,
                size: symbol.size,
                bytes: code.to_vec(),
            });
        } else {
            if report.ios_base_init.is_none() && report.init_env.is_none() {
                report.ios_base_init_first = true;
            }
            report.ios_base_init = Some(IosBaseInit {
                symbol: symbol.name,
                address: symbol.value,
                size: symbol.size,
                code_sha256: hex(&Sha256::digest(code)),
                name: IOS_BASE_INIT,
            });
        }
    }
    Ok(report)
}

/// The memory image: every loadable segment's file bytes at its address, the rest zero.
fn memory_image(info: &SelfInfo) -> Result<Vec<u8>, PrepareError> {
    let loads = &info.segments;
    let size = loads
        .iter()
        .map(|p| u128::from(p.vaddr) + u128::from(p.memsz))
        .max()
        .ok_or(PrepareError::MissingSegment("loadable"))?;
    if size > u128::from(IMAGE_LIMIT) || !mapped(loads, info.entry, 1) {
        return Err(PrepareError::InvalidImage);
    }
    let mut image = vec![0; usize::try_from(size).map_err(|_| PrepareError::InvalidImage)?];
    for p in loads {
        if p.filesz > p.memsz {
            return Err(PrepareError::SegmentTooLarge);
        }
        place(&mut image, p.vaddr, span(&info.elf, p.offset, p.filesz)?)?;
    }
    Ok(image)
}

/// The eboot's relocations as `boot.bin` stores them.
struct Relocated {
    relocations: Vec<Relocation>,
    /// By type, in the order the types first appear.
    counts: IndexMap<u32, u64>,
    /// The names of the imported symbols, in the order of their import indexes.
    imports: Vec<String>,
}

/// Reads the eboot's two relocation tables (PLT, then RELA). Absolute symbols are written into
/// `image` here.
fn relocate(
    loads: &[Segment],
    image: &mut [u8],
    blob: &[u8],
    tags: &HashMap<u64, u64>,
    symbols: &[Symbol],
) -> Result<Relocated, PrepareError> {
    let mut relocations = Vec::new();
    let mut counts = IndexMap::new();
    let mut imports = IndexSet::new();
    for (offset_tag, size_tag) in [
        (DT_SCE_JMPREL, DT_SCE_PLTRELSZ),
        (DT_SCE_RELA, DT_SCE_RELASZ),
    ] {
        let entries = table(blob, tags, offset_tag, size_tag)?;
        if !(entries.len() as u64).is_multiple_of(RELOCATION_SIZE) {
            return Err(PrepareError::BadRelocationTable);
        }
        for pos in (0..entries.len() as u64).step_by(RELOCATION_SIZE as usize) {
            let record = span(entries, pos, RELOCATION_SIZE)?;
            let target = u64_at(record, 0)?;
            let info = u64_at(record, 8)?;
            let addend = i64_at(record, 16)?;
            let kind = (info & 0xffff_ffff) as u32;
            let sym = info >> 32;
            *counts.entry(kind).or_insert(0) += 1;
            if !mapped(loads, target, 8) {
                return Err(PrepareError::RelocationOutside(target));
            }
            match kind {
                R_X86_64_RELATIVE => relocations.push(Relocation {
                    target,
                    kind: 0,
                    value: addend,
                    addend: 0,
                }),
                R_X86_64_64 | R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
                    let index =
                        usize::try_from(sym).map_err(|_| PrepareError::BadSymbolIndex(sym))?;
                    let symbol = symbols
                        .get(index)
                        .ok_or(PrepareError::BadSymbolIndex(sym))?;
                    let sum = i128::from(symbol.value) + i128::from(addend);
                    if symbol.section == SHN_ABS {
                        let value =
                            u64::try_from(sum).map_err(|_| PrepareError::RelocationValue)?;
                        place(image, target, &value.to_le_bytes())?;
                    } else if symbol.section != 0 {
                        let value =
                            i64::try_from(sum).map_err(|_| PrepareError::RelocationValue)?;
                        relocations.push(Relocation {
                            target,
                            kind: 0,
                            value,
                            addend: 0,
                        });
                    } else {
                        let (import, _) = imports.insert_full(index);
                        let kind = if symbol.kind == STT_OBJECT { 2 } else { 1 };
                        if (kind == 1 && addend != 0) || !(0..4096).contains(&addend) {
                            return Err(PrepareError::ImportAddend);
                        }
                        relocations.push(Relocation {
                            target,
                            kind,
                            value: import as i64,
                            addend,
                        });
                    }
                }
                _ => return Err(PrepareError::UnsupportedRelocation(kind)),
            }
        }
    }
    let imports = imports
        .iter()
        .filter_map(|&index| symbols.get(index))
        .map(|symbol| symbol.name.clone())
        .collect();
    Ok(Relocated {
        relocations,
        counts,
        imports,
    })
}

/// `boot.bin` (`G3PROBE2`), the small format `probe.c` reads: header, loadable segments, import
/// names, relocations, memory image; little-endian, no host struct packing.
fn boot_image(
    info: &SelfInfo,
    image: &[u8],
    relocated: &Relocated,
    init_env_is_ret: bool,
) -> Result<Vec<u8>, PrepareError> {
    let Relocated {
        relocations,
        imports: names,
        ..
    } = relocated;
    let mut boot = b"G3PROBE2".to_vec();
    let header = [
        image.len() as u64,
        info.entry,
        info.segments.len() as u64,
        relocations.len() as u64,
        names.len() as u64,
        u64::from(init_env_is_ret),
    ];
    for value in header {
        boot.extend_from_slice(&value.to_le_bytes());
    }
    for p in &info.segments {
        for value in [p.vaddr, p.memsz, u64::from(p.flags)] {
            boot.extend_from_slice(&value.to_le_bytes());
        }
    }
    for name in names {
        if name.len() >= IMPORT_NAME_SIZE {
            return Err(PrepareError::ImportNameTooLong);
        }
        let start = boot.len();
        boot.extend_from_slice(name.as_bytes());
        boot.resize(start + IMPORT_NAME_SIZE, 0);
    }
    for r in relocations {
        boot.extend_from_slice(&r.target.to_le_bytes());
        boot.extend_from_slice(&r.kind.to_le_bytes());
        boot.extend_from_slice(&r.value.to_le_bytes());
        boot.extend_from_slice(&r.addend.to_le_bytes());
    }
    boot.extend_from_slice(image);
    Ok(boot)
}

/// The files under `root` by their first path component, and their total size. The files are
/// those `Path.rglob('*')` yields, in the order Python 3.14 yields them, which sets the key order:
/// the entries of `root`, then, popping folders off a stack, the entries of each folder's
/// subfolders. A link to a folder is not followed.
fn resources(root: &Path) -> (IndexMap<String, u64>, u64) {
    let mut found = listing(root);
    let mut stack = vec![root.to_path_buf()];
    while let Some(folder) = stack.pop() {
        for (path, is_dir) in listing(&folder) {
            if is_dir {
                found.extend(listing(&path));
                stack.push(path);
            }
        }
    }
    let mut counts = IndexMap::new();
    let mut total: u64 = 0;
    for (path, _) in found {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let Some(top) = path
            .strip_prefix(root)
            .ok()
            .and_then(|relative| relative.components().next())
        else {
            continue;
        };
        *counts
            .entry(top.as_os_str().to_string_lossy().into_owned())
            .or_insert(0) += 1;
        total = total.saturating_add(metadata.len());
    }
    (counts, total)
}

/// The entries of `folder`, each with whether it is a folder (not following links); none when the
/// folder cannot be listed.
fn listing(folder: &Path) -> Vec<(PathBuf, bool)> {
    fs::read_dir(folder)
        .and_then(|entries| {
            entries
                .map(|entry| {
                    let entry = entry?;
                    let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
                    Ok((entry.path(), is_dir))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), PrepareError> {
    atomic_write(path, bytes).map_err(io_error(path))
}

/// Writes `report` as UTF-8 JSON the way the Python's `json.dumps(indent=2, ensure_ascii=False)`
/// does, plus a newline.
fn write_report(path: &Path, report: &impl Serialize) -> Result<(), PrepareError> {
    let mut text = serde_json::to_string_pretty(report)
        .map_err(|error| io_error(path)(io::Error::other(error)))?;
    text.push('\n');
    write(path, text.as_bytes())
}

/// Turns `<game>/eboot.bin` into `boot.bin`, `eboot.elf`, `entry.bin` and `analysis.json` in `out`.
pub fn prepare(game: &Path, out: &Path) -> Result<Report, PrepareError> {
    let source = read_self_bytes(&game.join("eboot.bin"))?;
    let info = parse_self(&source)?;
    let mut image = memory_image(&info)?;
    let dynamic = header_of(&info, PT_DYNAMIC, "dynamic")?;
    let mut dynamic_entries = Vec::new();
    for pos in (dynamic.offset..dynamic.offset.saturating_add(dynamic.filesz)).step_by(16) {
        let record = span(&info.elf, pos, 16)?;
        let tag = u64_at(record, 0)?;
        if tag == DT_NULL {
            break;
        }
        dynamic_entries.push((tag, u64_at(record, 8)?));
    }
    let tags: HashMap<u64, u64> = dynamic_entries.iter().copied().collect();
    let library = header_of(&info, PT_SCE_DYNLIBDATA, "SCE dynlib data")?;
    let blob = span(&info.elf, library.offset, library.filesz)?;
    let strings = table(blob, &tags, DT_SCE_STRTAB, DT_SCE_STRSZ)?;
    let syms = table(blob, &tags, DT_SCE_SYMTAB, DT_SCE_SYMTABSZ)?;
    if !(syms.len() as u64).is_multiple_of(SYMBOL_SIZE) {
        return Err(PrepareError::BadSymbolTable);
    }
    let symbols = (0..syms.len() as u64)
        .step_by(SYMBOL_SIZE as usize)
        .map(|pos| symbol_at(syms, strings, pos))
        .collect::<Result<Vec<_>, _>>()?;
    let relocated = relocate(&info.segments, &mut image, blob, &tags, &symbols)?;
    let known: HashMap<String, &'static str> = KNOWN_IMPORTS
        .iter()
        .map(|&name| (nid(name), name))
        .collect();
    let mut hints = IndexMap::new();
    for name in &relocated.imports {
        if let Some(&hint) = known.get(nid_part(name)) {
            hints.insert(name.clone(), hint);
        }
    }
    let libc = inspect_libc(&game.join("sce_module").join("libc.prx"))?;
    let boot = boot_image(&info, &image, &relocated, libc.init_env_is_ret)?;
    write(&out.join("boot.bin"), &boot)?;
    write(&out.join("eboot.elf"), &info.elf)?;
    write(&out.join("entry.bin"), image.get(..1024).unwrap_or(&image))?;
    let (resources, resource_bytes) = resources(&game.join("dvdroot_ps4"));
    let sfo_path = game.join("sce_sys").join("param.sfo");
    let sfo = load_sfo(&sfo_path).map_err(|error| match error {
        SfoReadError::Io(source) => io_error(&sfo_path)(source),
        SfoReadError::Invalid(error) => PrepareError::Sfo(error),
    })?;
    let needed = dynamic_entries
        .iter()
        .filter(|&&(tag, _)| tag == DT_NEEDED)
        .map(|&(_, offset)| name_at(strings, offset))
        .collect::<Result<Vec<_>, _>>()?;
    let modules = game.join("sce_module");
    let mut bundled_modules = fs::read_dir(&modules)
        .and_then(|entries| {
            entries
                .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
                .collect::<io::Result<Vec<_>>>()
        })
        .map_err(io_error(&modules))?;
    bundled_modules.sort();
    let report = Report {
        source_sha256: hex(&Sha256::digest(&source)),
        source_bytes: source.len() as u64,
        sfo,
        entry: info.entry,
        image_bytes: image.len() as u64,
        program_headers: info.program_headers,
        self_segments: info.self_segments,
        unavailable_metadata_headers: info.missing,
        needed,
        relocation_counts: relocated.counts,
        import_count: relocated.imports.len(),
        imports: relocated.imports,
        import_name_hints: hints,
        libc_evidence: libc,
        bundled_modules,
        resources,
        resource_bytes,
        status: STATUS,
    };
    write_report(&out.join("analysis.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::sfo::{load_sfo, parse_sfo, read_sfo, SfoReadError};

    /// Little-endian fields as `struct.pack('<…')` lays them out: `(value, width in bytes)` each.
    fn pack(fields: &[(u64, usize)]) -> Vec<u8> {
        fields
            .iter()
            .flat_map(|&(value, width)| value.to_le_bytes()[..width].to_vec())
            .collect()
    }

    fn put(data: &mut [u8], at: usize, bytes: &[u8]) {
        data[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// `<16sHHIQQQIHHHHHH`: an x86-64 ELF header whose program headers follow it at offset 64.
    fn elf_header(e_type: u64, phnum: u64) -> Vec<u8> {
        let mut header = b"\x7fELF\x02\x01\x01\x09".to_vec();
        header.resize(16, 0);
        header.extend(pack(&[
            (e_type, 2),
            (62, 2),
            (1, 4),
            (0, 8),
            (64, 8),
            (0, 8),
            (0, 4),
            (64, 2),
            (56, 2),
            (phnum, 2),
            (0, 2),
            (0, 2),
            (0, 2),
        ]));
        header
    }

    /// `<IIQQQQQQ`: type, flags, offset, vaddr, paddr, filesz, memsz, align.
    fn program_header([kind, flags, rest @ ..]: [u64; 8]) -> Vec<u8> {
        let mut header = pack(&[(kind, 4), (flags, 4)]);
        for value in rest {
            header.extend(value.to_le_bytes());
        }
        header
    }

    /// `<QQQQ`: a SELF segment entry (flags, file offset, size, memory size).
    fn self_entry(fields: [u64; 4]) -> Vec<u8> {
        fields
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    /// `tests/test_prepare.py::fixture`: a SELF whose one plaintext segment lands at file offset
    /// 0x1000 of the ELF while its PT_LOAD maps it at address 0.
    fn synthetic_self() -> Vec<u8> {
        let mut data = vec![0; 0x210];
        put(&mut data, 0, b"O\x15=\x1d");
        put(&mut data, 24, &pack(&[(1, 2)]));
        put(&mut data, 32, &self_entry([0x800, 0x200, 16, 16]));
        put(&mut data, 64, &elf_header(0xfe10, 1));
        put(
            &mut data,
            128,
            &program_header([1, 5, 0x1000, 0, 0, 16, 32, 0x1000]),
        );
        put(&mut data, 0x200, &(0..16).collect::<Vec<u8>>());
        data
    }

    /// The libc of `test_libc_ret_contract_is_verified_from_symbol_and_code`: `_init_env` at
    /// address 0 is `instruction`, found through the dynamic tags and the symbol table.
    fn synthetic_libc(instruction: u8) -> Vec<u8> {
        let mut data = vec![0; 0x320];
        put(&mut data, 0, b"O\x15=\x1d");
        put(&mut data, 24, &pack(&[(2, 2)]));
        put(&mut data, 32, &self_entry([0x800, 0x200, 16, 16]));
        put(&mut data, 64, &self_entry([0x100800, 0x220, 256, 256]));
        put(&mut data, 96, &elf_header(0xfe18, 3));
        put(
            &mut data,
            160,
            &program_header([1, 5, 0x1000, 0, 0, 16, 16, 0x1000]),
        );
        put(
            &mut data,
            216,
            &program_header([0x6100_0000, 4, 0x2000, 0, 0, 256, 0, 16]),
        );
        put(
            &mut data,
            272,
            &program_header([2, 4, 0x20b0, 0, 0, 80, 80, 8]),
        );
        data[0x200] = instruction;
        put(&mut data, 0x220, b"bzQExy189ZI#C#A\0");
        put(
            &mut data,
            0x260,
            &pack(&[(0, 4), (0x12, 1), (0, 1), (1, 2), (0, 8), (1, 8)]),
        );
        let tags = [
            (0x6100_0035, 0),
            (0x6100_0037, 32),
            (0x6100_0039, 64),
            (0x6100_003f, 24),
            (0, 0),
        ];
        for (i, (tag, value)) in tags.into_iter().enumerate() {
            put(&mut data, 0x2d0 + 16 * i, &pack(&[(tag, 8), (value, 8)]));
        }
        data
    }

    /// `result` must be an error whose message contains `text` (the Python's `assertRaisesRegex`).
    #[track_caller]
    fn assert_fails<T: std::fmt::Debug>(result: Result<T, PrepareError>, text: &str) {
        let message = result.unwrap_err().to_string();
        assert!(message.contains(text), "{message:?} lacks {text:?}");
    }

    #[test]
    fn unpacked_elf_has_the_same_headers_and_payload_as_self() {
        let expected = parse_self(&synthetic_self()).unwrap();
        let actual = parse_self(&expected.elf).unwrap();
        assert_eq!(actual.elf, expected.elf);
        assert_eq!(actual.entry, expected.entry);
        assert_eq!(actual.program_headers, expected.program_headers);
        assert_eq!(actual.segments, expected.segments);
        assert_eq!(actual.self_segments, []);
        assert_eq!(actual.missing, Vec::<usize>::new());
    }

    #[test]
    fn unpacked_elf_rejects_truncated_segments() {
        let elf = parse_self(&synthetic_self()).unwrap().elf;
        assert_fails(parse_self(&elf[..elf.len() - 1]), "out of bounds");
    }

    #[test]
    fn unpacked_elf_rejects_other_architectures_and_byte_orders() {
        let elf = parse_self(&synthetic_self()).unwrap().elf;
        for (offset, value) in [(4, 1), (5, 2), (18, 3)] {
            let mut bad = elf.clone();
            bad[offset] = value;
            assert_fails(parse_self(&bad), "little-endian x86-64 ELF");
        }
    }

    #[test]
    fn unpacked_elf_rejects_truncated_program_headers() {
        let elf = parse_self(&synthetic_self()).unwrap().elf;
        assert_fails(parse_self(&elf[..100]), "out of bounds");
    }

    #[test]
    fn unpacked_elf_checks_image_limit_before_allocating() {
        let mut elf = parse_self(&synthetic_self()).unwrap().elf;
        put(&mut elf, 64 + 8, &(512u64 * 1024 * 1024).to_le_bytes());
        assert_fails(parse_self(&elf), "512 MiB");
    }

    #[test]
    fn libc_ret_contract_is_verified_from_symbol_and_code() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("libc.prx");
        for unpacked in [false, true] {
            let write = |instruction| {
                let source = synthetic_libc(instruction);
                let bytes = if unpacked {
                    parse_self(&source).unwrap().elf
                } else {
                    source
                };
                std::fs::write(&path, bytes).unwrap();
            };
            write(0xc3);
            let proof = inspect_libc(&path).unwrap();
            assert!(proof.init_env_is_ret, "unpacked={unpacked}");
            assert_eq!(proof.init_env.unwrap().bytes, [0xc3]);
            write(0x90);
            assert!(
                !inspect_libc(&path).unwrap().init_env_is_ret,
                "unpacked={unpacked}"
            );
        }
        assert_eq!(nid("_init_env"), "bzQExy189ZI");
    }

    #[test]
    fn report_with_non_latin_title_is_written_as_utf8() {
        let title = "ゴッド・オブ・ウォーIII リマスタード";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("analysis.json");
        write_report(&path, &serde_json::json!({"sfo": {"TITLE": title}})).unwrap();
        let text = String::from_utf8(std::fs::read(&path).unwrap()).unwrap();
        assert!(text.contains(title), "{text}");
    }

    #[test]
    fn file_offset_is_not_virtual_address() {
        let info = parse_self(&synthetic_self()).unwrap();
        assert_eq!(info.elf[0x1000..0x1010], (0..16).collect::<Vec<u8>>());
        assert_eq!(info.program_headers[0].vaddr, 0);
        assert_eq!(info.missing, Vec::<usize>::new());
    }

    #[test]
    fn truncated_payload_rejected() {
        let data = synthetic_self();
        assert_fails(parse_self(&data[..data.len() - 1]), "out of bounds");
    }

    #[test]
    fn encrypted_or_compressed_payload_rejected() {
        for flag in [2u64, 8] {
            let mut data = synthetic_self();
            put(&mut data, 32, &(0x800 | flag).to_le_bytes());
            assert_fails(parse_self(&data), "encrypted/compressed");
        }
    }

    #[test]
    fn missing_required_segment_rejected() {
        let mut data = synthetic_self();
        put(&mut data, 32, &0u64.to_le_bytes());
        assert_fails(parse_self(&data), "required segment");
    }

    #[test]
    fn bad_program_header_index_rejected() {
        let mut data = synthetic_self();
        put(&mut data, 32, &0x10_0800u64.to_le_bytes());
        assert_fails(parse_self(&data), "segment index");
    }

    #[test]
    fn empty_game_files_are_named() {
        // An empty eboot.bin (a broken copy, or a placeholder made by hand) failed as
        // "out of bounds: 0x0+0x4 / 0x0", which names neither the file nor the cause.
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        std::fs::write(game.join("eboot.bin"), b"").unwrap();
        assert_fails(prepare(game, &game.join("out")), "eboot.bin is empty");
        // Every SELF the port loads goes through read_self, so a prx is named the same way.
        std::fs::write(game.join("libc.prx"), b"").unwrap();
        assert_fails(inspect_libc(&game.join("libc.prx")), "libc.prx is empty");
        assert_fails(read_self(&game.join("libc.prx")), "libc.prx is empty");
        // load_sfo tells a file that cannot be read from one that is not a param.sfo; read_sfo
        // answers None for both instead of failing.
        std::fs::write(game.join("param.sfo"), b"x").unwrap();
        assert!(matches!(
            load_sfo(&game.join("param.sfo")),
            Err(SfoReadError::Invalid(_))
        ));
        assert!(matches!(
            load_sfo(&game.join("none.sfo")),
            Err(SfoReadError::Io(_))
        ));
        assert_eq!(read_sfo(&game.join("param.sfo")), None);
        assert_eq!(read_sfo(&game.join("none.sfo")), None);
        assert_eq!(read_sfo(game), None);
        let message = parse_sfo(b"").unwrap_err().to_string();
        assert!(message.contains("param.sfo is empty"), "{message}");
        assert_fails(parse_self(b""), "empty SELF");
    }

    #[test]
    fn sha1_matches_the_standard_vectors() {
        // `nid` hashes with a SHA-1 of its own; these are the FIPS 180 examples, the last one
        // padded into a second block.
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }
}
