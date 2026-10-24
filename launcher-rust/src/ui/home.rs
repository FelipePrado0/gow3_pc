//! The Home page: layered background (spec 4.3) and the interactive block over it.

use std::f32::consts::TAU;
use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use iced::mouse;
use iced::widget::canvas::{self, gradient, Geometry, Path as Shape};
use iced::widget::image::Handle;
use iced::widget::text::Span;
use iced::widget::{
    button, canvas as canvas_widget, column, container, image, rich_text, row, span, stack, text,
};
use iced::{
    Bottom, Center, Color, ContentFit, Element, Fill, Font, Left, Padding, Point, Radians,
    Rectangle, Renderer, Right, Size, Theme, Top, Vector,
};

use crate::core::game::{game_info, game_patchable, game_profile_of, meta_folder, GameInfo};
use crate::core::gpu::chosen_gpu;
use crate::core::i18n::{tr, tr_fmt};
use crate::core::options::{short_label, Choice, DISPLAY_MODES, FPS_LIMITS, OUTPUTS};
use crate::core::patches::patch_status;
use crate::ui::app::{
    home_interactive_enabled, home_interactive_opacity, scene_size, Message, State,
};
use crate::ui::lang_menu;
use crate::ui::play::GameState;
use crate::ui::version_menu;

use crate::ui::theme::{
    button_flat, button_play, faded, faded_button, mirror_padding, mirror_x, palette, CINZEL_BLACK,
};
use crate::ui::widgets::{chip, ChipKind};

// --- Background layers (spec 4.3, mockup `launcher-mockup-v13.html`) ---

/// Layer 1: the base gradient, top to bottom.
const BASE_TOP: Color = Color::from_rgb(
    0x1b as f32 / 255.0,
    0x14 as f32 / 255.0,
    0x14 as f32 / 255.0,
);
/// Red glow at (68 %, 38 %): colour, peak alpha, radii as a share of width and height.
const RED_GLOW: (f32, f32, f32) = (200.0 / 255.0, 30.0 / 255.0, 37.0 / 255.0);
const RED_GLOW_AT: (f32, f32) = (0.68, 0.38);
const RED_GLOW_PEAK: f32 = 0.42;
const RED_GLOW_RADII: (f32, f32) = (0.38, 0.46);
/// Grey glow at (22 %, 78 %).
const GREY_GLOW: (f32, f32, f32) = (130.0 / 255.0, 130.0 / 255.0, 130.0 / 255.0);
const GREY_GLOW_AT: (f32, f32) = (0.22, 0.78);
const GREY_GLOW_PEAK: f32 = 0.14;
const GREY_GLOW_RADII: (f32, f32) = (0.25, 0.32);
/// Side of the glow sprite in pixels: the canvas has no radial gradient, so each glow is this
/// soft round picture stretched over its ellipse (ruling R80).
const SPRITE_SIZE: u32 = 256;
/// Layer 2: the launcher's key art (`assets/background.png`, mockup `#photo`): its size in pixels,
/// its opacity, and the point `cover` keeps in place (CSS `background-position: 70% 30%`).
pub const ART_SIZE: (u32, u32) = (1920, 1080);
const ART_OPACITY: f32 = 0.7;
const ART_ANCHOR: (f32, f32) = (0.70, 0.30);
/// The id of the container that holds the key art.
pub const ART_ID: &str = "home-art";
/// Mockup `#quick` and `#status`: their font sizes, and the gap above each.
const QUICK_SIZE: f32 = 13.0;
const HINT_SIZE: f32 = 12.0;
const QUICK_GAP: f32 = 10.0;
/// The id of the container that holds the settings line.
pub const QUICK_ID: &str = "home-quick";
/// The id of the container that holds the block under the logotype's right edge: logotype, PLAY,
/// chips, settings line and status line.
pub const BLOCK_ID: &str = "home-block";
/// Layer 3: uniform dark veil, left-hand darkening and its reach, red rising from the floor.
const VEIL_ALPHA: f32 = 0.32;
const LEFT_VEIL_REACH: f32 = 0.58;
const LEFT_VEIL_STOPS: [(f32, f32); 3] = [(0.0, 0.88), (0.30, 0.45), (0.58, 0.0)];
const BOTTOM_RED: (f32, f32, f32) = (140.0 / 255.0, 16.0 / 255.0, 20.0 / 255.0);
/// Layer 4: the fog quad is 140 % of the width and 70 % of the height, its bottom edge 6 % below
/// the view, and it is turned by -7 degrees about the middle of that edge.
const FOG_WIDTH: f32 = 1.4;
const FOG_HEIGHT: f32 = 0.70;
const FOG_BELOW: f32 = 0.06;
const FOG_ANGLE_DEGREES: f32 = -7.0;
const FOG_RED: (f32, f32, f32, f32) = (110.0 / 255.0, 12.0 / 255.0, 16.0 / 255.0, 0.38);
const FOG_FLOOR_ALPHA: f32 = 0.96;
/// Layer 5: the orange heat pulses once per period between these alphas.
const HEAT_PERIOD: f32 = 2.8;
const HEAT_MIN: f32 = 0.65;
const HEAT_MAX: f32 = 1.0;
/// One heat blob: centre and radii (shares of width and height), colour and peak alpha.
struct HeatBlob {
    centre: (f32, f32),
    radii: (f32, f32),
    colour: (f32, f32, f32),
    peak: f32,
}

