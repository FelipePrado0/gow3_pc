#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Parity tests: the runner must write what the Python modules wrote into `fixtures/runner/`
//! (`tools/runner_fixtures.py`).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use spartanport_launcher::core::draw;
use spartanport_launcher::core::patches::{write_game_patches, PatchJob};
use spartanport_launcher::runner::content_profile::{self, Sku};
use spartanport_launcher::runner::graphics_options;
use spartanport_launcher::runner::isa_rewrite::{self, Site};
use spartanport_launcher::runner::link_modules::DEFAULT_MODULES;
use spartanport_launcher::runner::prepare::prepare;
use spartanport_launcher::runner::{dxt, glyphs};
use spartanport_launcher::runner::{link_libc, link_modules};

fn fixture_dir(case: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/runner")).join(case)
}

/// The bytes of `expected/<name>` in `case`.
fn expected(case: &str, name: &str) -> Vec<u8> {
    fs::read(fixture_dir(case).join("expected").join(name)).unwrap()
}

/// `text` with the temporary folder `dir` spelled `<DATA_DIR>`, as the generator wrote it.
fn with_placeholder(text: &str, dir: &Path) -> String {
    let plain = dir.display().to_string();
    let escaped = plain.replace('\\', "\\\\");
    text.replace(&escaped, "<DATA_DIR>")
        .replace(&plain, "<DATA_DIR>")
}

/// Fails at the first differing byte instead of printing both files.
#[track_caller]
fn assert_same_bytes(actual: &[u8], expected: &[u8], name: &str) {
    if let Some(at) = actual.iter().zip(expected).position(|(a, b)| a != b) {
        panic!(
            "{name}: byte {at:#x} is {:#04x}, the Python wrote {:#04x}",
            actual[at], expected[at]
        );
    }
    assert_eq!(actual.len(), expected.len(), "{name}: length");
}

#[test]
fn prepare_matches_python() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    prepare(&fixture_dir("prepare").join("input"), &out).unwrap();
    for name in ["boot.bin", "eboot.elf", "entry.bin"] {
        let actual = fs::read(out.join(name)).unwrap();
        assert_same_bytes(&actual, &expected("prepare", name), name);
    }
    let analysis = String::from_utf8(fs::read(out.join("analysis.json")).unwrap()).unwrap();
    let expected = String::from_utf8(expected("prepare", "analysis.json")).unwrap();
    assert_eq!(with_placeholder(&analysis, dir.path()), expected);
}

