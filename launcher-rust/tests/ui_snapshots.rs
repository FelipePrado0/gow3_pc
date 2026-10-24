#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Snapshots (spec 7.2): the splash, Home and every drawer page at 1280 × 800 and 1920 × 1080, in
//! English, Portuguese and Arabic, against `snapshots/<shot>-<size>-<lang>.png`.
//!
//! `Simulator::snapshot` always renders at scale 2, so each render is averaged down to the logical
//! size (the mean of each 2 × 2 block) before it is compared or saved. Two images match when at
//! most 0.5 % of the pixels differ by more than 8 in some channel, which absorbs small font
//! rendering differences. A missing reference is written and fails the test, so the first run
//! creates them all.
//!
//! The Home art layer shows a small solid placeholder instead of the key art (`placeholder_art`),
//! which keeps the references small.
//!
//! The snapshots are drawn by tiny-skia, which gives the same pixels on any machine and loads the
//! images before it draws. wgpu depends on the graphics card, and decodes images on a worker thread
//! whose result a headless snapshot does not wait for.
//!
//! The comparison runs on Windows only: elsewhere the monospace font of the log and the Arabic
//! glyphs come from other system fonts. There each render is only checked for its size and for not
//! being blank.

mod common;

use std::borrow::Cow;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use common::{
    add_trophies, browsed, dump_with_patches, gpu_rows, load_mods, load_trophies, make_mod, open,
    two_displays, write_mod_patch, ARC_LAPTOP,
};
use iced::widget::image::Handle;
use iced::Size;
use iced_test::core::Settings;
use iced_test::simulator::Simulator;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::ui::app::{self, Message, State};
use spartanport_launcher::ui::home;
use spartanport_launcher::ui::lang_menu::LangMessage;
use spartanport_launcher::ui::pages::Page;
use spartanport_launcher::ui::persist;
use spartanport_launcher::ui::setup::{SetupState, Step};
use spartanport_launcher::ui::theme;

const SIZES: [(u32, u32); 2] = [(1280, 800), (1920, 1080)];
const LANGUAGES: [&str; 3] = ["en", "pt", "ar"];

/// A pixel differs when one of its channels is off by more than this.
const CHANNEL_TOLERANCE: u8 = 8;
/// Images match while at most 1 pixel in this many differs (0.5 %).
const PIXELS_PER_DIFFERENCE: usize = 200;

/// Where the clock of every snapshot stands after the state was built: the drawer has long finished
/// sliding, and the 2.8 s heat pulse of the Home background is at its peak (21¼ periods).
const CLOCK: Duration = Duration::from_millis(59_500);

/// The port folder the window shows. The fixture lives in a temporary folder whose name changes on
/// every run; the pages show this one instead, so the images are the same on every run.
const SHOWN_PORT: &str = if cfg!(windows) {
    r"C:\SpartanPort"
} else {
    "/opt/spartanport"
};

/// A third-party patch file for the fixture's game version.
const EXTERNAL_PATCH: &str = "<Patch><TitleID><ID>CUSA01623</ID></TitleID>\
     <Metadata Name=\"Sharper Shadows\" Author=\"Bo\" AppVer=\"01.02\" isEnabled=\"true\"><PatchList/></Metadata></Patch>";

/// What a snapshot shows: the splash over Home, a page (Home is the page without the drawer), or a
/// step of the first-run setup.
#[derive(Debug, Clone, Copy)]
enum Shot {
    Splash,
    Page(Page),
    Setup(Step),
}

impl Shot {
    fn name(self) -> String {
        match self {
            Shot::Splash => "splash".to_string(),
            Shot::Page(page) => page.label_en().to_lowercase(),
            Shot::Setup(step) => format!("setup{}", step.number()),
        }
    }
}

/// The background reads the app starts (a new game folder, opening Trophies or Mods), delivered.
fn finish_reads(state: &mut State) {
    load_trophies(state);
    load_mods(state);
}

