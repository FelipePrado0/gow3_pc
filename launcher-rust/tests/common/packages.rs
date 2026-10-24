//! Synthetic probe inputs for the runner's tests: `tests/test_probe.py::package`, the SELF of
//! `tests/test_link_libc.py::self_fixture`, and unpacked ELF modules with a dynamic table and a
//! library blob, which the Python linker tests replaced with mocks. The `src/runner` tests include
//! this file with `#[path]`.
#![allow(dead_code)]

/// Little-endian `u64`s, as `struct.pack('<…Q')` lays them out.
fn words(values: &[u64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn put(data: &mut [u8], at: usize, bytes: &[u8]) {
    data[at..at + bytes.len()].copy_from_slice(bytes);
}

/// `bytes` NUL-padded to `width`, never cut (`bytes.ljust(width, b'\0')`).
fn ljust(bytes: &[u8], width: usize) -> Vec<u8> {
    let mut padded = bytes.to_vec();
    padded.resize(width.max(bytes.len()), 0);
    padded
}

/// `tests/test_probe.py::package`: a loader image whose one segment maps `code`, padded to 4096
/// bytes, read-execute at address 0. `G3PROBE2` with `capabilities`, else `G3PROBE1`; `relocs` are
/// `(target, kind, value, addend)`.
pub fn package(
    code: &[u8],
    relocs: &[(u64, u64, i64, i64)],
    names: &[&str],
    capabilities: Option<u64>,
) -> Vec<u8> {
    let image = ljust(code, 4096);
    let mut data = match capabilities {
        Some(_) => b"G3PROBE2".to_vec(),
        None => b"G3PROBE1".to_vec(),
    };
    let counts = [relocs.len() as u64, names.len() as u64];
    data.extend(words(&[image.len() as u64, 0, 1, counts[0], counts[1]]));
    if let Some(capabilities) = capabilities {
        data.extend(capabilities.to_le_bytes());
    }
    data.extend(words(&[0, image.len() as u64, 5]));
    for name in names {
        data.extend(ljust(name.as_bytes(), 128));
    }
    for &(target, kind, value, addend) in relocs {
        data.extend(words(&[target, kind]));
        data.extend(value.to_le_bytes());
        data.extend(addend.to_le_bytes());
    }
    data.extend(image);
    data
}

/// `<16sHHIQQQIHHHHHH`: an x86-64 ELF header whose `phnum` program headers follow it at offset 64.
fn elf_header(e_type: u16, phnum: u16) -> Vec<u8> {
    let mut header = ljust(b"\x7fELF\x02\x01\x01\x09", 16);
    header.extend(e_type.to_le_bytes());
    header.extend(62u16.to_le_bytes());
    header.extend(1u32.to_le_bytes());
    header.extend(words(&[0, 64, 0]));
    header.extend(0u32.to_le_bytes());
    for value in [64u16, 56, phnum, 0, 0, 0] {
        header.extend(value.to_le_bytes());
    }
    header
}

/// `<IIQQQQQQ`: type, flags, offset, vaddr, paddr, filesz, memsz, align.
fn program_header(kind: u32, flags: u32, rest: [u64; 6]) -> Vec<u8> {
    let mut header = kind.to_le_bytes().to_vec();
    header.extend(flags.to_le_bytes());
    header.extend(words(&rest));
    header
}

/// `tests/test_link_libc.py::self_fixture`: a SELF holding one prx-like module: one code byte, a
/// string table, one exported symbol `bzQExy189ZI#C#A` whose library C and module A are both libc
/// version 1, and empty relocation tables.
pub fn self_fixture() -> Vec<u8> {
    let mut data = vec![0; 0x3a0];
    put(&mut data, 0, b"O\x15=\x1d");
    put(&mut data, 24, &2u16.to_le_bytes());
    put(&mut data, 32, &words(&[0x800, 0x200, 16, 16]));
    put(&mut data, 64, &words(&[0x10_0800, 0x220, 0x160, 0x160]));
    put(&mut data, 96, &elf_header(0xfe18, 3));
    put(
        &mut data,
        160,
        &program_header(1, 5, [0x1000, 0, 0, 16, 16, 0x1000]),
    );
    put(
        &mut data,
        216,
        &program_header(0x6100_0000, 4, [0x2000, 0, 0, 0x160, 0, 16]),
    );
    put(
        &mut data,
        272,
        &program_header(2, 4, [0x20b0, 0, 0, 176, 176, 8]),
    );
    data[0x200] = 0xc3;
    put(&mut data, 0x220, b"bzQExy189ZI#C#A\0");
    put(&mut data, 0x230, b"libc\0");
    let mut symbol = 0u32.to_le_bytes().to_vec();
    symbol.extend([0x12, 0]);
    symbol.extend(1u16.to_le_bytes());
    symbol.extend(words(&[0, 1]));
    put(&mut data, 0x260, &symbol);
    let libc = (1 << 32) | 16; // version 1, name at string offset 16
    let tags = [
        (0x6100_0035, 0),
        (0x6100_0037, 32),
        (0x6100_0039, 64),
        (0x6100_003f, 24),
        (0x6100_0029, 0),
        (0x6100_002d, 0),
        (0x6100_002f, 0),
        (0x6100_0031, 0),
        (0x6100_0013, (2 << 48) | libc),
        (0x6100_000d, libc),
        (0, 0),
    ];
    for (i, (tag, value)) in tags.into_iter().enumerate() {
        put(&mut data, 0x2d0 + 16 * i, &words(&[tag, value]));
    }
    data
}

/// A program header of a synthetic module and the bytes it holds in the file (its `filesz`).
pub struct Program {
    pub kind: u32,
    pub flags: u32,
    pub vaddr: u64,
    pub memsz: u64,
    pub align: u64,
    pub data: Vec<u8>,
}

/// A symbol of a synthetic module's table: `name` (empty for none), `st_info`, section, value, size.
pub struct Sym<'a> {
    pub name: &'a str,
    pub info: u8,
    pub section: u16,
    pub value: u64,
    pub size: u64,
}

/// An unpacked x86-64 ELF of type `e_type`: the program headers, then each header's bytes at its
/// own file offset (from 0x1000 on, 0x100-aligned), as `prepare::parse_self` reads a plain ELF.
pub fn elf(e_type: u16, programs: &[Program]) -> Vec<u8> {
    let mut offsets = Vec::new();
    let mut cursor = 0x1000;
    for program in programs {
        offsets.push(cursor);
        cursor = (cursor + program.data.len()).div_ceil(0x100) * 0x100;
    }
    let mut data = elf_header(e_type, programs.len() as u16);
    for (program, &offset) in programs.iter().zip(&offsets) {
        let (filesz, offset) = (program.data.len() as u64, offset as u64);
        data.extend(program_header(
            program.kind,
            program.flags,
            [
                offset,
                program.vaddr,
                program.vaddr,
                filesz,
                program.memsz,
                program.align,
            ],
        ));
    }
    for (program, &offset) in programs.iter().zip(&offsets) {
        data.resize(offset, 0);
        data.extend(&program.data);
    }
    data
}

/// The dynamic table (`PT_DYNAMIC`) and library blob (`PT_SCE_DYNLIBDATA`) of a module: a string
/// table, `symbols` and `relocations` (`(target, type, symbol, addend)`, all in the first table the
/// linkers read). The dynamic tags are `tags`, then one per entry of `ids` (`(tag, id, name,
/// version)`, a library or module id), then those that locate the tables.
pub fn dynamic(
    tags: &[(u64, u64)],
    ids: &[(u64, u64, &str, u64)],
    symbols: &[Sym],
    relocations: &[(u64, u32, u64, i64)],
) -> [Program; 2] {
    let mut strings = vec![0u8];
    let mut add = |text: &str| {
        let at = strings.len() as u64;
        strings.extend(text.as_bytes());
        strings.push(0);
        at
    };
    let mut entries = tags.to_vec();
    for &(tag, id, name, version) in ids {
        entries.push((tag, (id << 48) | (version << 32) | add(name)));
    }
    let mut table = Vec::new();
    for symbol in symbols {
        let name = if symbol.name.is_empty() {
            0
        } else {
            add(symbol.name) as u32
        };
        table.extend(name.to_le_bytes());
        table.extend([symbol.info, 0]);
        table.extend(symbol.section.to_le_bytes());
        table.extend(words(&[symbol.value, symbol.size]));
    }
    let mut rela = Vec::new();
    for &(target, kind, symbol, addend) in relocations {
        rela.extend(words(&[target, (symbol << 32) | u64::from(kind)]));
        rela.extend(addend.to_le_bytes());
    }
    let mut blob = Vec::new();
    for (offset_tag, size_tag, content) in [
        (0x6100_0035, 0x6100_0037, strings),
        (0x6100_0039, 0x6100_003f, table),
        (0x6100_0029, 0x6100_002d, rela),
        (0x6100_002f, 0x6100_0031, Vec::new()),
    ] {
        entries.push((offset_tag, blob.len() as u64));
        entries.push((size_tag, content.len() as u64));
        let end = blob.len() + content.len().div_ceil(8) * 8;
        blob.extend(content);
        blob.resize(end, 0);
    }
    entries.push((0, 0));
    let table = entries
        .iter()
        .flat_map(|&(tag, value)| words(&[tag, value]))
        .collect();
    [
        Program {
            kind: 2,
            flags: 4,
            vaddr: 0,
            memsz: 0,
            align: 8,
            data: table,
        },
        Program {
            kind: 0x6100_0000,
            flags: 4,
            vaddr: 0,
            memsz: 0,
            align: 16,
            data: blob,
        },
    ]
}
