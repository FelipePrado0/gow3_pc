//! The ISA rewrite table (Python `scripts/isa_rewrite.py`): finds the instructions a CPU before
//! Haswell may lack (MOVBE, LZCNT, BMI1, F16C) in the linked image and writes `isa-rewrite.bin`
//! (`G3ISA1`) for the C loader (`src/isa_rewrite.c`).
//!
//! `scan_segment` decodes with the `iced_x86` crate, the engine and version of the Python's
//! binding, so both find the same sites. The table is a 64-byte header (magic, image size, SHA-256
//! of the image, site count, unsupported count) and one 64-byte record per site, sorted by offset:
//! the loader rejects a table of another image or with records out of order.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use iced_x86::{
    Code, Decoder, DecoderOptions, FlowControl, Instruction, Mnemonic, OpKind, Register,
};
use sha2::{Digest, Sha256};

use super::prepare::{span, u64_at, PrepareError, Segment, IMPORT_NAME_SIZE, PF_X};
use crate::core::fs::atomic_write;

/// What a register slot holds when its operand is not a register.
pub const NO_REG: u8 = 0xFF;
const MAGIC: &[u8; 8] = b"G3ISA1\0\0";
const HEADER_SIZE: usize = 64;
const RECORD_SIZE: usize = 64;
/// The longest x86 instruction: the size of a record's `original` and `relocated` fields.
const MAX_INSTRUCTION: usize = 15;
const LINKED_MAGIC: &[u8] = b"G3PROBE5";
/// The `G3PROBE5` header (`<8s6Q`), procparam and main TLS words, then the module count.
const MODULE_COUNT_AT: u64 = 56 + 8 + 32;
/// The sizes of a `G3PROBE5`'s module, binding, segment and relocation records.
const MODULE_SIZE: u64 = 56;
const BINDING_SIZE: u64 = 24;
const SEGMENT_SIZE: u64 = 24;
const RELOCATION_SIZE: u64 = 32;
const FILE_NAME: &str = "isa-rewrite.bin";

/// The BMI1 instructions with a kind of their own, past TZCNT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bmi1Op {
    Andn,
    Bextr,
    Blsi,
    Blsmsk,
    Blsr,
}

/// The F16C conversions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum F16cOp {
    Vcvtph2ps,
    Vcvtps2ph,
}

/// What a site's instruction is (the Python's `KIND_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `movbe reg, [mem]`.
    MovbeLoad,
    /// `movbe [mem], reg`.
    MovbeStore,
    Lzcnt,
    Tzcnt,
    Bmi1(Bmi1Op),
    F16c(F16cOp),
}

impl Kind {
    /// The Python's `KIND_*` number, as the record stores it (`ISA_*` of `src/isa_emulate.h`).
    pub const fn as_u8(self) -> u8 {
        match self {
            Kind::MovbeLoad => 1,
            Kind::MovbeStore => 2,
            Kind::Lzcnt => 3,
            Kind::Tzcnt => 4,
            Kind::Bmi1(Bmi1Op::Andn) => 5,
            Kind::Bmi1(Bmi1Op::Bextr) => 6,
            Kind::Bmi1(Bmi1Op::Blsi) => 7,
            Kind::Bmi1(Bmi1Op::Blsmsk) => 8,
            Kind::Bmi1(Bmi1Op::Blsr) => 9,
            Kind::F16c(F16cOp::Vcvtph2ps) => 10,
            Kind::F16c(F16cOp::Vcvtps2ph) => 11,
        }
    }

    /// The instruction set the summary counts it under (the Python's `SET_OF_KIND`).
    fn set(self) -> &'static str {
        match self {
            Kind::MovbeLoad | Kind::MovbeStore => "MOVBE",
            Kind::Lzcnt => "LZCNT",
            Kind::Tzcnt | Kind::Bmi1(_) => "BMI1",
            Kind::F16c(_) => "F16C",
        }
    }
}

/// How the loader patches a site (the Python's `MODE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    /// 5 bytes or more: a jump to the site's stub.
    Jmp = 0,
    /// 4 bytes: the jump also covers the plain next instruction, copied into the stub.
    JmpRelocate = 1,
    /// 4 bytes: trapped at run time.
    Int3 = 2,
    /// An encoding the stubs do not emulate: segment override, LOCK, 32-bit addressing or F16C.
    Unsupported = 3,
}