/// The window over a fixture: a game folder with its `param.sfo` and a 35-trophy package (0 and 3
/// unlocked), the port's patch file and one third-party patch, two mods (one adds 3840 × 1080), two
/// monitors, two graphics cards and two log lines. `lang` is picked in the language menu, `shot`'s
/// page is open, and the clock stands at `CLOCK`; nothing has ticked, so the fire has no particle.
fn fixture(dir: &Path, shot: Shot, size: Size, lang: &str) -> State {
    let dump = dump_with_patches(dir);
    std::fs::write(dir.join("patches").join("Community.xml"), EXTERNAL_PATCH).unwrap();
    let mut state = common::state(dir);
    make_mod(&state, "HD Textures", "textures.bin");
    let ultrawide = state.paths.mods_dir("").join("Ultrawide").join("patches");
    std::fs::create_dir_all(&ultrawide).unwrap();
    write_mod_patch(&ultrawide, "3840x1080");
    add_trophies(&mut state, &dump);
    browsed(&mut state, "game_dir", &dump);
    finish_reads(&mut state);
    let _ = app::update(&mut state, Message::MonitorsListed(two_displays()));
    state.gpus = gpu_rows(&ARC_LAPTOP);
    state
        .log
        .push_text("Loading CUSA01623 v01.02\nVulkan device: NVIDIA GeForce RTX 4060 Laptop GPU\n");
    let _ = app::update(&mut state, Message::Resized(size));
    let _ = app::update(
        &mut state,
        Message::Lang(LangMessage::Pick(lang.to_string())),
    );
    match shot {
        // The whole splash: the monogram only flies off on a clock tick, and none is sent.
        Shot::Splash => state.splash.done = false,
        Shot::Page(page) => {
            open(&mut state, page);
            finish_reads(&mut state);
        }
        Shot::Setup(_) => {}
    }
    state.now = state.splash.started + CLOCK;
    show_fixed_paths(&mut state);
    if let Shot::Setup(step) = shot {
        let mut setup = SetupState::new(&state);
        setup.step = step;
        state.setup = Some(setup);
    }
    state.art_override = Some(placeholder_art());
    state
}

/// The Home art of the references: a 32 × 18 dark warm grey (`#2a1f1f`) instead of the key art,
/// whose photograph would make every reference several times larger. It goes through the same
/// layer: cover, 70 %, the 70 % / 30 % anchor and the clip above the tab bar.
fn placeholder_art() -> home::Art {
    const SIZE: (u32, u32) = (32, 18);
    let pixels = [0x2a, 0x1f, 0x1f, 0xff].repeat((SIZE.0 * SIZE.1) as usize);
    home::Art {
        handle: Handle::from_rgba(SIZE.0, SIZE.1, pixels),
        size: SIZE,
    }
}

/// Shows `SHOWN_PORT` (and its `dump` as the game folder) instead of the temporary folder; what
/// the pages read from the disk is already in the state. Nothing counts as edited afterwards.
fn show_fixed_paths(state: &mut State) {
    let port = Path::new(SHOWN_PORT);
    state.paths = Paths::from_env(port, &|_| None);
    state
        .settings
        .set("game_dir", port.join("dump").display().to_string().into());
    state.saved = Some(persist::gather(state));
}

static TINY_SKIA: Once = Once::new();

