#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Tests on the player's own dump of the game: `SPARTAN_GAME_DIR` names the folder with
//! `eboot.bin`. Without it every test here returns at once (CI runs only the synthetic tests).
//! Nothing here writes into the game folder.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use spartanport_launcher::core::patches::{
    compile_patches, eboot_image, eboot_segments, write_game_patches, PatchJob, Write,
};
use spartanport_launcher::runner::link_modules::DEFAULT_MODULES;
use spartanport_launcher::runner::pause_menu::{self, decode, MenuError, Op, Record, Script};
use spartanport_launcher::runner::prepare::{prepare, read_self};
use spartanport_launcher::runner::{glyphs, isa_rewrite, link_libc, link_modules, pause_menu_args};

/// The interpreter the comparisons against the frozen Python use: `SPARTANPORT_PYTHON` as one
/// program path, else `py -3` on Windows when `py.exe` is on `PATH`, `python` on Windows
/// without it, `python3` elsewhere.
fn python_command(env: &dyn Fn(&str) -> Option<String>) -> Result<Vec<String>, ()> {
    if let Some(program) = env("SPARTANPORT_PYTHON").filter(|p| !p.is_empty()) {
        return Ok(vec![program]);
    }
    if cfg!(windows) {
        let path = env("PATH").unwrap_or_default();
        let has_py = std::env::split_paths(&path).any(|dir| dir.join("py.exe").is_file());
        if has_py {
            Ok(vec!["py".to_string(), "-3".to_string()])
        } else {
            Ok(vec!["python".to_string()])
        }
    } else {
        Ok(vec!["python3".to_string()])
    }
}

/// The game folder of `SPARTAN_GAME_DIR`, when it holds an `eboot.bin`.
fn dump() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("SPARTAN_GAME_DIR")?);
    dir.join("eboot.bin").is_file().then_some(dir)
}

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
}

fn spartan() -> PathBuf {
    repo("patches/CUSA01623.xml")
}

fn metadata_names(xml: &Path) -> Vec<String> {
    let text = fs::read_to_string(xml).unwrap();
    let doc = roxmltree::Document::parse(&text).unwrap();
    doc.descendants()
        .filter(|n| n.has_tag_name("Metadata"))
        .map(|m| m.attribute("Name").unwrap().to_string())
        .collect()
}

fn by_address(writes: &[Write]) -> HashMap<u64, Vec<u8>> {
    writes
        .iter()
        .map(|w| (w.address, w.bytes.clone()))
        .collect()
}