/// The three heat blobs along the floor (mockup `#heat`).
const HEAT_BLOBS: [HeatBlob; 3] = [
    HeatBlob {
        centre: (0.52, 1.04),
        radii: (0.45, 0.30),
        colour: (235.0 / 255.0, 80.0 / 255.0, 30.0 / 255.0),
        peak: 0.55,
    },
    HeatBlob {
        centre: (0.18, 1.10),
        radii: (0.20, 0.20),
        colour: (230.0 / 255.0, 90.0 / 255.0, 30.0 / 255.0),
        peak: 0.30,
    },
    HeatBlob {
        centre: (0.84, 0.96),
        radii: (0.30, 0.30),
        colour: (235.0 / 255.0, 90.0 / 255.0, 30.0 / 255.0),
        peak: 0.50,
    },
];

// --- Home block (mockup `#home`, `#topright`, `.chips`, `#quick`) ---

/// Logotype height in the block.
const LOGO_HEIGHT: f32 = 54.0;
/// Corner monogram size and its distance from the edges (the splash flies it to the same place).
const MONOGRAM: f32 = 60.0;
const MONOGRAM_EDGE: f32 = 24.0;
/// PLAY: font size and padding.
const PLAY_SIZE: f32 = 22.0;
const PLAY_PADDING: [f32; 2] = [16.0, 54.0];
/// Mockup `#home`: 40 px from the right; 78 px from the window bottom, less the 52 px tab bar.
const BLOCK_RIGHT: f32 = 40.0;
const BLOCK_BOTTOM: f32 = 26.0;
/// Mockup `#topright`.
const TOP_RIGHT_TOP: f32 = 22.0;
const TOP_RIGHT_RIGHT: f32 = 24.0;

/// What the Home chips need from the disk, read once so `view` does no I/O.
#[derive(Debug, Clone, Default)]
pub struct HomeFacts {
    pub info: Option<GameInfo>,
    pub title_id: Option<String>,
    pub patchable: bool,
    pub saves: bool,
    /// Patches the last start left out (`patch_status.json`).
    pub skipped: usize,
}

/// Re-reads the game folder, the saves folder and the last start's patch report.
pub fn refresh_facts(state: &mut State) {
    let meta = meta_folder(
        &state.settings.get_str("game_dir"),
        &state.settings.get_str("update_dir"),
    );
    let (title_id, _profile) = game_profile_of(&meta);
    let savedata = user_dir(state).join("savedata");
    state.home = HomeFacts {
        info: game_info(&meta),
        title_id,
        patchable: game_patchable(&meta),
        saves: has_files(&savedata),
        skipped: patch_status(&state.paths.data_dir.join("out"))
            .map_or(0, |status| status.skipped.len()),
    };
}

fn user_dir(state: &State) -> std::path::PathBuf {
    state.paths.user_dir(&state.settings.get_str("user_dir"))
}

