//! The Trophies page: the game's trophies, which ones this port recorded, and a reset.
//!
//! The package is read and its icons decoded on a thread (`load`); `view` only draws what the
//! `Loaded` message stored.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use iced::widget::image::Handle;
use iced::widget::{button, column, container, image, pick_list, row, space, text};
use iced::{Center, Color, Element, Fill, Task};

use crate::core::i18n::{tr, tr_fmt, trophy_text};
use crate::core::paths::Paths;
use crate::core::settings::Settings;
use crate::core::trophies::{
    read_trp_icons, read_unlocked_trophies, reset_trophies, tier_color, tier_label, trophy_package,
    trophy_tier, Tier,
};
use crate::ui::app::{Message, State};
use crate::ui::play::GameState;
use crate::ui::tasks::blocking;
use crate::ui::theme::{self, palette};
use crate::ui::widgets::{Modal, ModalKind, ModalMessage, SELECT_TEXT};
use crate::APP_NAME;

/// Side of a trophy icon, in logical pixels.
const ICON_SIZE: f32 = 56.0;

/// Which trophies the list shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Unlocked,
    Locked,
}

impl Filter {
    const ALL: [Filter; 3] = [Filter::All, Filter::Unlocked, Filter::Locked];

    fn english(self) -> &'static str {
        match self {
            Filter::All => "All trophies",
            Filter::Unlocked => "Unlocked",
            Filter::Locked => "Locked",
        }
    }
}

/// Trophy icons, tiers, unlocked set and filter.
#[derive(Debug, Clone, Default)]
pub struct TrophiesView {
    /// The icon of each trophy.
    pub icons: BTreeMap<u32, Handle>,
    /// The same icon in grey at half brightness, for a trophy not yet unlocked.
    pub locked_icons: BTreeMap<u32, Handle>,
    pub tiers: BTreeMap<u32, Option<Tier>>,
    pub unlocked: BTreeSet<u32>,
    pub filter: Filter,
    /// Whether a load is under way.
    pub loading: bool,
    /// The folders the read was made for (`FOLDERS`, in order).
    pub folders: Vec<String>,
}

/// The settings a read of the page depends on.
const FOLDERS: [&str; 3] = ["game_dir", "update_dir", "user_dir"];

/// The values of `FOLDERS` in `settings`.
fn folders(settings: &Settings) -> Vec<String> {
    FOLDERS.iter().map(|key| settings.get_str(key)).collect()
}

/// Trophies page events.
#[derive(Debug, Clone)]
pub enum TrophiesMessage {
    Refresh,
    Filter(Filter),
    Reset,
    ResetConfirmed,
    /// A load finished.
    Loaded(Box<TrophiesView>),
    /// The reset finished: how many trophies were locked, or why it failed.
    ResetDone(Result<usize, String>),
}

/// The icon in grey at half brightness; `None` when the PNG cannot be decoded.
fn locked_icon(png: &[u8]) -> Option<Handle> {
    let mut rgba = ::image::load_from_memory(png).ok()?.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let luma = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
        let grey = (luma * 0.5).round().clamp(0.0, 255.0) as u8;
        pixel.0 = [grey, grey, grey, a];
    }
    let (width, height) = rgba.dimensions();
    Some(Handle::from_rgba(width, height, rgba.into_raw()))
}

/// Reads the package, the tiers and the unlocked flags; runs on a thread.
pub fn load(paths: &Paths, settings: &Settings) -> TrophiesView {
    let mut view = TrophiesView {
        folders: folders(settings),
        ..TrophiesView::default()
    };
    let package = trophy_package(
        &settings.get_str("game_dir"),
        &settings.get_str("update_dir"),
    );
    let pngs = package
        .map(|path| read_trp_icons(&path))
        .unwrap_or_default();
    for (id, png) in pngs {
        view.tiers.insert(id, trophy_tier(&png));
        if let Some(locked) = locked_icon(&png) {
            view.locked_icons.insert(id, locked);
        }
        view.icons.insert(id, Handle::from_bytes(png));
    }
    let user = paths.user_dir(&settings.get_str("user_dir"));
    view.unlocked = read_unlocked_trophies(&user.join("trophies.dat"));
    view
}