/// Draws the window at `width` × `height` and averages the 2× render down to that size.
fn render(state: &State, width: u32, height: u32) -> RgbaImage {
    // Read by `Simulator::with_size`. Set once, before any simulator of this binary exists.
    TINY_SKIA.call_once(|| std::env::set_var("ICED_TEST_BACKEND", "tiny-skia"));
    let settings = Settings {
        fonts: theme::font_bytes().map(Cow::Borrowed).to_vec(),
        ..Settings::default()
    };
    let size = Size::new(width as f32, height as f32);
    let mut ui = Simulator::with_size(settings, size, app::view(state));
    let snapshot = ui.snapshot(&theme::theme()).unwrap();
    // `Snapshot` keeps its pixels private; `matches_image` writes them to a missing file (named
    // after the renderer: `render-tiny-skia.png`).
    let scratch = tempfile::tempdir().unwrap();
    assert!(snapshot
        .matches_image(scratch.path().join("render.png"))
        .unwrap());
    let written = std::fs::read_dir(scratch.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    downsample(&image::open(written).unwrap().to_rgba8())
}

/// Each pixel is the mean of its 2 × 2 block, per channel.
fn downsample(full: &RgbaImage) -> RgbaImage {
    let row = full.width() as usize * 4;
    let mut out = Vec::with_capacity(full.as_raw().len() / 4);
    for rows in full.as_raw().chunks_exact(2 * row) {
        let (upper, lower) = rows.split_at(row);
        for (top, bottom) in upper.chunks_exact(8).zip(lower.chunks_exact(8)) {
            for channel in 0..4 {
                let sum = u32::from(top[channel])
                    + u32::from(top[channel + 4])
                    + u32::from(bottom[channel])
                    + u32::from(bottom[channel + 4]);
                out.push(((sum + 2) / 4) as u8);
            }
        }
    }
    RgbaImage::from_raw(full.width() / 2, full.height() / 2, out).unwrap()
}

/// `Ok` when the images have the same size and at most 0.5 % of the pixels differ by more than
/// `CHANNEL_TOLERANCE` in some channel.
fn compare(actual: &RgbaImage, reference: &RgbaImage) -> Result<(), String> {
    if actual.dimensions() != reference.dimensions() {
        return Err(format!(
            "size {:?}, reference {:?}",
            actual.dimensions(),
            reference.dimensions()
        ));
    }
    let differing = actual
        .pixels()
        .zip(reference.pixels())
        .filter(|(a, b)| {
            a.0.iter()
                .zip(b.0)
                .any(|(x, y)| x.abs_diff(y) > CHANNEL_TOLERANCE)
        })
        .count();
    let total = actual.pixels().len();
    if differing * PIXELS_PER_DIFFERENCE <= total {
        Ok(())
    } else {
        Err(format!(
            "{differing} of {total} pixels differ ({:.2} %)",
            differing as f64 * 100.0 / total as f64
        ))
    }
}

/// Writes a reference: RGB only (the window is opaque) and compressed hard, for a small repository.
fn save_reference(image: &RgbaImage, path: &Path) {
    let rgb: Vec<u8> = image
        .pixels()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    PngEncoder::new_with_quality(file, CompressionType::Best, FilterType::Adaptive)
        .write_image(&rgb, image.width(), image.height(), ExtendedColorType::Rgb8)
        .unwrap();
}

/// Compares `image` with the reference at `path`, writing it when missing (which fails).
fn matches_reference(image: &RgbaImage, path: &Path) -> Result<(), String> {
    if !path.exists() {
        save_reference(image, path);
        return Err(format!("reference created: {}", path.display()));
    }
    let reference = image::open(path).unwrap().to_rgba8();
    compare(image, &reference).map_err(|why| format!("{}: {why}", path.display()))
}

fn snapshots_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("snapshots")
}

static COMPARE_SKIPPED: Once = Once::new();