/// Whether `folder` holds a file at any depth (`save_backup._has_saves`).
fn has_files(folder: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            has_files(&path)
        } else {
            path.is_file()
        }
    })
}

// --- Canvas layers ---

#[derive(Debug, Clone, Copy)]
enum Layer {
    Base,
    Veil,
    Fog,
    /// Seconds since the launcher started.
    Heat(f32),
}

#[derive(Debug, Clone, Copy)]
struct Backdrop(Layer);

fn rgba((r, g, b): (f32, f32, f32), a: f32) -> Color {
    Color::from_rgba(r, g, b, a)
}

fn bg(alpha: f32) -> Color {
    Color {
        a: alpha,
        ..palette::BG
    }
}

/// The glow's alpha at `distance` from its centre (1 at its edge): `(1 - d²)²`, which leaves the
/// edge at 0 with no slope, so nothing marks where the glow ends.
fn falloff(distance: f32) -> f32 {
    let squared = distance * distance;
    if squared >= 1.0 {
        0.0
    } else {
        (1.0 - squared) * (1.0 - squared)
    }
}

/// The RGBA pixels of a soft round glow of `colour`: the colour in every pixel, so a filtered edge
/// does not darken, and the alpha falling off from the centre.
fn glow_pixels(colour: [u8; 3]) -> Vec<u8> {
    let half = SPRITE_SIZE as f32 / 2.0;
    let mut pixels = Vec::with_capacity((4 * SPRITE_SIZE * SPRITE_SIZE) as usize);
    for y in 0..SPRITE_SIZE {
        for x in 0..SPRITE_SIZE {
            let dx = (x as f32 + 0.5 - half) / half;
            let dy = (y as f32 + 0.5 - half) / half;
            let alpha = (falloff(dx.hypot(dy)) * 255.0).round() as u8;
            pixels.extend_from_slice(&[colour[0], colour[1], colour[2], alpha]);
        }
    }
    pixels
}

/// The glow sprite of `colour` (its alpha is ignored), made once per colour and process: a new
/// handle is a new picture for the renderer to upload.
pub fn glow_sprite(colour: Color) -> Handle {
    static SPRITES: Mutex<Vec<([u8; 3], Handle)>> = Mutex::new(Vec::new());
    let [r, g, b, _] = colour.into_rgba8();
    let key = [r, g, b];
    let mut sprites = SPRITES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, handle)) = sprites.iter().find(|(made, _)| *made == key) {
        return handle.clone();
    }
    let handle = Handle::from_rgba(SPRITE_SIZE, SPRITE_SIZE, glow_pixels(key));
    sprites.push((key, handle.clone()));
    handle
}

/// An elliptical glow of `colour` around `centre`: the sprite stretched over the ellipse, at
/// `peak` in its middle.
fn glow(
    frame: &mut canvas::Frame,
    centre: Point,
    radii: Vector,
    colour: (f32, f32, f32),
    peak: f32,
) {
    let bounds = Rectangle::new(
        Point::new(centre.x - radii.x, centre.y - radii.y),
        Size::new(2.0 * radii.x, 2.0 * radii.y),
    );
    frame.draw_image(
        bounds,
        canvas::Image::new(glow_sprite(rgba(colour, 1.0))).opacity(peak.clamp(0.0, 1.0)),
    );
}

fn vertical(size: Size, stops: &[(f32, Color)]) -> gradient::Linear {
    stops.iter().fold(
        gradient::Linear::new(Point::ORIGIN, Point::new(0.0, size.height)),
        |line, (at, colour)| line.add_stop(*at, *colour),
    )
}

/// `point` turned by `angle` about `pivot` (y points down, so a negative angle turns it
/// counter-clockwise on screen, like CSS `rotate(-7deg)`).
fn turned(point: Vector, pivot: Point, angle: Radians) -> Point {
    let (sin, cos) = angle.0.sin_cos();
    Point::new(
        pivot.x + point.x * cos - point.y * sin,
        pivot.y + point.x * sin + point.y * cos,
    )
}