/// Where the rel32 at `data[at..at + 4]` points, measured from `end`.
fn rel(data: &[u8], at: usize, end: u64) -> u64 {
    let disp = i32::from_le_bytes(data[at..at + 4].try_into().unwrap());
    end.checked_add_signed(i64::from(disp)).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn spartanport_signatures_in_the_reference_and_a_moved_build() {
    let Some(game) = dump() else {
        return;
    };
    let elf = read_self(&game.join("eboot.bin")).unwrap().elf;
    let image = eboot_image(&elf).unwrap();
    // As patches.py: .bss included (memsz).
    let segments = eboot_segments(&elf).unwrap();
    let compile = |name: &str, version: Option<&str>, image: Option<&[u8]>| {
        compile_patches(&spartan(), &[name.to_string()], version, &segments, image)
            .unwrap()
            .0
    };
    // The reference build: the signatures give exactly the Address writes.
    for name in metadata_names(&spartan()) {
        assert_eq!(
            compile(&name, None, Some(&image)),
            compile(&name, Some("01.02"), None),
            "{name}"
        );
    }
    // A build whose code moved like the full 01.02 update's: +0x120 from 0x1f0000 to 0x210000,
    // data unchanged (the bytes taken out keep the data at its address).
    let mut moved = image[..0x1f_0000].to_vec();
    moved.extend(vec![0; 0x120]);
    moved.extend(&image[0x1f_0000..0x21_0000]);
    moved.extend(&image[0x21_0120..]);
    assert_eq!(moved.len(), image.len());
    let shift = |at: u64| {
        if (0x1f_0000..0x21_0000).contains(&at) {
            at + 0x120
        } else {
            at
        }
    };
    for name in [
        "Resolution Patch - 4K",
        "Resolution Patch - 8K",
        "Skip Intro",
    ] {
        let reference = compile(name, None, Some(&image));
        let found = compile(name, None, Some(&moved));
        let found: Vec<u64> = found.iter().map(|w| w.address).collect();
        let expected: Vec<u64> = reference.iter().map(|w| shift(w.address)).collect();
        assert_eq!(found, expected, "{name}");
    }
    // Skip Intro's cave moved: the boot call into it is recomputed, the flag (data) is not.
    let writes = by_address(&compile("Skip Intro", None, Some(&moved)));
    assert_eq!(rel(&writes[&0x01_b36a], 1, 0x01_b36f), shift(0x21_2170));
    // The getter (not moved) and the cave share it.
    let flag = rel(&writes[&0x0c_8440], 2, 0x0c_844a);
    let cave = shift(0x21_2170);
    assert_eq!(rel(&writes[&cave], 2, cave + 10), flag);
}

#[test]
fn spartanport_skip_intro_original_bytes() {
    let Some(game) = dump() else {
        return;
    };
    let info = read_self(&game.join("eboot.bin")).unwrap();
    let text = info
        .program_headers
        .iter()
        .find(|p| p.kind == 1 && p.flags & 1 != 0)
        .unwrap();
    let nops = "90".repeat(15);
    for (offset, value) in [
        (0x18_4950, "7534"),
        (0x0c_8440, "488d05955b38008b00c3660f1f440000"),
        (0x01_b36a, "e821990900"),
        (0x21_2170, nops.as_str()),
    ] {
        let at = usize::try_from(text.offset + offset - text.vaddr).unwrap();
        assert_eq!(
            hex(&info.elf[at..at + value.len() / 2]),
            value,
            "{offset:#x}"
        );
    }
}

/// A third-party patch file and its selection: the Python and the Rust must both apply them on
/// the reference build.
const EXTERNAL: &str = r#"<Patch>
  <TitleID><ID>CUSA01623</ID></TitleID>
  <Metadata Name="On" AppVer="01.02" AppElf="eboot.bin" isEnabled="true"><PatchList><Line Type="bytes" Address="0x00401000" Value="9090"/></PatchList></Metadata>
  <Metadata Name="Off" AppVer="01.02" AppElf="eboot.bin"><PatchList><Line Type="bytes32" Address="0x00402000" Value="0x12345678"/></PatchList></Metadata>
  <Metadata Name="Mask" AppVer="01.02" AppElf="eboot.bin" isEnabled="true"><PatchList><Line Type="mask" Value="90 ?? 90" Offset="0"/></PatchList></Metadata>
</Patch>"#;

/// One start's patch step: the `--extra` names, `SPARTAN_PATCHES_ONLY`, the mods' patch files
/// and whether the third-party folder holds [`EXTERNAL`].
struct Case {
    name: &'static str,
    extra: &'static str,
    only: bool,
    mods: Vec<PathBuf>,
    external: bool,
}

/// Runs `scripts/patches.py` as `run.py` does (`run_script('patches.py', '--out', out, '--extra',
/// SPARTAN_PATCHES, '--game-dir', game, '--patches-dir', ..., '--patches-config', ...,
/// '--mod-patches', ...)` with `SPARTAN_PATCHES_ONLY` and `PYTHONIOENCODING=utf-8`). `None` when no
/// interpreter starts.
fn python_patches(
    case: &Case,
    out: &Path,
    game: &Path,
    patches_dir: &Path,
    config: &Path,
) -> Option<()> {
    let env = |key: &str| std::env::var(key).ok();
    let command = python_command(&env).ok()?;
    let (program, args) = command.split_first()?;
    let mut python = Command::new(program);
    python
        .args(args)
        .arg(repo("scripts/patches.py"))
        .arg("--out")
        .arg(out)
        .arg("--extra")
        .arg(case.extra)
        .arg("--game-dir")
        .arg(game)
        .arg("--patches-dir")
        .arg(patches_dir)
        .arg("--patches-config")
        .arg(config);
    for path in &case.mods {
        python.arg("--mod-patches").arg(path);
    }
    python
        .env("SPARTAN_PATCHES_ONLY", if case.only { "1" } else { "0" })
        .env("PYTHONIOENCODING", "utf-8");
    let output = python.output().ok()?;
    assert!(
        output.status.success(),
        "{}: patches.py failed:\n{}",
        case.name,
        String::from_utf8_lossy(&output.stderr)
    );
    Some(())
}

#[test]
fn real_patches_bin_matches_python() {
    let Some(game) = dump() else {
        return;
    };
    let elf = read_self(&game.join("eboot.bin")).unwrap().elf;
    let cases = [
        Case {
            name: "defaults",
            extra: "",
            only: false,
            mods: Vec::new(),
            external: false,
        },
        Case {
            name: "chosen",
            extra: "Resolution Patch - 4K;Frame Rate Patch - 120 FPS;Skip Intro;Skip Any Video With X Button",
            only: true,
            mods: Vec::new(),
            external: true,
        },
        Case {
            name: "mod",
            extra: "Resolution Patch - 3440x1440;Frame Rate Patch - 144 FPS",
            only: true,
            mods: vec![repo("extras/mods/Ultrawide/patches/Ultrawide.xml")],
            external: false,
        },
    ];
    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        let patches_dir = dir.path().join("patches");
        fs::create_dir_all(&patches_dir).unwrap();
        let config = dir.path().join("patches.json");
        if case.external {
            fs::write(patches_dir.join("extra.xml"), EXTERNAL).unwrap();
            fs::write(&config, r#"{"enabled": ["extra.xml/Off"]}"#).unwrap();
        }
        let (python, rust) = (dir.path().join("python"), dir.path().join("rust"));
        for out in [&python, &rust] {
            fs::create_dir_all(out).unwrap();
            fs::write(out.join("eboot.elf"), &elf).unwrap();
        }
        if python_patches(&case, &python, &game, &patches_dir, &config).is_none() {
            return;
        }
        let extra: Vec<String> = case.extra.split(';').map(str::to_string).collect();
        let job = PatchJob {
            out: &rust,
            game: &game,
            builtin_dir: &repo("patches"),
            mod_patches: &case.mods,
            extra: &extra,
            only: case.only,
            patches_dir: Some(&patches_dir),
            patches_config: Some(&config),
        };
        let mut log = Vec::new();
        write_game_patches(&job, &mut log).unwrap();
        let log = String::from_utf8(log).unwrap();
        for name in ["patches.bin", "patch_status.json"] {
            let ours = fs::read(rust.join(name)).unwrap();
            let theirs = fs::read(python.join(name)).unwrap();
            assert!(ours == theirs, "{}: {name} differs\n{log}", case.name);
        }
        if case.external {
            assert!(
                log.contains("Patches: external extra.xml/On (1 writes)\n"),
                "{log}"
            );
            assert!(
                log.contains("Patches: external extra.xml/Off (1 writes)\n"),
                "{log}"
            );
        }
        if !case.mods.is_empty() {
            // The Python writes it in text mode: CRLF on Windows.
            let ours = fs::read_to_string(rust.join("patches-merged.xml")).unwrap();
            let theirs = fs::read_to_string(python.join("patches-merged.xml")).unwrap();
            assert!(
                ours == theirs.replace("\r\n", "\n"),
                "{}: patches-merged.xml differs",
                case.name
            );
        }
    }
}

/// Runs `scripts/<script> <args>` as `run.py`'s `run_script` does (`PYTHONIOENCODING=utf-8`): its
/// exit code and what it printed, CRLF as LF. `None` when no interpreter starts.
fn python_script(script: &str, args: &[&OsStr]) -> Option<(i32, String)> {
    let env = |key: &str| std::env::var(key).ok();
    let command = python_command(&env).ok()?;
    let (program, interpreter) = command.split_first()?;
    let output = Command::new(program)
        .args(interpreter)
        .arg(repo("scripts").join(script))
        .args(args)
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .ok()?;
    let printed = [&output.stdout[..], &output.stderr[..]].concat();
    let printed = String::from_utf8_lossy(&printed).replace("\r\n", "\n");
    Some((output.status.code().unwrap_or(-1), printed))
}

#[test]
fn real_boot_linked_bin_matches_python() {
    let Some(game) = dump() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let (python, rust) = (dir.path().join("python"), dir.path().join("rust"));
    // run.py's order: prepare.py, link_libc.py and link_modules.py on the game.
    for script in ["prepare.py", "link_libc.py", "link_modules.py"] {
        let args = [game.as_os_str(), OsStr::new("--out"), python.as_os_str()];
        let Some((code, printed)) = python_script(script, &args) else {
            return;
        };
        assert_eq!(code, 0, "{script} failed:\n{printed}");
    }
    prepare(&game, &rust).unwrap();
    link_libc::link(&game, &rust).unwrap();
    link_modules::link(&game, &rust, &DEFAULT_MODULES).unwrap();
    // The ISA table run.py asks for on a CPU without MOVBE, BMI1, LZCNT or F16C.
    let boot = python.join("boot-linked.bin");
    let args = [boot.as_os_str(), OsStr::new("--out"), python.as_os_str()];
    let (code, printed) = python_script("isa_rewrite.py", &args).unwrap();
    let sites = isa_rewrite::rewrite(&rust.join("boot-linked.bin"), &rust).unwrap();
    let mut binaries = vec![
        "boot.bin",
        "eboot.elf",
        "entry.bin",
        "boot-libc.bin",
        "boot-linked.bin",
    ];
    if printed.contains("iced_x86 is not installed") {
        println!("This Python has no iced_x86: isa-rewrite.bin is not compared");
    } else {
        assert_eq!(code, 0, "isa_rewrite.py failed:\n{printed}");
        assert_eq!(printed, format!("{}\n", isa_rewrite::summary(&sites)));
        binaries.push("isa-rewrite.bin");
    }
    for name in binaries {
        let ours = fs::read(rust.join(name)).unwrap();
        let theirs = fs::read(python.join(name)).unwrap();
        let first = ours.iter().zip(&theirs).position(|(a, b)| a != b);
        assert!(
            ours == theirs,
            "{name} differs (sizes {} and {}, first difference at {first:?})",
            ours.len(),
            theirs.len()
        );
    }
    // The Python writes them in text mode: CRLF on Windows.
    for name in ["analysis.json", "libc-link.json", "link.json"] {
        let ours = fs::read_to_string(rust.join(name)).unwrap();
        let theirs = fs::read_to_string(python.join(name)).unwrap();
        assert!(ours == theirs.replace("\r\n", "\n"), "{name} differs");
    }
}

/// Runs `glyphs.build(game, out, style)` of `scripts/glyphs.py` (`None` when no interpreter starts).
fn python_glyphs(game: &Path, out: &Path, style: &str) -> Option<()> {
    let env = |key: &str| std::env::var(key).ok();
    let command = python_command(&env).ok()?;
    let (program, args) = command.split_first()?;
    let script = "import sys; sys.path.insert(0, sys.argv[1]); import glyphs; glyphs.build(sys.argv[2], sys.argv[3], sys.argv[4])";
    let output = Command::new(program)
        .args(args)
        .args(["-B", "-c", script])
        .arg(repo("scripts"))
        .arg(game)
        .arg(out)
        .arg(style)
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .ok()?;
    assert!(
        output.status.success(),
        "glyphs.build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(())
}

/// The DXT blocks of the texture whose DDS header is at `dds`, level by level: (offset, size).
fn dxt_blocks(data: &[u8], dds: usize, end: usize) -> Vec<(usize, usize)> {
    let height = u32::from_le_bytes(data[dds + 12..dds + 16].try_into().unwrap());
    let width = u32::from_le_bytes(data[dds + 16..dds + 20].try_into().unwrap());
    let block = if &data[dds + 84..dds + 88] == b"DXT5" {
        16
    } else {
        8
    };
    let (mut offset, mut level, mut blocks) = (dds + 128, 0u32, Vec::new());
    while offset < end {
        let w = (width.checked_shr(level).unwrap_or(0)).max(1) as usize;
        let h = (height.checked_shr(level).unwrap_or(0)).max(1) as usize;
        let size = w.div_ceil(4) * h.div_ceil(4) * block;
        if offset + size > end {
            break;
        }
        blocks.extend((0..size / block).map(|i| (offset + i * block, block)));
        offset += size;
        level += 1;
        if w == 1 && h == 1 {
            break;
        }
    }
    blocks
}

#[test]
fn real_glyph_wads_match_python_outside_the_painted_blocks() {
    let Some(game) = dump() else {
        return;
    };
    let assets = glyphs::Assets::in_port(&repo(""));
    for style in ["xbox", "nintendo", "keyboard"] {
        let dir = tempfile::tempdir().unwrap();
        let (python, rust) = (dir.path().join("python"), dir.path().join("rust"));
        if python_glyphs(&game, &python, style).is_none() {
            return;
        }
        let mounts = glyphs::build(&game, &rust, style, None, "", "", &assets, &mut |line| {
            println!("{line}")
        })
        .unwrap();
        let theirs = fs::read(python.join("glyphs/stamp.json")).unwrap();
        assert_eq!(
            fs::read(rust.join("glyphs/stamp.json")).unwrap(),
            theirs,
            "{style}: stamp.json"
        );
        for name in glyphs::WADS {
            let file = Path::new(name).file_name().unwrap();
            let (ours, theirs) = (
                rust.join("glyphs").join(file),
                python.join("glyphs").join(file),
            );
            assert_eq!(
                ours.is_file(),
                theirs.is_file(),
                "{style}: {name} made by one side only"
            );
            if !theirs.is_file() {
                continue;
            }
            assert!(
                mounts.contains_key(&format!("/app0/{name}")),
                "{style}: {name} not mounted"
            );
            let original = fs::read(game.join(name)).unwrap();
            let (ours, theirs) = (fs::read(ours).unwrap(), fs::read(theirs).unwrap());
            assert_eq!(ours.len(), theirs.len(), "{style}: {name} size");
            assert_eq!(theirs.len(), original.len(), "{style}: {name} size");
            // Both sides repaint the same entries; inside those, a DXT block either side
            // re-encoded is exempt (the text's edges move which blocks a lyre re-encode touches).
            // Every other byte of both copies is the game's own.
            let mut painted = vec![false; ours.len()];
            for entry in glyphs::entries(&original) {
                let (start, end) = (entry.offset + 0x40, entry.offset + 0x40 + entry.size);
                let changed = |copy: &[u8]| copy[start..end] != original[start..end];
                assert_eq!(
                    changed(&ours),
                    changed(&theirs),
                    "{style}: {name}: {} repainted by one side only (Rust: {})",
                    entry.name,
                    changed(&ours)
                );
                if !changed(&theirs) {
                    continue;
                }
                let Some(dds) = original[start..end]
                    .windows(4)
                    .position(|w| w == b"DDS ")
                    .map(|i| i + start)
                else {
                    continue;
                };
                if !matches!(&original[dds + 84..dds + 88], b"DXT1" | b"DXT5") {
                    continue;
                }
                for (at, size) in dxt_blocks(&original, dds, end) {
                    let changed = |copy: &[u8]| copy[at..at + size] != original[at..at + size];
                    if changed(&ours) || changed(&theirs) {
                        painted[at..at + size].fill(true);
                    }
                }
            }
            let differing = (0..ours.len())
                .find(|i| !painted[*i] && (ours[*i] != original[*i] || theirs[*i] != original[*i]));
            assert_eq!(
                differing, None,
                "{style}: {name} differs from the game outside the painted blocks"
            );
        }
    }
}

// -- Pause menu (scripts/pause_menu.py): tests/test_pause_menu.py::GameScript and
// tests/test_native_cheats.py on the dump's R_UI.WAD ----------------------------------------------

/// Runs `pause_menu.build(game, out, source, language=language)` of `scripts/pause_menu.py`: what
/// it printed (`None` when no interpreter starts).
fn python_pause_menu(
    game: &Path,
    out: &Path,
    source: Option<&Path>,
    language: Option<&str>,
) -> Option<String> {
    let env = |key: &str| std::env::var(key).ok();
    let command = python_command(&env).ok()?;
    let (program, args) = command.split_first()?;
    let script = "import sys; sys.path.insert(0, sys.argv[1]); import pause_menu; \
                  pause_menu.build(sys.argv[2], sys.argv[3], sys.argv[4] or None, \
                  language=sys.argv[5] if len(sys.argv) > 5 else None)";
    let output = Command::new(program)
        .args(args)
        .args(["-B", "-c", script])
        .arg(repo("scripts"))
        .arg(game)
        .arg(out)
        .arg(source.map_or(Path::new(""), |p| p))
        .args(language)
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .ok()?;
    assert!(
        output.status.success(),
        "pause_menu.build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn real_menu_wad_matches_python() {
    let Some(game) = dump() else {
        return;
    };
    if !game.join(pause_menu::WAD).is_file() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // A copy of the WAD stands in for the glyphs' one, the source when it is mounted.
    let glyph_copy = dir.path().join("glyphs").join("R_UI.WAD");
    fs::create_dir_all(dir.path().join("glyphs")).unwrap();
    fs::copy(game.join(pause_menu::WAD), &glyph_copy).unwrap();
    let cases = [
        ("default", None, None),
        ("de", None, Some("de")),
        (
            "glyph copy, pt-br",
            Some(glyph_copy.as_path()),
            Some("pt-br"),
        ),
    ];
    for (index, (case, source, language)) in cases.into_iter().enumerate() {
        let python = dir.path().join(format!("python-{index}"));
        let rust = dir.path().join(format!("rust-{index}"));
        let Some(printed) = python_pause_menu(&game, &python, source, language) else {
            return;
        };
        assert_eq!(printed, "", "{case}: the Python made no copy");
        let mounts: Vec<String> = source
            .map(|path| {
                vec![
                    "--mount".to_string(),
                    format!("/app0/{}={}", pause_menu::WAD, path.display()),
                ]
            })
            .unwrap_or_default();
        let mut log = Vec::new();
        let args = pause_menu_args(&game, &rust, &mounts, language, &mut |line| log.push(line));
        assert!(log.is_empty(), "{case}: {log:?}");
        let copy = rust.join("pause_menu").join("R_UI.WAD");
        assert_eq!(
            args,
            ["--pause-menu".to_string(), pause_menu::python_path(&copy)],
            "{case}"
        );
        for name in ["R_UI.WAD", "stamp.json"] {
            let ours = fs::read(rust.join("pause_menu").join(name)).unwrap();
            let theirs = fs::read(python.join("pause_menu").join(name)).unwrap();
            let first = ours.iter().zip(&theirs).position(|(a, b)| a != b);
            assert!(
                ours == theirs,
                "{case}: {name} differs (sizes {} and {}, first difference at {first:?})",
                ours.len(),
                theirs.len()
            );
        }
    }
}

/// The dump's `R_UI.WAD` and its copy patched in English (`GameScript.setUp`,
/// `NativeCheats.setUpClass`), made once.
struct Menu {
    original: Vec<u8>,
    data: Vec<u8>,
}

fn menu() -> Option<&'static Menu> {
    static MENU: OnceLock<Option<Menu>> = OnceLock::new();
    MENU.get_or_init(|| {
        let original = fs::read(dump()?.join(pause_menu::WAD)).ok()?;
        let data = pause_menu::patch(&original, Some("en")).unwrap();
        Some(Menu { original, data })
    })
    .as_ref()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

/// `haystack.count(needle)`: non-overlapping occurrences.
fn count(haystack: &[u8], needle: &[u8]) -> usize {
    let (mut found, mut at) = (0, 0);
    while let Some(i) = find(&haystack[at..], needle) {
        found += 1;
        at += i + needle.len();
    }
    found
}

/// `haystack.replace(from, to)`.
fn replace(haystack: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let (mut out, mut at) = (Vec::new(), 0);
    while let Some(i) = find(&haystack[at..], from) {
        out.extend(&haystack[at..at + i]);
        out.extend(to);
        at += i + from.len();
    }
    out.extend(&haystack[at..]);
    out
}

fn be(value: usize, size: usize) -> Vec<u8> {
    pause_menu::be(value, size).unwrap()
}

fn le16(data: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes(data[at..at + 2].try_into().unwrap()))
}

fn le32(data: &[u8], at: usize) -> usize {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize
}

fn f32_le(data: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}

/// `16 <literal>`: the instruction that pushes `text`.
fn push(s: &Script, text: &[u8]) -> Vec<u8> {
    [&[0x16][..], &be(s.literal(text).unwrap(), 2)].concat()
}

/// The code of `record`'s first block of `kind`.
fn first_block(record: &Record, kind: u8) -> usize {
    Script::blocks(record)
        .into_iter()
        .find(|b| b.kind == kind)
        .unwrap()
        .code
}

/// Where `record` is in the state table.
fn position(s: &Script, record: &Record) -> usize {
    s.records.iter().position(|r| r.at == record.at).unwrap()
}

/// The index of the state whose ref is `state`.
fn state_index(s: &Script, state: u64) -> usize {
    s.records
        .iter()
        .position(|r| (r.at - 4) as u64 == state)
        .unwrap()
}

/// `{code offset: index of the state it goes to}` for the `47` instructions in `spans`.
fn gotos(data: &[u8], spans: &[(usize, usize)]) -> HashMap<usize, usize> {
    let s = Script::parse(data).unwrap();
    let mut out = HashMap::new();
    for &(a, b) in spans {
        for op in decode(data, s.base + s.lit + a, s.base + s.lit + b).unwrap() {
            if op.opcode == 0x47 {
                out.insert(op.offset - s.base - s.lit, state_index(&s, op.operand));
            }
        }
    }
    out
}

/// The label PauseMain's entry gives line5 (local 6).
fn exit_label(data: &[u8]) -> Vec<u8> {
    let s = Script::parse(data).unwrap();
    let main = s.one("PauseMain").unwrap();
    let entry = s.block_bytes(first_block(main, 1)).unwrap();
    let at = find(entry, b"\x09\xe0\x06\x43").unwrap();
    let offset = usize::from(u16::from_be_bytes([entry[at - 2], entry[at - 1]]));
    let start = s.base + s.lit + offset;
    let end = data[start..].iter().position(|b| *b == 0).unwrap() + start;
    data[start..end].to_vec()
}

fn label_in(menu: &Menu, language: &str) -> Vec<u8> {
    exit_label(&pause_menu::patch(&menu.original, Some(language)).unwrap())
}

#[test]
fn real_menu_enemy_health_bar_follows_screen_y_with_native_animation() {
    let Some(m) = menu() else {
        return;
    };
    let new = Script::parse(&m.data).unwrap();
    let display = new.one("PauseDisplay").unwrap();
    let row = new
        .by_name("HLSubtitleDis")
        .into_iter()
        .find(|r| r.parent == display.at - 4)
        .unwrap();
    let y = new.one("HLScreenLocY").unwrap();
    assert_eq!(row.at, y.at + y.size);
    assert_eq!(row.at + row.size, display.at + display.size);
    for (kind, layer) in [(1, 8), (2, 9)] {
        let code = new.block_bytes(first_block(row, kind)).unwrap();
        let clip = push(&new, b"menuItem3Select");
        assert!(
            contains(code, &[&[0x01][..], &be(layer, 4), &clip].concat()),
            "{kind}"
        );
    }
    let arrows = Script::blocks(row)
        .into_iter()
        .find(|b| b.kind == 3 && b.triggers.contains(&0xe9))
        .unwrap()
        .code;
    assert!(contains(
        new.block_bytes(arrows).unwrap(),
        b"\x13\x0f\xfd\x43"
    ));
    assert!(contains(&m.data, b"Enemy Health Bar\0"));
}

#[test]
fn real_menu_display_initializes_health_value_before_the_row_is_selected() {
    let Some(m) = menu() else {
        return;
    };
    let new = Script::parse(&m.data).unwrap();
    let entry = first_block(new.one("PauseDisplay").unwrap(), 1);
    let code = new.block_bytes(entry).unwrap();
    assert_eq!(code[0], 0x4a); // refresh the setting on every entry to Display
    let call = i32::from_be_bytes(code[1..5].try_into().unwrap());
    let refresh = (entry as i64 + 5 + i64::from(call)) as usize;
    let at = new.base + new.lit + refresh;
    assert_eq!(&m.data[at..at + 3], b"\x13\x0f\xfe");
    let first = new.base + new.lit + entry;
    let assignments: Vec<String> = decode(&m.data, first, first + code.len())
        .unwrap()
        .iter()
        .filter(|op| {
            op.opcode == 0x16 && m.data[op.offset + 3..op.offset + 7] == *b"\x09\xe0\x0c\x43"
        })
        .map(|op| new.string(new.lit + op.operand as usize).unwrap())
        .collect();
    assert_eq!(
        assignments,
        ["[ArrowSpacer][PauseMenu:optSet3][ArrowSpacer]"]
    );
}

#[test]
fn real_menu_pause_row() {
    let Some(m) = menu() else {
        return;
    };
    let new = Script::parse(&m.data).unwrap();
    let (main, row) = (new.one("PauseMain").unwrap(), new.one("WadChoose").unwrap());
    assert_eq!(row.parent, main.at - 4);
    assert_eq!(main.at + main.size, row.at + row.size);
    let transitions: Vec<Vec<usize>> = Script::blocks(row)
        .into_iter()
        .filter(|b| b.kind == 3)
        .map(|b| b.triggers)
        .collect();
    assert_eq!(
        transitions,
        [
            pause_menu::UP.to_vec(),
            pause_menu::DOWN.to_vec(),
            pause_menu::CONFIRM.to_vec(),
            pause_menu::REPLY.to_vec()
        ]
    );
    // A patched copy is refused, not patched twice.
    assert!(matches!(
        pause_menu::patch(&m.data, Some("en")),
        Err(MenuError::Script(_))
    ));
}

#[test]
fn real_menu_exit_asks_quit_games_question_with_the_mark() {
    let Some(m) = menu() else {
        return;
    };
    let new = Script::parse(&m.data).unwrap();
    let block = |row: &Record, trigger: &[usize]| {
        new.block_bytes(pause_menu::transition(row, trigger).unwrap())
            .unwrap()
    };
    let ask = block(new.one("HLQuitGame").unwrap(), &pause_menu::CONFIRM);
    let send = [0x13, 0x00, 0x5e, 0x43];
    for row in [new.one("WadChoose").unwrap(), new.by_name("HLDisplay")[1]] {
        let mine = block(row, &pause_menu::CONFIRM);
        // The mark, the count of 5, the send.
        let third = [
            &[0x01][..],
            &be(pause_menu::MARK, 4),
            &[0x01],
            &be(5, 4),
            &send,
        ]
        .concat();
        assert_eq!(count(mine, &third), 1);
        // The mark is five bytes longer.
        let without = replace(mine, &third, &[&[0x01][..], &be(4, 4), &send].concat());
        assert_eq!(without, ask[..(mine.len() - 5).min(ask.len())]);
        // "No" as Quit Game.
        let reply = block(row, &pause_menu::REPLY);
        assert_eq!(
            reply[..pause_menu::REPLY_TEMPLATE.len()],
            pause_menu::REPLY_TEMPLATE
        );
    }
}

#[test]
fn real_menu_failsafe_states_end_the_process_on_the_mark() {
    let Some(m) = menu() else {
        return;
    };
    let (old, new) = (
        Script::parse(&m.original).unwrap(),
        Script::parse(&m.data).unwrap(),
    );
    let plain = |code: &[Op]| -> Vec<(u8, u64)> {
        code.iter()
            .filter(|op| op.opcode != 0x47)
            .map(|op| (op.opcode, op.operand))
            .collect()
    };
    let gotos = |s: &Script, code: &[Op]| -> Vec<usize> {
        code.iter()
            .filter(|op| op.opcode == 0x47)
            .map(|op| state_index(s, op.operand))
            .collect()
    };
    for (name, trigger, prefix) in [
        (
            "FailSafeDisplay",
            &pause_menu::REPLY[..],
            &pause_menu::SHOW_PREFIX[..],
        ),
        ("FailSafeYes", &pause_menu::CONFIRM, &pause_menu::YES_PREFIX),
    ] {
        let ref_old = pause_menu::transition(old.one(name).unwrap(), trigger).unwrap();
        let ref_new = pause_menu::transition(new.one(name).unwrap(), trigger).unwrap();
        assert_ne!(ref_old, ref_new); // the state runs the new block
        let before = old.block_bytes(ref_old).unwrap();
        let after = new.block_bytes(ref_new).unwrap();
        assert!(after.starts_with(prefix), "{name}");
        let first = old.base + old.lit + ref_old;
        let second = new.base + new.lit + ref_new + prefix.len();
        let a = decode(&m.original, first, first + before.len()).unwrap();
        let b = decode(&m.data, second, second + before.len()).unwrap();
        assert_eq!(plain(&a), plain(&b), "{name}");
        assert_eq!(gotos(&old, &a).len(), gotos(&new, &b).len(), "{name}");
    }
    // The exit call is skipped unless the local holds the mark.
    let yes =
        pause_menu::transition(new.one("FailSafeYes").unwrap(), &pause_menu::CONFIRM).unwrap();
    let start = new.base + new.lit + yes;
    let code = decode(&m.data, start, start + pause_menu::YES_PREFIX.len()).unwrap();
    let jump = code.iter().find(|op| op.opcode == 0x17).unwrap();
    assert_eq!(
        jump.offset as i64 + 3 + i64::from(jump.operand as u16 as i16),
        code.last().unwrap().offset as i64 + 1
    );
}

#[test]
fn real_menu_options_row() {
    let Some(m) = menu() else {
        return;
    };
    let (old, new) = (
        Script::parse(&m.original).unwrap(),
        Script::parse(&m.data).unwrap(),
    );
    let (options, row) = (
        new.one("PauseOptions").unwrap(),
        new.by_name("HLDisplay")[1],
    );
    assert_eq!(new.records.len(), old.records.len() + 21);
    assert_eq!(row.parent, options.at - 4);
    let cheats = new.one("HLNativeCheats").unwrap();
    assert_eq!(
        options.size,
        old.one("PauseOptions").unwrap().size + pause_menu::ROW_SIZE + cheats.size
    );
    assert_eq!(row.at + row.size, cheats.at);
    // The WAD entry, the script header and the padding agree with the new size.
    let size = le32(&m.data, new.base - 0x40 + 4);
    let old_size = le32(&m.original, old.base - 0x40 + 4);
    assert_eq!(le32(&m.data, new.base + 8), size - 4);
    // Six dedicated native slider instances (352 bytes each) and their 64-byte references.
    assert_eq!(
        m.data.len() - m.original.len(),
        ((size + 15) & !15) - ((old_size + 15) & !15) + 2496
    );
    assert_eq!(glyphs::entries(&m.data).last().unwrap().name, "GroupEnd");
}

#[test]
fn real_menu_every_other_goto_is_unchanged() {
    let Some(m) = menu() else {
        return;
    };
    let (old, new) = (
        Script::parse(&m.original).unwrap(),
        Script::parse(&m.data).unwrap(),
    );
    let dead = old.one("WadChoose").unwrap(); // its code is replaced by the pause row's
    let own: Vec<usize> = dead
        .items
        .iter()
        .filter(|(kind, _)| (1..=3).contains(kind))
        .map(|(_, code)| *code)
        .collect();
    let lo = *own.iter().min().unwrap();
    let last = *own.iter().max().unwrap();
    let hi = old.code_refs().into_iter().find(|r| *r > last).unwrap();
    let spans: Vec<(usize, usize)> = old
        .spans()
        .unwrap()
        .into_iter()
        .filter(|(a, _)| !(lo..hi).contains(a))
        .collect();
    let before = gotos(&m.original, &spans);
    let after = gotos(&m.data, &spans);
    let inserted = position(&new, new.by_name("HLDisplay")[1]);
    let display = new.one("PauseDisplay").unwrap();
    let health = new
        .records
        .iter()
        .position(|r| r.at == display.at + display.size - 0x44)
        .unwrap();
    let original_indices: Vec<usize> = new
        .records
        .iter()
        .enumerate()
        .filter(|(i, r)| {
            let name = new.name_of(r).unwrap_or("");
            *i != inserted
                && *i != health
                && !name.starts_with("Native")
                && !name.starts_with("HLNative")
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(original_indices.len(), old.records.len());
    let moved = |i: usize| original_indices[i];
    let mut changed: Vec<usize> = before
        .iter()
        .filter(|(at, b)| after[*at] != moved(**b))
        .map(|(at, _)| after[at])
        .collect();
    changed.sort_unstable();
    let wad_choose = moved(position(&old, dead));
    let cheats = position(&new, new.one("HLNativeCheats").unwrap());
    // Only the neighbors of the four added rows change destinations.
    let mut expected = vec![
        wad_choose, wad_choose, inserted, inserted, health, health, cheats, cheats,
    ];
    expected.sort_unstable();
    assert_eq!(changed, expected);
}

#[test]
fn real_menu_japanese_fallback() {
    let Some(m) = menu() else {
        return;
    };
    assert_eq!(label_in(m, "ja"), b"Exit to Desktop");
}

#[test]
fn real_menu_chinese_fallback() {
    let Some(m) = menu() else {
        return;
    };
    assert_eq!(label_in(m, "zh"), b"Exit to Desktop");
}

#[test]
fn real_menu_russian_fallback() {
    let Some(m) = menu() else {
        return;
    };
    assert_eq!(label_in(m, "ru"), b"Exit to Desktop");
}

#[test]
fn real_menu_polish_translated() {
    let Some(m) = menu() else {
        return;
    };
    assert_eq!(label_in(m, "pl"), "Wyjdź do pulpitu".as_bytes());
}

#[test]
fn real_menu_all_supported_languages() {
    let Some(m) = menu() else {
        return;
    };
    for label in &pause_menu::LABELS {
        assert_eq!(label_in(m, label.key), label.exit, "{}", label.key);
    }
}

/// The instructions of `record`'s first block of `kind` (with `triggers`), up to the next block
/// (`tests/test_native_cheats.py::NativeCheats.block`).
fn block(s: &Script, record: &Record, kind: u8, triggers: Option<&[usize]>) -> Vec<Op> {
    let at = Script::blocks(record)
        .into_iter()
        .find(|b| b.kind == kind && triggers.is_none_or(|t| b.triggers == t))
        .unwrap()
        .code;
    let end = s
        .code_refs()
        .into_iter()
        .find(|v| *v > at)
        .unwrap_or(s.symtab - s.lit);
    decode(s.data(), s.base + s.lit + at, s.base + s.lit + end).unwrap()
}

/// The state `name` goes to when `triggers` fire.
fn target(s: &Script, name: &str, triggers: &[usize]) -> usize {
    let code = block(s, s.one(name).unwrap(), 3, Some(triggers));
    code.iter().find(|op| op.opcode == 0x47).unwrap().operand as usize
}

/// The texts `code` pushes.
fn texts(s: &Script, code: &[Op]) -> Vec<String> {
    code.iter()
        .filter(|op| op.opcode == 0x16)
        .map(|op| s.string(s.lit + op.operand as usize).unwrap())
        .collect()
}

fn has(texts: &[String], text: &str) -> bool {
    texts.iter().any(|t| t == text)
}

fn operands(code: &[Op], opcode: u8) -> Vec<u64> {
    code.iter()
        .filter(|op| op.opcode == opcode)
        .map(|op| op.operand)
        .collect()
}

/// The triggers of `row`'s confirm toggle.
fn toggle_of(row: &Record) -> Vec<usize> {
    Script::blocks(row)
        .into_iter()
        .find(|b| b.kind == 3 && b.triggers.contains(&0xe9))
        .unwrap()
        .triggers
}

/// `{name: (offset, size)}` of the WAD entries with data (the last of a name wins).
fn objects(data: &[u8]) -> HashMap<String, (usize, usize)> {
    glyphs::entries(data)
        .into_iter()
        .filter(|e| e.size != 0)
        .map(|e| (e.name, (e.offset, e.size)))
        .collect()
}

/// The states `native_cheats` adds (with or without `NativeMultipliersLink`).
fn native_names(with_link: bool) -> Vec<String> {
    let mut names: Vec<String> = ["HLNativeCheats", "NativeCheats", "NativeMultipliers"]
        .map(String::from)
        .to_vec();
    if with_link {
        names.push("NativeMultipliersLink".to_string());
    }
    names.push("NativeMultiplierReset".to_string());
    names.extend((0..8).map(|i| format!("NativeCheat{i}")));
    names.extend((0..6).map(|i| format!("NativeMultiplier{i}")));
    names
}

#[test]
fn real_menu_cheats_is_between_game_settings_and_audio() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    let row = s.one("HLNativeCheats").unwrap();
    let (up, down) = (&pause_menu::UP, &pause_menu::DOWN);
    assert_eq!(target(&s, "HLGSettings", down), row.at - 4);
    assert_eq!(target(&s, "HLAudio", up), row.at - 4);
    assert_eq!(
        target(&s, "HLNativeCheats", up),
        s.one("HLGSettings").unwrap().at - 4
    );
    assert_eq!(
        target(&s, "HLNativeCheats", down),
        s.one("HLAudio").unwrap().at - 4
    );
}

#[test]
fn real_menu_every_cheat_has_native_selection_and_initial_value() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    let page = s.one("NativeCheats").unwrap();
    let queries = operands(&block(&s, page, 1, None), 0x13);
    for index in 0..8usize {
        assert!(queries.contains(&(0xf00 + index as u64)), "{index}");
        let row = s.one(&format!("NativeCheat{index}")).unwrap();
        assert_eq!(row.parent, page.at - 4);
        let texts = texts(&s, &block(&s, row, 1, None));
        let slot = index - usize::from(index >= 6);
        assert!(has(&texts, &format!("menuItem{slot}Select")), "{index}");
        let toggle = operands(&block(&s, row, 3, Some(&toggle_of(row))), 0x13);
        assert!(toggle.contains(&(0xf10 + index as u64)), "{index}");
    }
}

#[test]
fn real_menu_qte_precedes_multipliers_and_last_row_scrolls() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    assert!(contains(&m.data, b"Automatic QTE\0"));
    let (up, down) = (&pause_menu::UP, &pause_menu::DOWN);
    assert_eq!(
        target(&s, "NativeCheat7", down),
        s.one("NativeMultipliersLink").unwrap().at - 4
    );
    assert_eq!(
        target(&s, "NativeMultipliersLink", up),
        s.one("NativeCheat7").unwrap().at - 4
    );
    let entry = block(&s, s.one("NativeMultipliersLink").unwrap(), 1, None);
    let texts = texts(&s, &entry);
    for text in ["Infinite Magic", "Multipliers", "menuItem7Select"] {
        assert!(has(&texts, text), "{text}");
    }
    for op in entry.iter().filter(|op| op.opcode == 0x09) {
        assert!(op.operand & 0xff <= 0x18, "{op:?}");
    }
}

#[test]
fn real_menu_multipliers_stays_visible_and_qte_uses_a_visible_value_slot() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    for name in [
        "NativeCheat0",
        "NativeCheat6",
        "NativeCheat7",
        "NativeMultipliersLink",
    ] {
        let texts = texts(&s, &block(&s, s.one(name).unwrap(), 1, None));
        assert!(has(&texts, "Multipliers"), "{name}");
        if name != "NativeCheat0" {
            assert!(has(&texts, "Automatic QTE"), "{name}");
        }
    }
    let texts = texts(&s, &block(&s, s.one("NativeCheat7").unwrap(), 1, None));
    assert!(has(
        &texts,
        "[ArrowSelectionL][PauseMenu:optSet6][ArrowSelectionR]"
    ));
}