/// One instruction to rewrite (the Python's `Site`, one `IsaSite` record of the table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// The instruction's address in the image.
    pub offset: u64,
    pub length: u8,
    pub kind: Kind,
    /// The operand size in bits (the first register operand's); 0 for F16C.
    pub width: u8,
    pub mode: Mode,
    /// The operands' register numbers (x86 numbering), in order; `NO_REG` for the memory operand,
    /// an immediate or no operand.
    pub dst: u8,
    pub src1: u8,
    pub src2: u8,
    /// 0 without a memory operand, 1 when it is the destination, 2 when it is a source.
    pub mem_slot: u8,
    /// The memory operand's registers (`NO_REG` when absent or RIP-relative) and index scale.
    pub base: u8,
    pub index: u8,
    pub scale: u8,
    /// Bit 0: RIP-relative, `disp` is the absolute target.
    pub mem_flags: u8,
    pub disp: i64,
    /// The instruction's `length` bytes, zero-padded.
    pub original: [u8; MAX_INSTRUCTION],
    /// For `JmpRelocate`, the next instruction's bytes, which the stub runs; otherwise empty.
    pub relocated: Vec<u8>,
}

/// Why the table could not be made (the Python prints `ISA: rewrite table unavailable (…)`).
#[derive(Debug, thiserror::Error)]
pub enum IsaError {
    #[error(transparent)]
    Prepare(#[from] PrepareError),
    #[error("not a G3PROBE5 image")]
    NotLinked,
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// A value a record stores in one byte. Every caller's value fits (instruction lengths up to 15,
/// widths up to 64, register numbers below 32, scales up to 8): the saturation is never reached.
fn byte(value: impl TryInto<u8>) -> u8 {
    value.try_into().unwrap_or(u8::MAX)
}

/// `bytes` in a record's 15-byte field, zero-padded.
fn field(bytes: &[u8]) -> [u8; MAX_INSTRUCTION] {
    let mut field = [0; MAX_INSTRUCTION];
    for (to, from) in field.iter_mut().zip(bytes) {
        *to = *from;
    }
    field
}

fn kind_of(ins: &Instruction) -> Option<Kind> {
    Some(match ins.mnemonic() {
        Mnemonic::Movbe if ins.op0_kind() == OpKind::Register => Kind::MovbeLoad,
        Mnemonic::Movbe => Kind::MovbeStore,
        Mnemonic::Lzcnt => Kind::Lzcnt,
        Mnemonic::Tzcnt => Kind::Tzcnt,
        Mnemonic::Andn => Kind::Bmi1(Bmi1Op::Andn),
        Mnemonic::Bextr => Kind::Bmi1(Bmi1Op::Bextr),
        Mnemonic::Blsi => Kind::Bmi1(Bmi1Op::Blsi),
        Mnemonic::Blsmsk => Kind::Bmi1(Bmi1Op::Blsmsk),
        Mnemonic::Blsr => Kind::Bmi1(Bmi1Op::Blsr),
        Mnemonic::Vcvtph2ps => Kind::F16c(F16cOp::Vcvtph2ps),
        Mnemonic::Vcvtps2ph => Kind::F16c(F16cOp::Vcvtps2ph),
        _ => return None,
    })
}

fn register_number(register: Register) -> u8 {
    if register == Register::None {
        NO_REG
    } else {
        byte(register.number())
    }
}

/// The site of `ins`. Operands in order fill `dst`, `src1`, `src2`; the memory operand leaves its
/// slot `NO_REG`.
fn site_of(ins: &Instruction, kind: Kind, original: &[u8]) -> Site {
    let mut registers = [NO_REG; 3];
    let mut mem_slot = 0;
    let mut width = 0;
    for operand in 0..ins.op_count() {
        match ins.op_kind(operand) {
            OpKind::Register => {
                let register = ins.op_register(operand);
                // The decoder gives these kinds three operands at most.
                if let Some(slot) = usize::try_from(operand)
                    .ok()
                    .and_then(|at| registers.get_mut(at))
                {
                    *slot = register_number(register);
                }
                if width == 0 {
                    width = register.size() * 8;
                }
            }
            OpKind::Memory => mem_slot = if operand == 0 { 1 } else { 2 },
            _ => {}
        }
    }
    let f16c = matches!(kind, Kind::F16c(_));
    let [dst, src1, src2] = registers;
    let mut site = Site {
        offset: ins.ip(),
        length: byte(ins.len()),
        kind,
        width: if f16c { 0 } else { byte(width) },
        mode: Mode::Jmp,
        dst,
        src1,
        src2,
        mem_slot,
        base: NO_REG,
        index: NO_REG,
        scale: 0,
        mem_flags: 0,
        disp: 0,
        original: field(original),
        relocated: Vec::new(),
    };
    let mut unsupported = f16c || ins.segment_prefix() != Register::None || ins.has_lock_prefix();
    if mem_slot != 0 {
        unsupported |= [ins.memory_base(), ins.memory_index()]
            .iter()
            .any(|&register| register != Register::None && register.size() != 8);
        site.scale = byte(ins.memory_index_scale());
        // The table stores the displacement and the absolute target as `int64`; the decoder
        // reports them as `u64`.
        if ins.is_ip_rel_memory_operand() {
            site.mem_flags = 1;
            site.disp = ins.ip_rel_memory_address() as i64;
        } else {
            site.base = register_number(ins.memory_base());
            site.index = register_number(ins.memory_index());
            site.disp = ins.memory_displacement64() as i64;
        }
    }
    site.mode = if unsupported {
        Mode::Unsupported
    } else if ins.len() >= 5 {
        Mode::Jmp
    } else {
        Mode::Int3
    };
    site
}

/// Whether `ins` may be moved into a 4-byte site's stub (branch targets are checked later).
fn is_plain(ins: &Instruction, end: u64) -> bool {
    ins.code() != Code::INVALID
        && ins.flow_control() == FlowControl::Next
        && !ins.is_ip_rel_memory_operand()
        && !ins.has_rep_prefix()
        && !ins.has_repne_prefix()
        && kind_of(ins).is_none()
        && ins.next_ip() <= end
}

/// The sites of one executable segment loaded at `address`, in address order.
pub fn scan_segment(code: &[u8], address: u64) -> Vec<Site> {
    // Fails only for a bitness other than 16, 32 or 64, or a slice that ends at the top of the
    // address space.
    let Ok(mut decoder) = Decoder::try_with_ip(64, code, address, DecoderOptions::NONE) else {
        return Vec::new();
    };
    let end = address.saturating_add(code.len() as u64);
    let mut sites: Vec<Site> = Vec::new();
    let mut targets = HashSet::new();
    // (site, address and bytes of the plain instruction after it)
    let mut candidates = Vec::new();
    // A 4-byte site waiting for the instruction after it.
    let mut pending = None;
    while decoder.can_decode() {
        let start = decoder.position();
        let ins = decoder.decode();
        let bytes = code.get(start..decoder.position()).unwrap_or_default();
        if matches!(
            ins.flow_control(),
            FlowControl::UnconditionalBranch | FlowControl::ConditionalBranch | FlowControl::Call
        ) && ins.op0_kind() == OpKind::NearBranch64
        {
            targets.insert(ins.near_branch_target());
        }
        if let Some(site) = pending.take() {
            if is_plain(&ins, end) {
                candidates.push((site, ins.ip(), bytes.to_vec()));
            }
        }
        let Some(kind) = kind_of(&ins) else {
            continue;
        };
        let site = site_of(&ins, kind, bytes);
        if site.mode == Mode::Int3 && ins.len() == 4 {
            pending = Some(sites.len());
        }
        sites.push(site);
    }
    for (site, ip, relocated) in candidates {
        if let Some(site) = sites.get_mut(site).filter(|_| !targets.contains(&ip)) {
            site.mode = Mode::JmpRelocate;
            site.relocated = relocated;
        }
    }
    sites
}

/// `pos + count * size`, or the out-of-bounds error the Python's `span` gives that far.
fn skip(data: &[u8], pos: u64, count: u64, size: u64) -> Result<u64, PrepareError> {
    count
        .checked_mul(size)
        .and_then(|bytes| pos.checked_add(bytes))
        .ok_or(PrepareError::Truncated {
            offset: pos,
            size: count.saturating_mul(size),
            len: data.len() as u64,
        })
}

/// The segments and memory image of a `G3PROBE5` file, in `link_modules`' order. A segment's
/// bytes sit at its address of the image, so `offset` is `vaddr` and `filesz` is `memsz`.
pub fn read_boot_linked(data: &[u8]) -> Result<(Vec<Segment>, Vec<u8>), IsaError> {
    let header = span(data, 0, 56)?;
    if span(header, 0, 8)? != LINKED_MAGIC {
        return Err(IsaError::NotLinked);
    }
    let size = u64_at(header, 8)?;
    let segment_count = u64_at(header, 24)?;
    let relocation_count = u64_at(header, 32)?;
    let name_count = u64_at(header, 40)?;
    let modules = u64_at(data, MODULE_COUNT_AT)?;
    let pos = skip(data, MODULE_COUNT_AT + 8, modules, MODULE_SIZE)?;
    let bindings = u64_at(data, pos)?;
    // `u64_at` read the 8 bytes at `pos`, so `pos + 8` fits.
    let pos = skip(data, pos + 8, bindings, BINDING_SIZE)?;
    let mut segments = Vec::new();
    // Stops at the first record past the data, as the Python does.
    for index in 0..segment_count {
        let record = span(data, skip(data, pos, index, SEGMENT_SIZE)?, SEGMENT_SIZE)?;
        let address = u64_at(record, 0)?;
        let size = u64_at(record, 8)?;
        segments.push(Segment {
            offset: address,
            vaddr: address,
            filesz: size,
            memsz: size,
            // The ELF's 32-bit `p_flags` in a 64-bit word; the scan reads only `PF_X`.
            flags: u64_at(record, 16)? as u32,
        });
    }
    let pos = skip(data, pos, segment_count, SEGMENT_SIZE)?;
    let pos = skip(data, pos, name_count, IMPORT_NAME_SIZE as u64)?;
    let pos = skip(data, pos, relocation_count, RELOCATION_SIZE)?;
    Ok((segments, span(data, pos, size)?.to_vec()))
}

/// `isa-rewrite.bin` for `sites` of `image`.
pub fn encode_table(image: &[u8], sites: &[Site]) -> Vec<u8> {
    // A site takes 4 bytes at least: 2^32 of them would need 16 GiB of code.
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let unsupported = sites.iter().filter(|s| s.mode == Mode::Unsupported).count();
    let mut table = Vec::with_capacity(HEADER_SIZE + RECORD_SIZE * sites.len());
    table.extend_from_slice(MAGIC);
    table.extend_from_slice(&(image.len() as u64).to_le_bytes());
    table.extend_from_slice(&Sha256::digest(image));
    table.extend_from_slice(&count(sites.len()).to_le_bytes());
    table.extend_from_slice(&count(unsupported).to_le_bytes());
    table.extend_from_slice(&[0; 8]);
    let mut sorted: Vec<&Site> = sites.iter().collect();
    // The C loader rejects records out of order.
    sorted.sort_by_key(|s| s.offset);
    for s in sorted {
        table.extend_from_slice(&s.offset.to_le_bytes());
        table.extend_from_slice(&[
            s.length,
            s.kind.as_u8(),
            s.width,
            s.mode as u8,
            s.dst,
            s.src1,
            s.src2,
            s.mem_slot,
            s.base,
            s.index,
            s.scale,
            s.mem_flags,
            0,
            0,
            0,
            0,
        ]);
        table.extend_from_slice(&s.disp.to_le_bytes());
        table.extend_from_slice(&s.original);
        table.push(byte(s.relocated.len()));
        table.extend_from_slice(&field(&s.relocated));
        table.push(0);
    }
    table
}

/// The line the Python prints after writing the table.
pub fn summary(sites: &[Site]) -> String {
    let per_set: Vec<String> = ["MOVBE", "BMI1", "LZCNT", "F16C"]
        .iter()
        .map(|&set| {
            let count = sites.iter().filter(|s| s.kind.set() == set).count();
            format!("{set} {count}")
        })
        .collect();
    let modes = |mode| sites.iter().filter(|s| s.mode == mode).count();
    format!(
        "ISA: {} sites ({}); {} relocated, {} int3, {} unsupported",
        sites.len(),
        per_set.join(", "),
        modes(Mode::JmpRelocate),
        modes(Mode::Int3),
        modes(Mode::Unsupported)
    )
}

/// Scans the executable segments of `boot_linked` and writes `<out>/isa-rewrite.bin` (the Python's
/// `main`), even when no site was found: a table of none. Returns the sites, in scan order, for
/// the caller to print `summary`; an error means there is no table to mount.
pub fn rewrite(boot_linked: &Path, out: &Path) -> Result<Vec<Site>, IsaError> {
    let data = fs::read(boot_linked).map_err(|source| IsaError::Io {
        path: boot_linked.to_path_buf(),
        source,
    })?;
    let (segments, image) = read_boot_linked(&data)?;
    let mut sites = Vec::new();
    for segment in segments.iter().filter(|s| s.flags & PF_X != 0) {
        let code = span(&image, segment.vaddr, segment.memsz)?;
        sites.extend(scan_segment(code, segment.vaddr));
    }
    let path = out.join(FILE_NAME);
    atomic_write(&path, &encode_table(&image, &sites))
        .map_err(|source| IsaError::Io { path, source })?;
    Ok(sites)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use sha2::{Digest, Sha256};

    use super::*;

    // The Python test's instructions, as hex.
    const BLSR_ECX: &str = "c4e270f3c9"; // blsr ecx, ecx (5 bytes)
    const MOVBE_STORE: &str = "0f38f108"; // movbe [rax], ecx (4)
    const BEXTR_RIP: &str = "c4e270f7050c000000"; // bextr eax, [rip+0xc], ecx (9)
    const LZCNT_AX: &str = "66f30fbdc1"; // lzcnt ax, cx (5)
    const TZCNT_RAX: &str = "f3480fbcc1"; // tzcnt rax, rcx (5)
    const NOP: &str = "90";
    const RET: &str = "c3";
    const JMP_SELF: &str = "ebfe";
    const MOV_RIP: &str = "8b0500000000"; // mov eax, [rip+0] (6)
    const MOV_FS: &str = "648b042500000000"; // mov eax, fs:[0] (8): not a site
    const VCVTPH2PS: &str = "c4e2791310"; // vcvtph2ps xmm2, [rax]
    const BLSR: Kind = Kind::Bmi1(Bmi1Op::Blsr);

    /// The bytes of the hex strings `parts`, one after the other (`bytes.fromhex`).
    fn code(parts: &[&str]) -> Vec<u8> {
        let text: String = parts.concat();
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
            .collect()
    }

    /// `bytes` as a record's 15-byte field.
    fn padded(bytes: &[u8]) -> [u8; 15] {
        let mut field = [0; 15];
        field[..bytes.len()].copy_from_slice(bytes);
        field
    }

    /// The only site of `sites` (the Python's `s, = …`).
    fn one(sites: Vec<Site>) -> Site {
        assert_eq!(sites.len(), 1, "{sites:?}");
        sites.into_iter().next().unwrap()
    }

    /// The Python's `boot_linked`: a `G3PROBE5` as `link_modules` writes it, `(code, size, flags)`
    /// segments laid out from address 0, no modules, bindings, names or relocations.
    fn boot_linked(segments: &[(&[u8], u64, u64)]) -> Vec<u8> {
        let mut image = Vec::new();
        let mut table = Vec::new();
        for &(code, size, flags) in segments {
            table.push([image.len() as u64, size, flags]);
            let start = image.len();
            image.extend_from_slice(code);
            image.resize(start + (size as usize).max(code.len()), 0);
        }
        let mut data = b"G3PROBE5".to_vec();
        let header = [image.len() as u64, 0, table.len() as u64, 0, 0, 1];
        // Procparam, main TLS (4 words), no modules, no bindings.
        let words = header.into_iter().chain([0; 7]).chain(table.concat());
        for word in words {
            data.extend_from_slice(&word.to_le_bytes());
        }
        data.extend_from_slice(&image);
        data
    }

    /// One record of a table, as the Python's `RECORD.unpack_from` reads it.
    #[derive(Debug)]
    struct Record {
        offset: u64,
        length: u8,
        kind: u8,
        mode: u8,
        mem_slot: u8,
        base: u8,
        disp: i64,
        original: [u8; 15],
        relocated_length: u8,
        relocated: [u8; 15],
    }

    fn record(table: &[u8], at: usize) -> Record {
        let r = &table[at..at + 64];
        Record {
            offset: u64::from_le_bytes(r[0..8].try_into().unwrap()),
            length: r[8],
            kind: r[9],
            mode: r[11],
            mem_slot: r[15],
            base: r[16],
            disp: i64::from_le_bytes(r[24..32].try_into().unwrap()),
            original: r[32..47].try_into().unwrap(),
            relocated_length: r[47],
            relocated: r[48..63].try_into().unwrap(),
        }
    }

    #[track_caller]
    fn assert_fails<T: std::fmt::Debug>(result: Result<T, IsaError>, text: &str) {
        let error = result.unwrap_err().to_string();
        assert!(error.starts_with(text), "{error}");
    }

    // ScanSegmentTests.

    #[test]
    fn blsr_is_a_jmp_site() {
        let s = one(scan_segment(&code(&[BLSR_ECX]), 0x1000));
        assert_eq!(
            (s.offset, s.length, s.kind, s.width, s.mode),
            (0x1000, 5, BLSR, 32, Mode::Jmp)
        );
        assert_eq!((s.dst, s.src1, s.src2, s.mem_slot), (1, 1, NO_REG, 0));
        assert_eq!(s.original, padded(&code(&[BLSR_ECX])));
        assert_eq!(s.relocated, b"");
    }

    #[test]
    fn bextr_rip_relative_memory_source() {
        let s = one(scan_segment(&code(&[BEXTR_RIP]), 0x1000));
        assert_eq!(
            (s.kind, s.width, s.dst, s.src1, s.src2, s.mem_slot),
            (Kind::Bmi1(Bmi1Op::Bextr), 32, 0, NO_REG, 1, 2)
        );
        assert_eq!(
            (s.base, s.index, s.mem_flags, s.disp),
            (NO_REG, NO_REG, 1, 0x1000 + 9 + 0xc)
        );
    }

    #[test]
    fn sixteen_bit_lzcnt_and_sixty_four_bit_tzcnt() {
        let sites = scan_segment(&code(&[LZCNT_AX, TZCNT_RAX]), 0);
        let [a, b] = sites.as_slice() else {
            panic!("{sites:?}")
        };
        assert_eq!((a.kind, a.width, a.length), (Kind::Lzcnt, 16, 5));
        assert_eq!((b.kind, b.width, b.dst, b.src1), (Kind::Tzcnt, 64, 0, 1));
    }

    #[test]
    fn four_byte_store_relocates_a_plain_next_instruction() {
        let s = one(scan_segment(&code(&[MOVBE_STORE, NOP, RET]), 0));
        assert_eq!(
            (s.kind, s.mode, s.mem_slot, s.base, s.dst, s.src1),
            (Kind::MovbeStore, Mode::JmpRelocate, 1, 0, NO_REG, 1)
        );
        assert_eq!(s.relocated, code(&[NOP]));
    }

    #[test]
    fn four_byte_site_falls_back_to_int3() {
        // A branch, a RIP-relative operand, another site, the end of the segment.
        for tail in [JMP_SELF, MOV_RIP, BLSR_ECX, ""] {
            let s = scan_segment(&code(&[MOVBE_STORE, tail]), 0).remove(0);
            assert_eq!(
                (s.mode, s.relocated.as_slice()),
                (Mode::Int3, &[][..]),
                "{tail}"
            );
        }
    }

    #[test]
    fn next_instruction_that_is_a_branch_target_blocks_relocation() {
        let s = one(scan_segment(&code(&["eb04", MOVBE_STORE, NOP, RET]), 0)); // jmp +4 lands on the nop
        assert_eq!(s.mode, Mode::Int3);
    }

    #[test]
    fn unsupported_encodings() {
        for (parts, why) in [
            (["64", BEXTR_RIP], "segment prefix"),
            (["67", MOVBE_STORE], "32-bit address"),
            (["", VCVTPH2PS], "vcvtph2ps xmm2, [rax]"),
        ] {
            let s = one(scan_segment(&code(&[parts[0], parts[1], RET]), 0));
            assert_eq!(s.mode, Mode::Unsupported, "{why}");
        }
        let s = one(scan_segment(&code(&[VCVTPH2PS, RET]), 0));
        assert_eq!(s.kind, Kind::F16c(F16cOp::Vcvtph2ps));
    }

    #[test]
    fn negative_displacement_is_signed() {
        let s = one(scan_segment(&code(&["0f38f048f8"]), 0)); // movbe ecx, [rax-8]
        assert_eq!(
            (s.kind, s.disp, s.base, s.mem_slot),
            (Kind::MovbeLoad, -8, 0, 2)
        );
    }

    #[test]
    fn sib_memory_operand() {
        let s = one(scan_segment(&code(&["c4e2e0f2449108"]), 0)); // andn rax, rbx, [rcx+rdx*4+8]
        assert_eq!(
            (s.kind, s.width, s.dst, s.src1, s.src2, s.mem_slot),
            (Kind::Bmi1(Bmi1Op::Andn), 64, 0, 3, NO_REG, 2)
        );
        assert_eq!(
            (s.base, s.index, s.scale, s.mem_flags, s.disp),
            (1, 2, 4, 0, 8)
        );
    }

    #[test]
    fn non_sites_are_ignored() {
        assert_eq!(scan_segment(&code(&[MOV_FS, RET]), 0), Vec::<Site>::new());
    }

    // ReadBootLinkedTests.

    #[test]
    fn read_boot_linked_returns_segments_and_image() {
        let blsr_ret = code(&[BLSR_ECX, RET]);
        let data = boot_linked(&[(&blsr_ret, 0x1000, 5), (b"data", 0x2000, 6)]);
        let (segments, image) = read_boot_linked(&data).unwrap();
        let triples: Vec<_> = segments
            .iter()
            .map(|s| (s.vaddr, s.memsz, s.flags))
            .collect();
        assert_eq!(triples, [(0, 0x1000, 5), (0x1000, 0x2000, 6)]);
        assert_eq!(image.len(), 0x3000);
        assert_eq!(
            (&image[..6], &image[0x1000..0x1004]),
            (&blsr_ret[..], &b"data"[..])
        );
    }

    #[test]
    fn read_boot_linked_rejects_bad_magic_and_truncation() {
        let data = boot_linked(&[(&code(&[RET]), 0x1000, 5)]);
        let bad = [&b"G3PROBE4"[..], &data[8..]].concat();
        assert_fails(read_boot_linked(&bad), "not a G3PROBE5 image");
        assert_fails(read_boot_linked(&data[..data.len() - 1]), "out of bounds");
    }

    // Not in the Python: sizes and counts whose arithmetic overflows `u64`, and an executable
    // segment past the image, fail instead of panicking.
    #[test]
    fn hostile_counts_and_segments_are_out_of_bounds() {
        let data = boot_linked(&[(&code(&[RET]), 0x1000, 5)]);
        let with_max_at = |at: usize| {
            let mut hostile = data.clone();
            hostile[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
            hostile
        };
        // Image size, segment, relocation and name counts, module and binding counts.
        for at in [8, 24, 32, 40, 96, 104] {
            assert_fails(read_boot_linked(&with_max_at(at)), "out of bounds");
        }
        let dir = tempfile::tempdir().unwrap();
        let boot = dir.path().join("boot-linked.bin");
        fs::write(&boot, with_max_at(112)).unwrap(); // the segment's address
        assert_fails(rewrite(&boot, dir.path()), "out of bounds");
    }

    #[test]
    fn records_are_sixty_four_bytes() {
        assert_eq!(encode_table(&[], &[]).len(), 64);
        let sites = scan_segment(&code(&[BLSR_ECX]), 0);
        assert_eq!(encode_table(&[], &sites).len(), 64 + 64);
    }

    // TableTests.

    #[test]
    fn encode_table_header_and_record() {
        let body = code(&[BLSR_ECX, MOVBE_STORE, NOP, RET]);
        let sites = scan_segment(&body, 0x2000);
        let image = [vec![0; 0x2000], body].concat();
        let table = encode_table(&image, &sites);
        assert_eq!(&table[..8], b"G3ISA1\0\0");
        assert_eq!(&table[8..16], (image.len() as u64).to_le_bytes());
        assert_eq!(&table[16..48], Sha256::digest(&image).as_slice());
        assert_eq!(&table[48..56], [2, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&table[56..64], [0; 8]);
        assert_eq!(table.len(), 64 + 2 * 64);
        let r = record(&table, 64 + 64);
        // KIND_MOVBE_STORE, MODE_JMP_RELOCATE.
        assert_eq!(
            (r.offset, r.length, r.kind, r.mode, r.mem_slot, r.base),
            (0x2005, 4, 2, 1, 1, 0)
        );
        assert_eq!(
            (r.relocated_length, r.relocated),
            (1, padded(&code(&[NOP])))
        );
        assert_eq!(r.original, padded(&code(&[MOVBE_STORE])));
    }

    #[test]
    fn encode_table_keeps_negative_displacement() {
        let sites = scan_segment(&code(&["0f38f048f8", RET]), 0); // movbe ecx, [rax-8]
        assert_eq!(record(&encode_table(&[], &sites), 64).disp, -8);
    }

    #[test]
    fn encode_table_sorts_records_by_offset() {
        let late = one(scan_segment(&code(&[BLSR_ECX]), 0x2000));
        let early = one(scan_segment(&code(&[BLSR_ECX]), 0x1000));
        let table = encode_table(&[0; 0x3000], &[late, early]);
        let offsets: Vec<u64> = (0..2).map(|i| record(&table, 64 + 64 * i).offset).collect();
        assert_eq!(offsets, [0x1000, 0x2000]);
    }

    #[test]
    fn summary_line() {
        let sites = scan_segment(&code(&[BLSR_ECX, MOVBE_STORE, NOP, VCVTPH2PS, RET]), 0);
        assert_eq!(
            summary(&sites),
            "ISA: 3 sites (MOVBE 1, BMI1 1, LZCNT 0, F16C 1); 1 relocated, 0 int3, 1 unsupported"
        );
    }

    #[test]
    fn main_writes_table_for_executable_segments_only() {
        let dir = tempfile::tempdir().unwrap();
        let boot = dir.path().join("boot-linked.bin");
        let (blsr_ret, blsr) = (code(&[BLSR_ECX, RET]), code(&[BLSR_ECX]));
        fs::write(
            &boot,
            boot_linked(&[(&blsr_ret, 0x1000, 5), (b"", 0x1000, 5), (&blsr, 0x1000, 6)]),
        )
        .unwrap();
        let sites = rewrite(&boot, dir.path()).unwrap();
        assert_eq!(sites, scan_segment(&blsr_ret, 0));
        assert_eq!(
            summary(&sites),
            "ISA: 1 sites (MOVBE 0, BMI1 1, LZCNT 0, F16C 0); 0 relocated, 0 int3, 0 unsupported"
        );
        let table = fs::read(dir.path().join("isa-rewrite.bin")).unwrap();
        let image = read_boot_linked(&fs::read(&boot).unwrap()).unwrap().1;
        assert_eq!(table, encode_table(&image, &sites));
    }

    #[test]
    fn main_reports_unavailable_input() {
        let dir = tempfile::tempdir().unwrap();
        let (bad, short) = (dir.path().join("bad.bin"), dir.path().join("short.bin"));
        fs::write(&bad, [&b"G3PROBE4"[..], &[0; 64]].concat()).unwrap();
        let data = boot_linked(&[(&code(&[RET]), 0x1000, 5)]);
        fs::write(&short, &data[..data.len() - 1]).unwrap();
        let missing = dir.path().join("missing.bin");
        assert!(matches!(
            rewrite(&missing, dir.path()),
            Err(IsaError::Io { .. })
        ));
        for (path, reason) in [(&bad, "not a G3PROBE5 image"), (&short, "out of bounds")] {
            assert_fails(rewrite(path, dir.path()), reason);
        }
        assert!(!dir.path().join("isa-rewrite.bin").exists());
    }

    // Not a test of the Python, which behaves the same: an image without sites gets a table of
    // none, and the run mounts it.
    #[test]
    fn rewrite_without_sites_writes_an_empty_table() {
        let dir = tempfile::tempdir().unwrap();
        let boot = dir.path().join("boot-linked.bin");
        fs::write(&boot, boot_linked(&[(&code(&[MOV_FS, RET]), 0x1000, 5)])).unwrap();
        assert_eq!(rewrite(&boot, dir.path()).unwrap(), Vec::<Site>::new());
        let table = fs::read(dir.path().join("isa-rewrite.bin")).unwrap();
        let image = read_boot_linked(&fs::read(&boot).unwrap()).unwrap().1;
        assert_eq!(table.len(), 64);
        assert_eq!(&table[..8], b"G3ISA1\0\0");
        assert_eq!(&table[8..16], (image.len() as u64).to_le_bytes());
        assert_eq!(&table[16..48], Sha256::digest(&image).as_slice());
        assert_eq!(&table[48..64], [0; 16]); // no site, none unsupported, padding
    }
}