/// The red and black fog quad of layer 4.
fn paint_fog(frame: &mut canvas::Frame, size: Size) {
    let pivot = Point::new(size.width / 2.0, size.height * (1.0 + FOG_BELOW));
    let angle = Radians(FOG_ANGLE_DEGREES.to_radians());
    let half = FOG_WIDTH * size.width / 2.0;
    let tall = FOG_HEIGHT * size.height;
    let corners = [(-half, -tall), (half, -tall), (half, 0.0), (-half, 0.0)]
        .map(|(x, y)| turned(Vector::new(x, y), pivot, angle));
    let quad = Shape::new(|path| {
        path.move_to(corners[0]);
        path.line_to(corners[1]);
        path.line_to(corners[2]);
        path.line_to(corners[3]);
        path.close();
    });
    let (r, g, b, a) = FOG_RED;
    let line = gradient::Linear::new(
        turned(Vector::new(0.0, -tall), pivot, angle),
        turned(Vector::new(0.0, 0.0), pivot, angle),
    )
    .add_stop(0.0, Color::from_rgba(r, g, b, 0.0))
    .add_stop(0.5, Color::from_rgba(r, g, b, a))
    .add_stop(1.0, bg(FOG_FLOOR_ALPHA));
    frame.fill(&quad, line);
}

/// Heat alpha multiplier at `elapsed` seconds: a sine between `HEAT_MIN` and `HEAT_MAX`.
pub fn heat_pulse(elapsed: f32) -> f32 {
    let wave = 0.5 + 0.5 * (elapsed / HEAT_PERIOD * TAU).sin();
    HEAT_MIN + (HEAT_MAX - HEAT_MIN) * wave
}