#[test]
fn real_menu_toggling_scrolled_cheats_updates_their_visible_row() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    for index in 0..8usize {
        let row = s.one(&format!("NativeCheat{index}")).unwrap();
        let instructions = block(&s, row, 3, Some(&toggle_of(row)));
        let slot = (index - usize::from(index >= 6)) as u64;
        let setters: Vec<u64> = operands(&instructions, 0x09)
            .iter()
            .map(|operand| operand & 0xff)
            .collect();
        assert_eq!(setters, [0x11 + slot, 9 + slot], "cheat {index}");
        assert!(operands(&instructions, 0x13).contains(&(0xf10 + index as u64)));
    }
}

#[test]
fn real_menu_six_multipliers_have_native_bars_and_directional_controls() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    let page = s.one("NativeMultipliers").unwrap();
    let queries = operands(&block(&s, page, 1, None), 0x13);
    for index in 0..6u64 {
        assert!(queries.contains(&(0xf20 + index)), "{index}");
        let row = s.one(&format!("NativeMultiplier{index}")).unwrap();
        assert_eq!(row.parent, page.at - 4);
        for (trigger, function) in [(&[0xe6, 0xf2][..], 0xf30), (&[0xe2, 0xee][..], 0xf40)] {
            let calls = operands(&block(&s, row, 3, Some(trigger)), 0x13);
            assert!(calls.contains(&(function + index)), "{index}");
        }
    }
    let objects = objects(&m.data);
    for index in 4..10 {
        let (offset, size) = objects[&format!("gosliderbar{index}")];
        assert_eq!(size, 148);
        assert_eq!(le16(&m.data, offset + 64 + 0x76), index);
    }
}

