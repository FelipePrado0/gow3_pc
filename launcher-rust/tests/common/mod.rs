//! State and fixture builders the UI tests share. Each test crate uses only some of them.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use iced::widget::text;
use iced::{Element, Font, Size};
use iced_test::simulator::simulator;
use spartanport_launcher::core::gpu::{parse_display_list, Display, GpuRow};
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::core::trophies::TROPHY_COUNT;
use spartanport_launcher::ui::app::{self, Message, State};
use spartanport_launcher::ui::pages::mods::{self, ModsMessage};
use spartanport_launcher::ui::pages::trophies::{self, TrophiesMessage};
use spartanport_launcher::ui::pages::Page;

/// A state over `dir` only, with the splash already gone.
pub fn state(dir: &Path) -> State {
    let root = dir.display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    let mut state = State::new(Paths::from_env(dir, &env));
    state.splash.done = true;
    state
}

pub fn open(state: &mut State, page: Page) {
    let _ = app::update(state, Message::ShowPage(page));
}

/// Clicks `label` in the current view and returns what the click emitted.
pub fn messages(state: &State, label: &str) -> Vec<Message> {
    let mut ui = simulator(app::view(state));
    ui.click(label)
        .unwrap_or_else(|error| panic!("click {label:?}: {error:?}"));
    ui.into_messages().collect()
}

/// Clicks `label` and feeds the emitted messages to `update`.
pub fn click(state: &mut State, label: &str) {
    for message in messages(state, label) {
        let _ = app::update(state, message);
    }
}

/// The size `label` takes on its own, on one line, at `size`: in `font`, or with `None` in the
/// renderer's default font, as a text without `.font(..)` gets it. (An explicit `Font::DEFAULT` is
/// the system's sans-serif instead, whose width differs from one system to another.)
pub fn natural_size(label: &str, font: Option<Font>, size: f32) -> Size {
    let alone = text(label).size(size);
    let alone: Element<'_, Message> = match font {
        Some(font) => alone.font(font).into(),
        None => alone.into(),
    };
    simulator(alone).find(label).unwrap().bounds().size()
}

/// A headless tiny-skia renderer with the simulator's default font, for tests that build a
/// `UserInterface` themselves (to keep its widget state from one view to the next, or to run an
/// operation of their own).
pub fn headless_renderer() -> iced::Renderer {
    use iced::advanced::renderer::Headless;
    iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        Font::with_name("Fira Sans"),
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .unwrap()
}

/// How many containers with `id` the 1280 × 800 window of `view` holds.
pub fn count_ids(view: Element<'_, Message>, id: &iced::widget::Id) -> usize {
    use iced::advanced::widget::Operation;
    use iced_test::runtime::user_interface::{Cache, UserInterface};
    struct Count<'a>(&'a iced::widget::Id, usize);
    impl Operation for Count<'_> {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn container(&mut self, id: Option<&iced::widget::Id>, _bounds: iced::Rectangle) {
            if id == Some(self.0) {
                self.1 += 1;
            }
        }
    }
    let mut renderer = headless_renderer();
    let mut ui = UserInterface::build(
        view,
        Size::new(1280.0, 800.0),
        Cache::default(),
        &mut renderer,
    );
    let mut count = Count(id, 0);
    ui.operate(&renderer, &mut count);
    count.1
}

/// The folder dialog answered `path` for the setting `key`.
pub fn browsed(state: &mut State, key: &str, path: &Path) {
    let _ = app::update(
        state,
        Message::Browsed(key.into(), Some(path.to_path_buf())),
    );
}

// --- Monitors and graphics cards ---

const TWO_DISPLAYS: &str =
    "0\tMonitor A\t1\t0\t0\t1920\t1080\n1\tMonitor B\t0\t1920\t0\t2560\t1440\n";

pub fn two_displays() -> Vec<Display> {
    parse_display_list(TWO_DISPLAYS)
}

pub type GpuTuple = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

pub const ARC_LAPTOP: [GpuTuple; 2] = [
    ("0", "Intel(R) Arc(TM) Graphics", "integrated", "off", "0"),
    (
        "1",
        "NVIDIA GeForce RTX 4060 Laptop GPU",
        "discrete",
        "on",
        "1",
    ),
];

/// Rows as `spartanport-gpu-capabilities --list` prints them: index, name, type, default, pick.
pub fn gpu_rows(rows: &[GpuTuple]) -> Vec<GpuRow> {
    rows.iter()
        .map(|(index, name, kind, msaa, chosen)| GpuRow {
            index: index.to_string(),
            name: name.to_string(),
            kind: kind.to_string(),
            msaa: (!msaa.is_empty()).then(|| msaa.to_string()),
            chosen: *chosen == "1",
        })
        .collect()
}

// --- Game folder, patches and mods ---