#[test]
fn link_matches_python() {
    let dir = tempfile::tempdir().unwrap();
    let game = fixture_dir("link").join("input");
    let out = dir.path().join("out");
    fs::create_dir_all(&out).unwrap();
    fs::copy(game.join("boot.bin"), out.join("boot.bin")).unwrap();
    link_libc::link(&game, &out).unwrap();
    link_modules::link(&game, &out, &DEFAULT_MODULES).unwrap();
    for name in ["boot-libc.bin", "boot-linked.bin"] {
        let actual = fs::read(out.join(name)).unwrap();
        assert_same_bytes(&actual, &expected("link", name), name);
    }
    for name in ["libc-link.json", "link.json"] {
        let actual = String::from_utf8(fs::read(out.join(name)).unwrap()).unwrap();
        let expected = String::from_utf8(expected("link", name)).unwrap();
        assert_eq!(with_placeholder(&actual, dir.path()), expected, "{name}");
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `site` in the shape of `expected/sites.json` (the generator's `site_entry`): kind and mode by
/// the names of the Python's `KIND_*` and `MODE_*`.
fn site_entry(site: &Site) -> Value {
    const KINDS: [&str; 11] = [
        "MOVBE_LOAD",
        "MOVBE_STORE",
        "LZCNT",
        "TZCNT",
        "ANDN",
        "BEXTR",
        "BLSI",
        "BLSMSK",
        "BLSR",
        "VCVTPH2PS",
        "VCVTPS2PH",
    ];
    const MODES: [&str; 4] = ["JMP", "JMP_RELOCATE", "INT3", "UNSUPPORTED"];
    json!({
        "offset": site.offset,
        "length": site.length,
        "kind": KINDS[usize::from(site.kind.as_u8()) - 1],
        "width": site.width,
        "mode": MODES[site.mode as usize],
        "registers": {
            "dst": site.dst,
            "src1": site.src1,
            "src2": site.src2,
            "base": site.base,
            "index": site.index,
        },
        "mem_slot": site.mem_slot,
        "scale": site.scale,
        "mem_flags": site.mem_flags,
        "disp": site.disp,
        "original": hex(&site.original[..usize::from(site.length)]),
        "relocated": hex(&site.relocated),
    })
}

#[test]
fn isa_table_matches_python() {
    let dir = tempfile::tempdir().unwrap();
    let boot = fixture_dir("isa").join("input").join("boot-linked.bin");
    let sites = isa_rewrite::rewrite(&boot, dir.path()).unwrap();
    let actual = fs::read(dir.path().join("isa-rewrite.bin")).unwrap();
    assert_same_bytes(
        &actual,
        &expected("isa", "isa-rewrite.bin"),
        "isa-rewrite.bin",
    );
    // The sites `rewrite` wrote, which the generator took from `scan_segment` over the one
    // executable segment.
    let entries = sites.iter().map(site_entry).collect();
    let expected: Value = serde_json::from_slice(&expected("isa", "sites.json")).unwrap();
    assert_eq!(Value::Array(entries), expected);
}

#[test]
fn content_matches_python() {
    for (name, sku) in [("full", Sku::Full), ("trial", Sku::Trial)] {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        fs::create_dir_all(game.join("sce_sys")).unwrap();
        fs::copy(
            fixture_dir("content").join("input").join("param.sfo"),
            game.join("sce_sys").join("param.sfo"),
        )
        .unwrap();
        let out = dir.path().join("out");
        content_profile::prepare(&game, &out, sku).unwrap();
        for file in ["content.bin", "content-profile.json"] {
            let actual = fs::read(out.join(file)).unwrap();
            let expected = fs::read(
                fixture_dir("content")
                    .join("expected")
                    .join(name)
                    .join(file),
            )
            .unwrap();
            assert_same_bytes(&actual, &expected, &format!("{name}/{file}"));
        }
    }
}

#[test]
fn patches_match_python() {
    // As the generator ran patches.py: the game folder (eboot.bin, param.sfo), out/eboot.elf, the
    // case's patches.xml as the built-in file, mod.xml as a mod's, SPARTAN_PATCHES_ONLY=1.
    let input = fixture_dir("patches").join("input");
    let dir = tempfile::tempdir().unwrap();
    let game = dir.path().join("game");
    fs::create_dir_all(game.join("sce_sys")).unwrap();
    fs::copy(input.join("eboot.bin"), game.join("eboot.bin")).unwrap();
    fs::copy(
        input.join("param.sfo"),
        game.join("sce_sys").join("param.sfo"),
    )
    .unwrap();
    let builtin = dir.path().join("patches");
    fs::create_dir_all(&builtin).unwrap();
    fs::copy(input.join("patches.xml"), builtin.join("CUSA01623.xml")).unwrap();
    let out = dir.path().join("out");
    fs::create_dir_all(&out).unwrap();
    fs::copy(input.join("eboot.elf"), out.join("eboot.elf")).unwrap();
    let extra: Vec<String> = ["Found", "Call", "Missing", "Data"]
        .map(String::from)
        .to_vec();
    let mods = [input.join("mod.xml")];
    let mut log = Vec::new();
    let job = PatchJob {
        out: &out,
        game: &game,
        builtin_dir: &builtin,
        mod_patches: &mods,
        extra: &extra,
        only: true,
        patches_dir: None,
        patches_config: None,
    };
    write_game_patches(&job, &mut log).unwrap();
    for name in ["patches-merged.xml", "patches.bin", "patch_status.json"] {
        let actual = fs::read(out.join(name)).unwrap();
        assert_same_bytes(&actual, &expected("patches", name), name);
    }
    // What patches.py printed for this run.
    assert_eq!(
        String::from_utf8(log).unwrap(),
        "Patches: Resolution Patch - 3440x1440 from the mod file patches/mod.xml\n\
         Patches: Resolution Patch - 2560x1080 from the mod file patches/mod.xml\n\
         Patches: Skip Any Video With X Button from the mod file patches/mod.xml\n\
         Patches: CUSA01623 01.02, eboot.bin sha256 \
         ffed585c6a2b513546eba7f283c45ad27382a8e31cf54fe03b4e1c0ca63195d5\n\
         Patches: Missing skipped (signature not found in this eboot.bin)\n\
         Patches: CUSA01623; 3 writes from ['Found', 'Call', 'Data']\n"
    );
}

#[test]
fn graphics_matches_python() {
    // As the generator ran graphics_options: prepare three times; before the second the in-game
    // menu wrote input/edit.json (a restart request) into the ini.
    let input = fixture_dir("graphics").join("input");
    let dir = tempfile::tempdir().unwrap();
    let ini = dir.path().join("spartanport.ini");
    let state = dir.path().join("graphics_state.json");
    fs::copy(input.join("spartanport.ini"), &ini).unwrap();
    fs::copy(input.join("graphics_state.json"), &state).unwrap();
    let edit: Value = serde_json::from_slice(&fs::read(input.join("edit.json")).unwrap()).unwrap();
    let edit: indexmap::IndexMap<String, String> = edit
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_string()))
        .collect();
    for step in ["good", "tried", "reverted"] {
        if step == "tried" {
            graphics_options::write_keys(&ini, &edit).unwrap();
        }
        let prepared = graphics_options::prepare(&ini, &state, &[]);
        let environment = graphics_options::environment(&prepared.options);
        let mut environment_json = serde_json::to_string_pretty(&environment).unwrap();
        environment_json.push('\n');
        for (name, actual) in [
            ("spartanport.ini", fs::read(&ini).unwrap()),
            ("graphics_state.json", fs::read(&state).unwrap()),
            ("environment.json", environment_json.into_bytes()),
        ] {
            assert_same_bytes(
                &actual,
                &expected("graphics", &format!("{step}/{name}")),
                &format!("{step}/{name}"),
            );
        }
    }
}

/// The PNGs of case `dxt` as RGBA, by name.
fn dxt_images() -> Vec<(String, image::RgbaImage)> {
    let mut paths: Vec<PathBuf> = fs::read_dir(fixture_dir("dxt").join("input"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    let images: Vec<(String, image::RgbaImage)> = paths
        .iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_str().unwrap().to_string();
            (name, image::open(path).unwrap().to_rgba8())
        })
        .collect();
    let names: Vec<&str> = images.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        ["alpha", "edges", "flat", "gradient", "noise", "odd"]
    );
    images
}