/// Marks the list as loading and reads it in the background.
pub fn refresh(state: &mut State) -> Task<Message> {
    state.trophies.loading = true;
    let paths = state.paths.clone();
    let settings = state.settings.clone();
    blocking(move || load(&paths, &settings))
        .map(|view| Message::Trophies(TrophiesMessage::Loaded(Box::new(view))))
}

/// Whether the game is running, which blocks a reset.
fn game_running(state: &State) -> bool {
    match state.game {
        GameState::Idle | GameState::Copying => false,
        GameState::Preparing { .. } | GameState::Running { .. } => true,
    }
}

fn dialog(state: &mut State, kind: ModalKind, body: String) {
    state.modal = Some(Modal {
        kind,
        title: APP_NAME.to_string(),
        text: body,
    });
}

fn trophies_dat(state: &State) -> PathBuf {
    state
        .paths
        .user_dir(&state.settings.get_str("user_dir"))
        .join("trophies.dat")
}

/// Applies a Trophies page event.
pub fn update(state: &mut State, message: TrophiesMessage) -> Task<Message> {
    let lang = state.lang.clone();
    match message {
        TrophiesMessage::Refresh => return refresh(state),
        TrophiesMessage::Filter(filter) => state.trophies.filter = filter,
        // A read for folders typed over since is dropped: each keystroke starts one, and they
        // finish in any order (the read of the current folders is on its way or already in).
        TrophiesMessage::Loaded(view) if view.folders != folders(&state.settings) => {}
        TrophiesMessage::Loaded(view) => {
            let filter = state.trophies.filter;
            state.trophies = *view;
            state.trophies.filter = filter;
        }
        TrophiesMessage::Reset => {
            if game_running(state) {
                let body = tr(&lang, "Close the game before resetting the trophies.");
                dialog(state, ModalKind::Info, body);
            } else if state.trophies.unlocked.is_empty() {
                dialog(state, ModalKind::Info, tr(&lang, "No trophy is unlocked."));
            } else {
                let count = state.trophies.unlocked.len().to_string();
                let body = tr_fmt(
                    &lang,
                    "Lock all {} unlocked trophies again? A copy is kept as trophies.dat.bak.",
                    &[&count],
                );
                let kind = ModalKind::YesNo {
                    yes: Message::Trophies(TrophiesMessage::ResetConfirmed),
                    no: Message::Modal(ModalMessage::Dismiss),
                };
                dialog(state, kind, body);
            }
        }
        TrophiesMessage::ResetConfirmed => {
            state.modal = None;
            let path = trophies_dat(state);
            return blocking(move || reset_trophies(&path).map_err(|error| error.to_string()))
                .map(|result| Message::Trophies(TrophiesMessage::ResetDone(result)));
        }
        TrophiesMessage::ResetDone(result) => {
            if let Err(error) = result {
                let body = tr_fmt(&lang, "Could not reset the trophies: {}", &[&error]);
                dialog(state, ModalKind::Error, body);
            }
            return refresh(state);
        }
    }
    Task::none()
}

/// `#rrggbb` as a colour; the muted grey when the text is odd.
fn parse_color(hex: &str) -> Color {
    let channel = |at: usize| {
        hex.get(at..at + 2)
            .and_then(|digits| u8::from_str_radix(digits, 16).ok())
    };
    match (channel(1), channel(3), channel(5)) {
        (Some(r), Some(g), Some(b)) => Color::from_rgb8(r, g, b),
        _ => palette::MUTED,
    }
}

fn filter_box<'a>(state: &State) -> Element<'a, Message> {
    let lang = state.lang.clone();
    let label = |filter: Filter| tr(&lang, filter.english());
    let labels: Vec<String> = Filter::ALL.into_iter().map(label).collect();
    let selected = label(state.trophies.filter);
    let lang = state.lang.clone();
    pick_list(labels, Some(selected), move |chosen: String| {
        Filter::ALL
            .into_iter()
            .find(|filter| tr(&lang, filter.english()) == chosen)
            .map_or(Message::Noop, |filter| {
                Message::Trophies(TrophiesMessage::Filter(filter))
            })
    })
    .text_size(SELECT_TEXT)
    .style(theme::pick_list_dark)
    .into()
}

