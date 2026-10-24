//! The language selector of the Home page (spec 4.5).

use iced::time::Instant;
use iced::widget::{button, column, container, mouse_area, row, text};
use iced::{Element, Fill, Padding, Right, Top};
use serde_json::json;

use crate::core::i18n::{native_name, region_of, tr, Region, LANGS};
use crate::ui::app::{home_interactive_enabled, home_interactive_opacity, Message, State};
use crate::ui::hover_menu::{self, HoverMenu, HoverMessage};
pub use crate::ui::hover_menu::{HOVER_OPEN, LEAVE_CLOSE};
use crate::ui::theme::{
    button_flat, container_card, faded_button, mirror_padding, mirror_x, palette, CINZEL_BOLD,
};

/// Distance of the button from the top and right edges (mockup `#topright`).
pub(crate) const EDGE_TOP: f32 = 22.0;
pub(crate) const EDGE_RIGHT: f32 = 24.0;
/// The list hangs this far below the top edge (mockup `#langmenu`, `top:30px`).
pub(crate) const MENU_DROP: f32 = 30.0;
/// Minimum list width (mockup `#langmenu`).
pub(crate) const MENU_WIDTH: f32 = 200.0;

/// Open state and hover timers.
pub type LangMenu = HoverMenu;

/// Selector events.
#[derive(Debug, Clone)]
pub enum LangMessage {
    Enter,
    Leave,
    Toggle,
    Pick(String),
    Tick(Instant),
}

/// Applies a selector event; `Enter` and `Leave` are stamped with `state.now`.
pub fn update(state: &mut State, message: LangMessage) {
    let now = state.now;
    let hover = match message {
        LangMessage::Enter => HoverMessage::Enter,
        LangMessage::Leave => HoverMessage::Leave,
        LangMessage::Toggle => HoverMessage::Toggle,
        LangMessage::Tick(at) => HoverMessage::Tick(at),
        // Stores the language; `app::update` then saves, which rebuilds the window in it.
        LangMessage::Pick(code) => {
            let menu = &mut state.lang_menu;
            menu.open = false;
            menu.hover_since = None;
            menu.leave_since = None;
            state.settings.set("ui_language", json!(code));
            return;
        }
    };
    hover_menu::apply(&mut state.lang_menu, hover, now);
}

/// Whether a timer is running, so the app keeps the clock ticking.
pub fn timing(state: &State) -> bool {
    hover_menu::timing(&state.lang_menu)
}

/// The button with the current language's own name.
pub fn button_view(state: &State) -> Element<'_, Message> {
    let enabled = home_interactive_enabled(state);
    // Faded with the rest of the Home behind a wide drawer (ruling R79).
    let opacity = home_interactive_opacity(state);
    let label = text(format!("{} ▾", native_name(&state.lang))).size(12);
    let pressed = button(label)
        .padding([3, 10])
        .style(move |theme, status| faded_button(button_flat(theme, status), opacity))
        .on_press_maybe(enabled.then_some(Message::Lang(LangMessage::Toggle)));
    if !enabled {
        return pressed.into();
    }
    mouse_area(pressed)
        .on_enter(Message::Lang(LangMessage::Enter))
        .on_exit(Message::Lang(LangMessage::Leave))
        .into()
}

/// The open list, positioned under the button in the top-right corner of the Home view (the
/// top-left one, right to left).
pub fn menu(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let groups = [
        (Region::Americas, "Americas"),
        (Region::Europe, "Europe"),
        (Region::Asia, "Asia & Middle East"),
    ];
    let mut list = column![].width(MENU_WIDTH).spacing(2);
    for (region, header) in groups {
        list = list.push(
            text(tr(lang, header))
                .font(CINZEL_BOLD)
                .size(11)
                .color(palette::RED_HI),
        );
        for code in LANGS.into_iter().filter(|code| region_of(code) == region) {
            let name = native_name(code);
            let label = if code == lang {
                format!("✓ {name}")
            } else {
                name.to_string()
            };
            list = list.push(
                button(text(label).size(12.5))
                    .width(Fill)
                    .padding([5, 12])
                    .style(button_flat)
                    .on_press(Message::Lang(LangMessage::Pick(code.to_string()))),
            );
        }
    }
    let card = container(list).padding(10).style(container_card);
    let hovered = mouse_area(card)
        .on_enter(Message::Lang(LangMessage::Enter))
        .on_exit(Message::Lang(LangMessage::Leave));
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