/// As `glyphs.encode_region` encodes a picture: each 4-row strip on its own (the last one shorter
/// when the height is not a multiple of 4), the blocks of the strips one after the other.
fn strips(image: &image::RgbaImage, encode: fn(&[u8], u32, u32) -> Vec<u8>) -> Vec<u8> {
    let (width, height) = image.dimensions();
    let row = width as usize * 4;
    let mut blocks = Vec::new();
    for y in (0..height).step_by(4) {
        let rows = (height - y).min(4);
        let start = y as usize * row;
        let strip = &image.as_raw()[start..start + rows as usize * row];
        blocks.extend(encode(strip, width, rows));
    }
    blocks
}

#[test]
fn dxt1_blocks_match_pillow() {
    for (name, image) in dxt_images() {
        let name = format!("{name}.dxt1");
        let actual = strips(&image, dxt::encode_dxt1);
        assert_same_bytes(&actual, &expected("dxt", &name), &name);
    }
}

#[test]
fn dxt5_blocks_match_pillow() {
    for (name, image) in dxt_images() {
        let name = format!("{name}.dxt5");
        let actual = strips(&image, dxt::encode_dxt5);
        assert_same_bytes(&actual, &expected("dxt", &name), &name);
    }
}

/// A PNG of the fixtures as a picture of its own mode (RGB or RGBA).
fn picture_of(path: &Path) -> draw::Picture {
    let image = image::open(path).unwrap();
    match image {
        image::DynamicImage::ImageRgb8(rgb) => draw::Picture::from_rgb(&rgb),
        other => draw::Picture::from_rgba(&other.to_rgba8()),
    }
}

/// The peak signal-to-noise ratio of `actual` against `expected` over columns `x0..x1` and rows
/// `y0..y1`, every band; infinite when they are equal.
fn psnr(actual: &draw::Picture, expected: &draw::Picture, region: [usize; 4]) -> f64 {
    assert_eq!(
        (actual.width, actual.height, actual.mode),
        (expected.width, expected.height, expected.mode)
    );
    let [x0, y0, x1, y1] = region;
    let (mut sum, mut count) = (0.0f64, 0usize);
    for y in y0..y1 {
        for x in x0..x1 {
            for (a, b) in actual.pixel(x, y).iter().zip(expected.pixel(x, y)) {
                sum += (f64::from(*a) - f64::from(*b)).powi(2);
                count += 1;
            }
        }
    }
    if sum == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255.0f64 * 255.0 / (sum / count as f64)).log10()
}