impl canvas::Program<Message> for Backdrop {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let size = bounds.size();
        let mut frame = canvas::Frame::new(renderer, size);
        let at = |(x, y): (f32, f32)| Point::new(x * size.width, y * size.height);
        let radii = |(x, y): (f32, f32)| Vector::new(x * size.width, y * size.height);
        match self.0 {
            Layer::Base => {
                frame.fill_rectangle(
                    Point::ORIGIN,
                    size,
                    vertical(size, &[(0.0, BASE_TOP), (1.0, palette::BG)]),
                );
                glow(
                    &mut frame,
                    at(RED_GLOW_AT),
                    radii(RED_GLOW_RADII),
                    RED_GLOW,
                    RED_GLOW_PEAK,
                );
                glow(
                    &mut frame,
                    at(GREY_GLOW_AT),
                    radii(GREY_GLOW_RADII),
                    GREY_GLOW,
                    GREY_GLOW_PEAK,
                );
            }
            Layer::Veil => {
                frame.fill_rectangle(Point::ORIGIN, size, bg(VEIL_ALPHA));
                let reach = LEFT_VEIL_REACH * size.width;
                let left = LEFT_VEIL_STOPS.iter().fold(
                    gradient::Linear::new(Point::ORIGIN, Point::new(reach, 0.0)),
                    |line, (share, alpha)| line.add_stop(share / LEFT_VEIL_REACH, bg(*alpha)),
                );
                frame.fill_rectangle(Point::ORIGIN, Size::new(reach, size.height), left);
                let red = |a| rgba(BOTTOM_RED, a);
                frame.fill_rectangle(
                    Point::ORIGIN,
                    size,
                    vertical(
                        size,
                        &[
                            (0.0, bg(0.6)),
                            (0.28, bg(0.0)),
                            (0.5, red(0.0)),
                            (0.78, red(0.5)),
                            (1.0, bg(0.98)),
                        ],
                    ),
                );
            }
            Layer::Fog => paint_fog(&mut frame, size),
            Layer::Heat(elapsed) => {
                let pulse = heat_pulse(elapsed);
                for blob in HEAT_BLOBS {
                    glow(
                        &mut frame,
                        at(blob.centre),
                        radii(blob.radii),
                        blob.colour,
                        blob.peak * pulse,
                    );
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

fn layer<'a>(which: Layer) -> Element<'a, Message> {
    canvas_widget(Backdrop(which))
        .width(Fill)
        .height(Fill)
        .into()
}

/// A picture for the Home art layer and its size in pixels.
#[derive(Debug, Clone)]
pub struct Art {
    pub handle: Handle,
    pub size: (u32, u32),
}

/// The launcher's key art, one handle per process.
pub fn art() -> Handle {
    static ART: OnceLock<Handle> = OnceLock::new();
    ART.get_or_init(|| Handle::from_bytes(&include_bytes!("../../assets/background.png")[..]))
        .clone()
}

/// What the Home art layer draws: the key art, unless `state.art_override` holds another picture
/// (the snapshot tests' small placeholder).
pub fn shown_art(state: &State) -> Art {
    state.art_override.clone().unwrap_or_else(|| Art {
        handle: art(),
        size: ART_SIZE,
    })
}

/// The part of a `picture` (its size in pixels) that a `cover` fit shows in `scene`: the picture
/// scaled to cover the scene, cut to its shape around `ART_ANCHOR`. An empty scene shows the whole
/// picture.
pub fn art_crop(picture: (u32, u32), scene: Size) -> Rectangle<u32> {
    let full = Size::new(picture.0 as f32, picture.1 as f32);
    let scale = (scene.width / full.width).max(scene.height / full.height);
    if !scale.is_finite() || scale <= 0.0 {
        return Rectangle {
            x: 0,
            y: 0,
            width: picture.0,
            height: picture.1,
        };
    }
    let shown = Size::new(
        (scene.width / scale).min(full.width),
        (scene.height / scale).min(full.height),
    );
    Rectangle {
        x: ((full.width - shown.width) * ART_ANCHOR.0).round() as u32,
        y: ((full.height - shown.height) * ART_ANCHOR.1).round() as u32,
        width: shown.width.round() as u32,
        height: shown.height.round() as u32,
    }
}

/// The five layers of spec 4.3, bottom to top, with the fire drawn over them.
pub fn background(state: &State) -> Element<'_, Message> {
    let elapsed = state
        .now
        .saturating_duration_since(state.splash.started)
        .as_secs_f32();
    let shown = shown_art(state);
    let art = image(shown.handle)
        .width(Fill)
        .height(Fill)
        .content_fit(ContentFit::Cover)
        .crop(art_crop(shown.size, scene_size(state)))
        .opacity(ART_OPACITY);
    stack![
        layer(Layer::Base),
        container(art).width(Fill).height(Fill).id(ART_ID)
    ]
    .width(Fill)
    .height(Fill)
    // The renderers clip an image only to its layer: `cover` around the crop draws the whole
    // picture, which would run over the tab bar when the scene is wider than the art.
    .clip(true)
    .push(layer(Layer::Veil))
    .push(layer(Layer::Fog))
    .push(layer(Layer::Heat(elapsed)))
    // The only fire layer (R96): the splash's dark background covers it until the flight.
    .push(state.fire.view())
    .into()
}

// --- Texts ---

fn choice_label(table: &[Choice], value: &str, lang: &str) -> String {
    table
        .iter()
        .find(|choice| choice.value == value)
        .map_or_else(|| value.to_string(), |choice| tr(lang, choice.en))
}

fn ini_value<'a>(state: &'a State, key: &str) -> &'a str {
    state.ini.values.get(key).map_or("", String::as_str)
}

/// The three chips, saves, game and graphics card (ruling R77: the settings line replaced the
/// requirements chip).
pub fn chips(state: &State) -> [String; 3] {
    let lang = state.lang.as_str();
    // Only the card the game will use, by its name (the probe lists no Vulkan version).
    let gpu = match chosen_gpu(&state.gpus, &state.settings.get_str("gpu_id")) {
        Some(row) => format!("✓ {}", row.name),
        None => tr(lang, "• GPU: not checked"),
    };
    let game = match &state.home.info {
        Some(info) => {
            let id = state.home.title_id.as_deref().unwrap_or(&info.title);
            format!("✓ {id} · v{}", info.version)
        }
        None => tr(lang, "✗ No eboot.bin in the game folder"),
    };
    // Only whether saves exist: the folder is on the Game page.
    let saves = if state.home.saves {
        tr(lang, "✓ Saves found")
    } else {
        tr(lang, "• No saves yet")
    };
    [saves, game, gpu]
}

/// The line under the settings line (mockup `#status`, Python `self.status`): what the game is
/// doing while it starts, runs or its system files are copied; at rest, the patches the last start
/// left out (ruling R85, the tail of the Python's `summary()`, which needs a patchable game in the
/// game folder), else the in-game menu hint.
pub fn status_line(state: &State) -> String {
    let busy = !matches!(state.game, GameState::Idle);
    let facts = &state.home;
    if busy && !state.status.is_empty() {
        state.status.clone()
    } else if !busy && facts.skipped > 0 && facts.info.is_some() && facts.patchable {
        tr_fmt(
            &state.lang,
            "{} patches not found in this eboot.bin at the last start (see Log)",
            &[&facts.skipped.to_string()],
        )
    } else {
        tr(
            &state.lang,
            "In the game: Insert or R3+L2\nopens the port's menu.",
        )
        .replace('\n', " ")
    }
}

/// The settings line under PLAY in pieces (mockup `#quick`): output resolution, FPS limit, window
/// mode and monitor, the keys the Graphics page offers (ruling R78: not the parked upscaler and
/// preset). Each text comes with whether it stands out (`<b>`: the resolution and the FPS); the
/// other two and the separators are muted. The options show their short names, and the monitor
/// counts from 1.
pub fn quick_segments(state: &State) -> Vec<(String, bool)> {
    let lang = state.lang.as_str();
    let short = |table: &[Choice], key: &str| {
        short_label(&choice_label(table, ini_value(state, key), lang)).to_string()
    };
    let fps = match ini_value(state, "fps_limit") {
        "0" => choice_label(FPS_LIMITS, "0", lang),
        limit => format!("{limit} FPS"),
    };
    // The saved index of the probe's list, or empty for the primary monitor.
    let monitor = state
        .settings
        .get_str("monitor")
        .parse::<u32>()
        .map_or(1, |index| index.saturating_add(1));
    let items = [
        (short(OUTPUTS, "output_res"), true),
        (fps, true),
        (
            choice_label(DISPLAY_MODES, ini_value(state, "display_mode"), lang),
            false,
        ),
        (format!("{} {monitor}", tr(lang, "Monitor")), false),
    ];
    let mut segments = Vec::with_capacity(2 * items.len());
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            segments.push((" · ".to_string(), false));
        }
        segments.push(item);
    }
    segments
}

