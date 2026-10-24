//! The version button of the Home (spec 4.5): the installed release, with the newer one noted.

use iced::time::Instant;
use iced::widget::{button, column, container, mouse_area, row, scrollable, space, text};
use iced::{Element, Fill, Padding, Right, Task, Top};

use crate::core::i18n::{tr, tr_fmt};
use crate::core::release::{prereleases, stables, Release};
use crate::ui::app::{home_interactive_enabled, home_interactive_opacity, Message, State};
use crate::ui::hover_menu::{self, HoverMessage};
use crate::ui::lang_menu::{EDGE_RIGHT, EDGE_TOP, MENU_DROP, MENU_WIDTH};
use crate::ui::theme::{
    button_flat, container_card, faded_button, mirror_padding, mirror_x, palette, text_muted,
    CINZEL_BOLD,
};
use crate::ui::update::{self, UpdateMessage, UpdateState};

/// Id of the container that holds the button, for the tests.
pub const BUTTON_ID: &str = "version-button";

/// Id of the card that holds the list, for the tests.
pub const MENU_ID: &str = "version-menu";

/// Pre-release rows shown before the list scrolls.
pub const VISIBLE_PRERELEASES: usize = 8;

/// A ceiling on one row: 12.5 pt text + `[5, 12]` padding + 2 spacing measures about 28.25 pt,
/// so the cap of `VISIBLE_PRERELEASES` rows binds from the ninth row and leaves a peek of it.
pub const ROW_HEIGHT: f32 = 29.0;

/// Button events.
#[derive(Debug, Clone)]
pub enum VersionMessage {
    Enter,
    Leave,
    Toggle,
    Tick(Instant),
    Pick(Release),
}

/// Applies the hover timers; when the menu just opened over `UpdateState::None` or `Failed`,
/// returns the task of a silent check. While a download runs the menu stays closed (spec 4.2).
pub fn update(state: &mut State, message: VersionMessage) -> Task<Message> {
    if matches!(state.update, UpdateState::Downloading { .. }) {
        let menu = &mut state.version_menu;
        menu.open = false;
        menu.hover_since = None;
        menu.leave_since = None;
        return Task::none();
    }
    let hover = match message {
        // Not a hover event: closes the menu and installs unless it is the running release.
        VersionMessage::Pick(release) => {
            let menu = &mut state.version_menu;
            menu.open = false;
            menu.hover_since = None;
            menu.leave_since = None;
            if release.id.same_build(&state.installed) {
                return Task::none();
            }
            return update::update(state, UpdateMessage::Install(release));
        }
        VersionMessage::Enter => HoverMessage::Enter,
        VersionMessage::Leave => HoverMessage::Leave,
        VersionMessage::Toggle => HoverMessage::Toggle,
        VersionMessage::Tick(at) => HoverMessage::Tick(at),
    };
    let was_open = state.version_menu.open;
    let now = state.now;
    hover_menu::apply(&mut state.version_menu, hover, now);
    let stale = matches!(state.update, UpdateState::None | UpdateState::Failed(_));
    if !was_open && state.version_menu.open && stale {
        return update::update(state, UpdateMessage::Check { manual: false });
    }
    Task::none()
}

/// Whether a timer is running, so the app keeps the clock ticking.
pub fn timing(state: &State) -> bool {
    hover_menu::timing(&state.version_menu)
}

/// The installed release, with the newer stable noted; the download's progress while one runs.
pub fn button_text(state: &State) -> String {
    let lang = state.lang.as_str();
    if let UpdateState::Downloading {
        release, percent, ..
    } = &state.update
    {
        return tr_fmt(
            lang,
            "Downloading version {}… {}%",
            &[&release.id.short(), &percent.to_string()],
        );
    }
    let mut label = state.installed.label(lang);
    if let Some(newer) = update::newer(state) {
        label.push_str(" · ");
        label.push_str(&tr_fmt(lang, "{} available", &[&newer.id.version]));
    }
    label.push_str(" ▾");
    label
}