fn header<'a>(state: &State) -> Element<'a, Message> {
    let lang = &state.lang;
    let view = &state.trophies;
    let summary = if view.icons.is_empty() {
        String::new()
    } else {
        let got = view
            .icons
            .keys()
            .filter(|id| view.unlocked.contains(id))
            .count()
            .to_string();
        let total = view.icons.len().to_string();
        tr_fmt(lang, "{} of {} unlocked", &[&got, &total])
    };
    let flat = |label: String, message: TrophiesMessage| {
        button(text(label))
            .style(theme::button_flat)
            .on_press(Message::Trophies(message))
    };
    row![
        text(summary).size(18).font(theme::CINZEL_BOLD),
        space::horizontal(),
        filter_box(state),
        flat(tr(lang, "Reset trophies"), TrophiesMessage::Reset),
        flat(tr(lang, "Refresh"), TrophiesMessage::Refresh),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

fn card<'a>(state: &State, id: u32) -> Option<Element<'a, Message>> {
    let view = &state.trophies;
    let lang = &state.lang;
    let got = view.unlocked.contains(&id);
    let handle = if got {
        view.icons.get(&id)
    } else {
        view.locked_icons.get(&id).or_else(|| view.icons.get(&id))
    }?;
    let (name, description) = trophy_text(lang, id as usize);
    let name = if name.is_empty() {
        tr_fmt(lang, "Trophy {}", &[&id.to_string()])
    } else {
        name
    };
    let ink = if got { palette::TEXT } else { palette::MUTED };
    let tier = view.tiers.get(&id).copied().flatten();
    let id_line = match tier {
        Some(tier) => format!("ID {id}  ·  {}", tier_label(tier, lang)),
        None => format!("ID {id}"),
    };
    let id_ink = match tier {
        Some(tier) if got => parse_color(tier_color(tier)),
        _ => palette::MUTED,
    };
    let mut info = column![text(name).size(16).color(ink)].spacing(2);
    if !description.is_empty() {
        info = info.push(text(description).size(14).color(ink));
    }
    info = info.push(text(id_line).size(12).color(id_ink));
    let (badge, badge_ink) = if got {
        (tr(lang, "UNLOCKED"), palette::RED_HI)
    } else {
        (tr(lang, "LOCKED"), palette::MUTED)
    };
    let edge = if got { palette::RED } else { palette::LINE };
    let body = row![
        image(handle.clone()).width(ICON_SIZE).height(ICON_SIZE),
        container(info).width(Fill),
        text(badge).size(12).color(badge_ink),
    ]
    .spacing(12)
    .align_y(Center);
    Some(
        container(body)
            .padding([8, 12])
            .width(Fill)
            .style(move |theme| {
                let mut style = theme::container_card(theme);
                style.border.color = edge;
                style
            })
            .into(),
    )
}

pub fn view(state: &State) -> Element<'_, Message> {
    let lang = &state.lang;
    let view = &state.trophies;
    let mut page = column![header(state)].spacing(8);
    if view.icons.is_empty() {
        if !view.loading {
            page = page.push(
                text(tr(
                    lang,
                    "No trophy list: choose the game folder on the Game page (its sce_sys/trophy/trophy00.trp is read).",
                ))
                .size(14)
                .style(theme::text_muted),
            );
        }
    } else {
        for &id in view.icons.keys() {
            let got = view.unlocked.contains(&id);
            let shown = match view.filter {
                Filter::All => true,
                Filter::Unlocked => got,
                Filter::Locked => !got,
            };
            if let Some(card) = shown.then(|| card(state, id)).flatten() {
                page = page.push(card);
            }
        }
    }
    page.push(
        text(tr(
            lang,
            "Only trophies unlocked with this port are listed; the ones earned on a PlayStation are not known to it.",
        ))
        .size(13)
        .style(theme::text_muted),
    )
    .into()
}