/// The settings line under PLAY as one text.
pub fn quick_line(state: &State) -> String {
    quick_segments(state)
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

/// The chip colour follows the mark the text starts with.
fn chip_kind(label: &str) -> ChipKind {
    if label.starts_with('✓') {
        ChipKind::Ok
    } else if label.starts_with('✗') {
        ChipKind::Warn
    } else {
        ChipKind::Plain
    }
}

// --- The block ---

fn logo() -> Handle {
    static LOGO: OnceLock<Handle> = OnceLock::new();
    LOGO.get_or_init(|| {
        Handle::from_bytes(&include_bytes!("../../assets/spartanport-logo.png")[..])
    })
    .clone()
}

/// The monogram artwork, embedded once: the window icon reads the same bytes.
pub static MONOGRAM_PNG: &[u8] = include_bytes!("../../assets/spartanport.png");

/// The monogram image, one handle per process (the splash shows the same one).
pub fn monogram() -> Handle {
    static MONOGRAM_IMAGE: OnceLock<Handle> = OnceLock::new();
    MONOGRAM_IMAGE
        .get_or_init(|| Handle::from_bytes(MONOGRAM_PNG))
        .clone()
}

/// PLAY and STOP at `opacity`: the Home block fades through its own colours behind a wide drawer
/// (ruling R79), never through a shade over its bounds.
fn play_row(state: &State, enabled: bool, opacity: f32) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    // PLAY waits while the system files are copied; a started game turns it into RUNNING and STOP.
    let (running, copying) = match state.game {
        GameState::Idle => (false, false),
        GameState::Copying => (false, true),
        GameState::Preparing { .. } | GameState::Running { .. } => (true, false),
    };
    let (label, press) = if running {
        (tr(lang, "RUNNING"), None)
    } else {
        (
            tr(lang, "PLAY"),
            (enabled && !copying).then_some(Message::Play),
        )
    };
    let play = button(text(label).font(CINZEL_BLACK).size(PLAY_SIZE))
        .padding(PLAY_PADDING)
        .style(move |theme, status| faded_button(button_play(theme, status), opacity))
        .on_press_maybe(press);
    let mut line = row![].spacing(12).align_y(Center);
    if running {
        line = line.push(
            button(text(tr(lang, "Stop").to_uppercase()).size(14))
                .padding([14, 22])
                .style(move |theme, status| faded_button(button_flat(theme, status), opacity))
                .on_press_maybe(enabled.then_some(Message::Stop)),
        );
    }
    line.push(play).into()
}