#[test]
fn real_menu_custom_pages_preserve_the_native_footer() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    for name in native_names(true) {
        let record = s.one(&name).unwrap();
        for b in Script::blocks(record) {
            let code = block(&s, record, b.kind, Some(&b.triggers));
            assert!(
                !code
                    .iter()
                    .any(|op| op.opcode == 0x09 && op.operand == 0xe01d),
                "{name}"
            );
        }
    }
    for text in [
        &b"Confirm: open.\0"[..],
        b"Left/Right: adjust. Changes are saved.\0",
        b"Show the health of the last enemy hit.\0",
    ] {
        assert!(!contains(&m.data, text));
    }
}

#[test]
fn real_menu_multiplier_bars_are_separate_from_numbers_and_original_sliders() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    let (originals, objects) = (objects(&m.original), objects(&m.data));
    let texts = texts(&s, &block(&s, s.one("NativeMultipliers").unwrap(), 1, None));
    for row in 0..6 {
        assert!(has(&texts, &format!("sliderBar{}", row + 4)), "{row}");
        let (offset, _) = objects[&format!("gosliderbar{}", row + 4)];
        // Dedicated controls end left of the original value column, including the selection scale.
        let x = f32_le(&m.data, offset + 64 + 0x64);
        let scale = f32_le(&m.data, offset + 64 + 0x40);
        assert!(x < 403.0, "{row}: {x}");
        assert!(scale < 1.0, "{row}: {scale}");
    }
    for row in 0..4 {
        let name = format!("gosliderbar{row}");
        let ((before, size), (after, _)) = (originals[&name], objects[&name]);
        assert!(
            m.original[before + 64..before + 64 + size] == m.data[after + 64..after + 64 + size],
            "{name}"
        );
    }
}

#[test]
fn real_menu_every_state_reference_and_relative_call_stays_in_the_script() {
    let Some(m) = menu() else {
        return;
    };
    let s = Script::parse(&m.data).unwrap();
    let starts: HashSet<u64> = s.records.iter().map(|r| (r.at - 4) as u64).collect();
    let (first, end) = ((s.base + s.lit) as i64, (s.base + s.symtab) as i64);
    for name in native_names(false) {
        let record = s.one(&name).unwrap();
        for b in Script::blocks(record) {
            for op in block(&s, record, b.kind, Some(&b.triggers)) {
                if op.opcode == 0x47 {
                    assert!(starts.contains(&op.operand), "{name}");
                }
                if op.opcode == 0x4a {
                    let target = op.offset as i64 + 5 + i64::from(op.operand as u32 as i32);
                    assert!(first <= target && target < end, "{name}");
                }
            }
        }
    }
}