#[test]
fn painted_textures_match_pillow_within_tolerance() {
    let port = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let assets = glyphs::Assets::in_port(&port);
    let painter = glyphs::Painter::new(&assets);
    let folder = fixture_dir("glyphs");
    let mut results = Vec::new();
    for style in ["xbox", "nintendo", "keyboard"] {
        let mut names: Vec<PathBuf> = fs::read_dir(folder.join("expected").join(style))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        names.sort();
        let choice = glyphs::Choice {
            style,
            faces: None,
            pad_map: "",
            key_map: "",
        };
        for path in names {
            let id = path.file_stem().unwrap().to_str().unwrap().to_string();
            let expected = picture_of(&path);
            let texture = glyphs::TEXTURES
                .iter()
                .chain(glyphs::KEYBOARD_TEXTURES.iter())
                .find(|(name, _)| *name == id)
                .map(|(_, texture)| *texture)
                .unwrap_or(glyphs::Texture {
                    form: glyphs::Form::Controller,
                    buttons: &[],
                });
            let (painted, region) = if texture.form.is_lyre() {
                let input = picture_of(&folder.join("input").join(format!("{id}.png")));
                let painted =
                    glyphs::lyre_paint(&input, texture.form, texture.buttons[0], &choice, &painter)
                        .unwrap();
                let shrink = 2048 / input.width;
                (painted, glyphs::LYRE_CORNER.map(|v| v / shrink))
            } else {
                let (w, h) = (expected.width, expected.height);
                let painted =
                    glyphs::render(texture.form, texture.buttons, &choice, w, h, &painter).unwrap();
                (painted, [0, 0, w, h])
            };
            results.push((style, id, psnr(&painted, &expected, region)));
        }
    }
    for (style, id, value) in &results {
        println!("PSNR {style} {id}: {value:.2} dB");
    }
    assert_eq!(results.len(), 25 + 1 + 25 + 1 + 22);
    let low: Vec<_> = results
        .iter()
        .filter(|(_, _, value)| *value < 40.0)
        .collect();
    assert!(low.is_empty(), "below 40 dB: {low:?}");
}

#[test]
fn glyph_mip_chains_match_pillow() {
    // `glyphs.encode` of Pillow's own drawing: the top level and every smaller (bicubic) level.
    let folder = fixture_dir("glyphs").join("expected");
    for (style, id, fourcc, extension) in [
        ("xbox", "TX7FF61BF7", glyphs::FourCc::Dxt1, "dxt1"),
        ("keyboard", "TXD43D1443", glyphs::FourCc::Dxt5, "dxt5"),
    ] {
        let picture = picture_of(&folder.join(style).join(format!("{id}.png")));
        let name = format!("encode/{style}/{id}.{extension}");
        let expected = fs::read(folder.join(&name)).unwrap();
        let actual = glyphs::encode(&picture, fourcc, expected.len()).unwrap();
        assert_same_bytes(&actual, &expected, &name);
    }
}

#[test]
fn lyre_corner_blocks_match_pillow() {
    // `glyphs.encode_region` of Pillow's painted glow mask over its input, every level of a
    // texture of 0xee bytes: the same blocks re-encoded (Lanczos levels), the others untouched.
    let folder = fixture_dir("glyphs");
    let id = "TXEACA5E71";
    let original = picture_of(&folder.join("input").join(format!("{id}.png")));
    let painted = picture_of(&folder.join("expected/xbox").join(format!("{id}.png")));
    let name = format!("encode_region/xbox/{id}.dxt1");
    let expected = fs::read(folder.join("expected").join(&name)).unwrap();
    let mut data = vec![0u8; 128];
    data[..4].copy_from_slice(b"DDS ");
    data[12..16].copy_from_slice(&(original.height as u32).to_le_bytes());
    data[16..20].copy_from_slice(&(original.width as u32).to_le_bytes());
    data.resize(expected.len(), 0xee);
    let end = data.len();
    let region = glyphs::LYRE_CORNER.map(|v| v / (2048 / original.width));
    glyphs::encode_region(
        &mut data,
        0,
        end,
        &painted,
        glyphs::FourCc::Dxt1,
        region,
        Some(&original),
    )
    .unwrap();
    assert_same_bytes(&data, &expected, &name);
}