pub fn sfo(entries: &[(&str, &str)]) -> Vec<u8> {
    let keys_off = 20 + 16 * entries.len();
    let (mut keys, mut values, mut table) = (Vec::new(), Vec::new(), Vec::new());
    for (key, value) in entries {
        let mut raw = value.as_bytes().to_vec();
        raw.push(0);
        table.extend_from_slice(&(keys.len() as u16).to_le_bytes());
        table.extend_from_slice(&0x204u16.to_le_bytes());
        table.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        table.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        table.extend_from_slice(&(values.len() as u32).to_le_bytes());
        keys.extend_from_slice(key.as_bytes());
        keys.push(0);
        values.extend_from_slice(&raw);
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

/// A dump with `eboot.bin` and a tiny `param.sfo`, plus a test patch file under the port folder.
pub fn dump_with_patches(port: &Path) -> PathBuf {
    let dump = port.join("dump");
    std::fs::create_dir_all(dump.join("sce_sys")).unwrap();
    std::fs::write(dump.join("eboot.bin"), b"elf").unwrap();
    std::fs::write(
        dump.join("sce_sys").join("param.sfo"),
        sfo(&[
            ("TITLE_ID", "CUSA01623"),
            ("TITLE", "God of War III"),
            ("APP_VER", "01.02"),
        ]),
    )
    .unwrap();
    std::fs::create_dir_all(port.join("patches")).unwrap();
    std::fs::write(
        port.join("patches").join("CUSA01623.xml"),
        "<Patch><TitleID><ID>CUSA01623</ID></TitleID>\
         <Metadata Name=\"Fix Alpha\" Author=\"Ann\" Note=\"Fixes alpha.\" AppVer=\"01.02\" isEnabled=\"true\"><PatchList/></Metadata>\
         <Metadata Name=\"Resolution Patch - 3440x1440\" AppVer=\"01.02\"><PatchList/></Metadata>\
         <Metadata Name=\"Fix Other\" AppVer=\"01.01\" isEnabled=\"true\"><PatchList/></Metadata></Patch>",
    )
    .unwrap();
    dump
}

pub fn write_mod_patch(folder: &Path, size: &str) {
    std::fs::write(
        folder.join("u.xml"),
        format!(
            "<Patch><TitleID><ID>CUSA01623</ID></TitleID>\
             <Metadata Name=\"Resolution Patch - {size}\" AppVer=\"01.02\"><PatchList/></Metadata></Patch>"
        ),
    )
    .unwrap();
}

pub fn make_mod(state: &State, name: &str, file: &str) {
    let folder = state.paths.mods_dir("").join(name).join("EXEC");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(file), b"data").unwrap();
}

/// What the background read of the Mods page delivers.
pub fn load_mods(state: &mut State) {
    let view = mods::load(&state.paths, &state.settings);
    let _ = app::update(state, Message::Mods(ModsMessage::Loaded(Box::new(view))));
}

// --- Trophies ---

/// A 4x4 solid PNG.
pub fn tiny_png(shade: u8) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([200, shade, 30, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// A `trophy00.trp` with `TROP000.PNG`... as the core reader expects it.
pub fn trp_package(count: usize) -> Vec<u8> {
    let element = 0x30;
    let table_end = 0x60 + element * count;
    let mut file = vec![0u8; table_end];
    file[..4].copy_from_slice(&[0xdc, 0xa2, 0x4d, 0x00]);
    file[0x10..0x14].copy_from_slice(&(count as u32).to_be_bytes());
    file[0x14..0x18].copy_from_slice(&(element as u32).to_be_bytes());
    for index in 0..count {
        let png = tiny_png(index as u8 * 5);
        let at = 0x60 + index * element;
        let name = format!("TROP{index:03}.PNG");
        file[at..at + name.len()].copy_from_slice(name.as_bytes());
        let offset = file.len() as u64;
        file[at + 0x20..at + 0x28].copy_from_slice(&offset.to_be_bytes());
        file[at + 0x28..at + 0x30].copy_from_slice(&(png.len() as u64).to_be_bytes());
        file.extend(png);
    }
    file
}

/// Puts a 35-trophy package in `game` and makes it the game folder; the saves unlock trophies
/// 0 and 3.
pub fn add_trophies(state: &mut State, game: &Path) {
    std::fs::create_dir_all(game.join("sce_sys/trophy")).unwrap();
    std::fs::write(
        game.join("sce_sys/trophy/trophy00.trp"),
        trp_package(TROPHY_COUNT),
    )
    .unwrap();
    let _ = app::update(
        state,
        Message::Set("game_dir".into(), game.display().to_string().into()),
    );
    let user = state.paths.user_dir("");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("trophies.dat"), [0b1001u8, 0, 0, 0]).unwrap();
}

/// A state whose game folder holds a 35-trophy package and whose saves unlock trophies 0 and 3.
pub fn state_with_trophies(dir: &Path) -> State {
    let mut state = state(dir);
    state.lang = "en".into();
    add_trophies(&mut state, &dir.join("game"));
    state
}

/// What the background read of the Trophies page delivers.
pub fn load_trophies(state: &mut State) {
    let view = trophies::load(&state.paths, &state.settings);
    let _ = app::update(
        state,
        Message::Trophies(TrophiesMessage::Loaded(Box::new(view))),
    );
}