fn bottom_block(state: &State, enabled: bool) -> Element<'_, Message> {
    let opacity = home_interactive_opacity(state);
    let chips = chips(state).iter().fold(row![].spacing(8), |line, label| {
        line.push(chip(label, chip_kind(label), opacity))
    });
    let quick: Vec<Span<'_, (), Font>> = quick_segments(state)
        .into_iter()
        .map(|(piece, bright)| {
            let colour = if bright {
                palette::TEXT
            } else {
                palette::MUTED
            };
            span(piece).color(faded(colour, opacity))
        })
        .collect();
    let quick = container(rich_text(quick).size(QUICK_SIZE)).id(QUICK_ID);
    let status = text(status_line(state))
        .size(HINT_SIZE)
        .color(faded(palette::MUTED, opacity));
    let lines = column![chips, quick, status]
        .spacing(QUICK_GAP)
        .align_x(mirror_x(Right, state.rtl));
    let content = column![
        image(logo()).height(LOGO_HEIGHT).opacity(opacity),
        play_row(state, enabled, opacity),
        lines,
    ]
    .spacing(14)
    .align_x(mirror_x(Right, state.rtl));
    let placed = container(container(content).id(BLOCK_ID))
        .width(Fill)
        .height(Fill)
        .align_x(mirror_x(Right, state.rtl))
        .align_y(Bottom)
        .padding(mirror_padding(
            Padding {
                right: BLOCK_RIGHT,
                bottom: BLOCK_BOTTOM,
                ..Padding::ZERO
            },
            state.rtl,
        ));
    placed.into()
}

/// The buttons in the top-right corner: the version menu's and the language's with `version`, the
/// language's alone without it (the setup has no versions to pick).
pub fn top_right(state: &State, version: bool) -> Element<'_, Message> {
    // The language button keeps the corner; mirrored, the version button reads on its right.
    let language = lang_menu::button_view(state);
    let buttons = if !version {
        row![language]
    } else if state.rtl {
        row![language, version_menu::button_view(state)]
    } else {
        row![version_menu::button_view(state), language]
    };
    container(buttons.spacing(8).wrap())
        .width(Fill)
        .height(Fill)
        .align_x(mirror_x(Right, state.rtl))
        .align_y(Top)
        .padding(mirror_padding(
            Padding {
                top: TOP_RIGHT_TOP,
                right: TOP_RIGHT_RIGHT,
                ..Padding::ZERO
            },
            state.rtl,
        ))
        .into()
}

/// The monogram in the top corner, drawn once the splash is gone.
pub fn corner_monogram(state: &State) -> Element<'_, Message> {
    container(image(monogram()).width(MONOGRAM).height(MONOGRAM))
        .width(Fill)
        .height(Fill)
        .align_x(mirror_x(Left, state.rtl))
        .padding(MONOGRAM_EDGE)
        .into()
}

/// The interactive Home layer: corner monogram, version and language buttons, and the block
/// with the logotype, PLAY, chips, settings line and hint. Each part is sized to its content, so
/// the empty space between them lets clicks through to the layers below.
pub fn block(state: &State) -> Element<'_, Message> {
    let enabled = home_interactive_enabled(state);
    let mut layers = stack![].width(Fill).height(Fill);
    if state.splash.done {
        layers = layers.push(corner_monogram(state));
    }
    layers
        .push(top_right(state, true))
        .push(bottom_block(state, enabled))
        .into()
}