/// The button, before the language one in the top-right corner.
pub fn button_view(state: &State) -> Element<'_, Message> {
    let downloading = matches!(state.update, UpdateState::Downloading { .. });
    let enabled = home_interactive_enabled(state);
    let opacity = home_interactive_opacity(state);
    let pressed = button(text(button_text(state)).size(12))
        .padding([3, 10])
        .style(move |theme, status| faded_button(button_flat(theme, status), opacity))
        .on_press_maybe(
            (enabled && !downloading).then_some(Message::VersionMenu(VersionMessage::Toggle)),
        );
    // The id sits on a container: iced's button has none.
    let pressed = container(pressed).id(BUTTON_ID);
    if !enabled || downloading {
        return pressed.into();
    }
    mouse_area(pressed)
        .on_enter(Message::VersionMenu(VersionMessage::Enter))
        .on_exit(Message::VersionMenu(VersionMessage::Leave))
        .into()
}

/// A row that is not a button: the list's status line.
fn muted_row(label: String) -> Element<'static, Message> {
    container(text(label).size(12.5).style(text_muted))
        .padding([5, 12])
        .into()
}

fn release_row<'a>(
    release: &'a Release,
    installed: bool,
    newer: bool,
    lang: &str,
) -> Element<'a, Message> {
    let short = release.id.short();
    let label = if installed {
        format!("✓ {short}")
    } else {
        short
    };
    let content: Element<'a, Message> = if newer {
        row![
            text(label).size(12.5),
            space::horizontal(),
            text(tr(lang, "new")).size(12.5).color(palette::RED_HI),
        ]
        .into()
    } else {
        text(label).size(12.5).into()
    };
    button(content)
        .width(Fill)
        .padding([5, 12])
        .style(button_flat)
        .on_press(Message::VersionMenu(VersionMessage::Pick(release.clone())))
        .into()
}

fn header<'a>(lang: &str, key: &str) -> Element<'a, Message> {
    text(tr(lang, key))
        .font(CINZEL_BOLD)
        .size(11)
        .color(palette::RED_HI)
        .into()
}

/// The open list, under the button in the top-right corner of the Home view (the top-left one,
/// right to left).
pub fn menu(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let mut list = column![].width(MENU_WIDTH).spacing(2);
    list = list.push(header(lang, "Releases"));
    match &state.update {
        UpdateState::None | UpdateState::Checking => {
            list = list.push(muted_row(tr(lang, "Fetching…")));
        }
        UpdateState::Failed(_) => {
            list = list.push(muted_row(tr(lang, "Could not fetch the versions")));
        }
        UpdateState::Ready(_) | UpdateState::Downloading { .. } => {
            let releases = update::releases(state);
            let newer = update::newer(state).map(|r| &r.id);
            for r in stables(releases) {
                list = list.push(release_row(
                    r,
                    r.id.same_build(&state.installed),
                    newer == Some(&r.id),
                    lang,
                ));
            }
            list = list.push(header(lang, "Pre-releases"));
            let pre = prereleases(releases);
            if pre.is_empty() {
                let key = if cfg!(windows) {
                    "No test builds"
                } else {
                    "No test builds for Linux"
                };
                list = list.push(muted_row(tr(lang, key)));
            } else {
                let rows = column(
                    pre.into_iter()
                        .map(|r| release_row(r, r.id.same_build(&state.installed), false, lang)),
                )
                .spacing(2);
                list = list.push(
                    container(scrollable(rows)).max_height(VISIBLE_PRERELEASES as f32 * ROW_HEIGHT),
                );
            }
        }
    }
    let card = container(list)
        .padding(10)
        .style(container_card)
        .id(MENU_ID);
    let hovered = mouse_area(card)
        .on_enter(Message::VersionMenu(VersionMessage::Enter))
        .on_exit(Message::VersionMenu(VersionMessage::Leave));
    container(row![hovered])
        .width(Fill)
        .height(Fill)
        .align_x(mirror_x(Right, state.rtl))
        .align_y(Top)
        .padding(mirror_padding(
            Padding {
                top: EDGE_TOP + MENU_DROP,
                right: EDGE_RIGHT,
                ..Padding::ZERO
            },
            state.rtl,
        ))
        .into()
}