/// Renders `shot` at every size and language and checks each render; the failures are reported
/// together, so a first run writes all the references.
fn check(shot: Shot) {
    let mut failures = Vec::new();
    for (width, height) in SIZES {
        for lang in LANGUAGES {
            let dir = tempfile::tempdir().unwrap();
            let state = fixture(
                dir.path(),
                shot,
                Size::new(width as f32, height as f32),
                lang,
            );
            let image = render(&state, width, height);
            let name = format!("{}-{width}x{height}-{lang}.png", shot.name());
            assert_eq!(image.dimensions(), (width, height), "{name}");
            let first = *image.get_pixel(0, 0);
            assert!(
                image.pixels().any(|pixel| *pixel != first),
                "{name} is blank"
            );
            if !cfg!(windows) {
                COMPARE_SKIPPED.call_once(|| {
                    let _ = writeln!(
                        std::io::stderr(),
                        "ui_snapshots: pixel comparison skipped (references are made on Windows)"
                    );
                });
                continue;
            }
            if let Err(failure) = matches_reference(&image, &snapshots_dir().join(&name)) {
                failures.push(failure);
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn splash() {
    check(Shot::Splash);
}

#[test]
fn home() {
    check(Shot::Page(Page::Home));
}

#[test]
fn graphics() {
    check(Shot::Page(Page::Graphics));
}

#[test]
fn performance() {
    check(Shot::Page(Page::Performance));
}

#[test]
fn game() {
    check(Shot::Page(Page::Game));
}

#[test]
fn trophies() {
    check(Shot::Page(Page::Trophies));
}

#[test]
fn controls() {
    check(Shot::Page(Page::Controls));
}

#[test]
fn mods() {
    check(Shot::Page(Page::Mods));
}

#[test]
fn advanced() {
    check(Shot::Page(Page::Advanced));
}

#[test]
fn setup_folders() {
    check(Shot::Setup(Step::Folders));
}

#[test]
fn setup_gpu() {
    check(Shot::Setup(Step::Gpu));
}

#[test]
fn setup_video() {
    check(Shot::Setup(Step::Video));
}

#[test]
fn log() {
    check(Shot::Page(Page::Log));
}

#[test]
fn the_home_glows_fall_off_without_bands() {
    // Ruling R80: the glows are a soft sprite. Concentric discs showed as rings, a jump of several
    // levels at each disc's edge; a smooth falloff moves a level or two between neighbours.
    let (width, height) = (1280, 800);
    let dir = tempfile::tempdir().unwrap();
    let size = Size::new(width as f32, height as f32);
    let state = fixture(dir.path(), Shot::Page(Page::Home), size, "en");
    let image = render(&state, width, height);
    // A row through the heat blobs along the floor, left of the Home block.
    let y = 690;
    let jumps: Vec<(u32, u8)> = (100..560)
        .map(|x| {
            let (a, b) = (image.get_pixel(x, y).0, image.get_pixel(x + 1, y).0);
            (x, (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap())
        })
        .filter(|(_, step)| *step > 2)
        .collect();
    assert!(
        jumps.is_empty(),
        "steps of more than 2 levels (x, step): {jumps:?}"
    );
}

#[test]
fn the_splash_halo_hugs_the_monogram() {
    // Ruling R80: the halo is the glows' soft sprite, which falls off without a step (a flat disc
    // showed as a ball). Ruling R87: like the mockup's `#slogo` drop shadow (26 px of blur), it
    // ends within 40 px of the 200 px monogram instead of reaching hundreds of pixels out.
    let (width, height) = (1280, 800);
    let dir = tempfile::tempdir().unwrap();
    let size = Size::new(width as f32, height as f32);
    let state = fixture(dir.path(), Shot::Splash, size, "en");
    let image = render(&state, width, height);
    // The monogram is 200 px, centred 30 px above the middle of the scene (the window less the
    // 52 px bar); the row through its centre, outwards from beside its letters.
    let (centre_x, centre_y) = (width / 2, (height - 52) / 2 - 30);
    let red = |dx: u32| image.get_pixel(centre_x + dx, centre_y)[0];
    let far = red(460);
    assert!(
        red(90) > far + 15,
        "no halo: {} beside the letters, {far} far out",
        red(90)
    );
    let falloff: Vec<u8> = (101..141).map(red).collect();
    for (inner, outer) in falloff.iter().zip(&falloff[1..]) {
        assert!(
            *outer <= inner + 1 && inner.saturating_sub(*outer) <= 2,
            "not a smooth falloff: {falloff:?}"
        );
    }
    let beyond: Vec<(u32, u8)> = (141..460)
        .map(|dx| (dx, red(dx)))
        .filter(|(_, value)| value.abs_diff(far) > 1)
        .collect();
    assert!(
        beyond.is_empty(),
        "the halo reaches past 40 px from the monogram (dx, red): {beyond:?}"
    );
}

#[test]
fn the_home_block_behind_a_wide_drawer_has_no_shade_box() {
    // Ruling R79: the block fades through its own colours. A shade over its bounds, darker than
    // the dim layer around it, showed as a black box.
    let (width, height) = (1920, 1080);
    let dir = tempfile::tempdir().unwrap();
    let size = Size::new(width as f32, height as f32);
    let state = fixture(dir.path(), Shot::Page(Page::Log), size, "en");
    let image = render(&state, width, height);
    let settings = Settings {
        fonts: theme::font_bytes().map(Cow::Borrowed).to_vec(),
        ..Settings::default()
    };
    let block = Simulator::with_size(settings, size, app::view(&state))
        .find(iced_test::selector::id(home::BLOCK_ID))
        .unwrap()
        .bounds();
    // Right of the drawer and inside the block (whose width follows its chips), above and below
    // its top edge, left of the logotype.
    let x = ((0.72 * width as f32).ceil() + 8.0).max(block.x + 8.0);
    assert!(block.x < x && x < block.x + block.width, "{block:?}");
    let above = image.get_pixel(x as u32, block.y as u32 - 4).0;
    let inside = image.get_pixel(x as u32, block.y as u32 + 4).0;
    assert!(
        above.iter().zip(inside).all(|(a, b)| a.abs_diff(b) <= 3),
        "above the block {above:?}, inside it {inside:?}"
    );
}

#[test]
fn the_key_art_stays_above_the_tab_bar() {
    // At 1920 × 1080 the scene is wider than the art, so `cover` overflows it by 36 px; the bar
    // below keeps its own colour where it has no text.
    let (width, height) = (1920, 1080);
    let dir = tempfile::tempdir().unwrap();
    let size = Size::new(width as f32, height as f32);
    let mut state = fixture(dir.path(), Shot::Page(Page::Home), size, "en");
    // The real key art: this test writes no reference.
    state.art_override = None;
    let image = render(&state, width, height);
    let panel = theme::palette::PANEL.into_rgba8();
    for (x, y) in [(1000, 1032), (1500, 1040), (1500, 1060), (1000, 1075)] {
        assert_eq!(image.get_pixel(x, y).0, panel, "({x}, {y})");
    }
}
