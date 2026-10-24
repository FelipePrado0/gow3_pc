//! The drawer: the panel that slides in from the left (from the right in a right-to-left
//! language) with the open page.

use iced::animation::{Animation, Easing};
use iced::widget::{button, column, container, opaque, pin, row, scrollable, text, Id};
use iced::{Center, Element, Fill, Right};

use crate::core::i18n::tr;
use crate::ui::app::{Message, State, DRAWER_SLIDE};
use crate::ui::pages::{self, Page};
use crate::ui::persist::dirty;
use crate::ui::theme::{button_play, container_panel, text_muted, CINZEL_BLACK, CINZEL_BOLD};
use crate::ui::widgets::{hairline, HAIRLINE};

/// Space between the drawer's page and its scrollbar.
const SCROLLBAR_GAP: f32 = 8.0;

/// Whether the drawer shows a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawerState {
    #[default]
    Closed,
    Open(Page),
}

/// Share of the window width the drawer of `page` takes.
pub fn drawer_fraction(page: Page) -> f32 {
    if page.wide() {
        0.72
    } else {
        0.46
    }
}

/// The drawer's share of the window, standing at `fraction`; it moves with the slide's duration
/// and easing.
pub fn fraction_animation(fraction: f32) -> Animation<f32> {
    Animation::new(fraction)
        .duration(DRAWER_SLIDE)
        .easing(Easing::EaseOutQuint)
}

/// Drawer width in logical pixels for the window, at the share `state.drawer_fraction` reaches at
/// `state.now`.
pub fn drawer_width(state: &State) -> f32 {
    state.window_size.width
        * state
            .drawer_fraction
            .interpolate_with(|fraction| fraction, state.now)
}

/// Horizontal shift of the drawer: `-width` when hidden (`progress` 0), none when open (1).
pub fn slide_offset(progress: f32, width: f32) -> f32 {
    (progress - 1.0) * width
}

/// The id of the red line along the drawer's inner edge.
pub const EDGE_ID: &str = "drawer-edge";

/// The drawer panel's widget id.
pub fn panel_id() -> Id {
    Id::new("drawer")
}

/// The drawer of `page`, shifted by the slide animation at `state.now`; right to left, it is
/// mirrored against the right edge of the window.
pub fn view(state: &State, page: Page) -> Element<'_, Message> {
    let width = drawer_width(state);
    let progress = state.drawer_progress.interpolate(0.0, 1.0, state.now);
    let title = column![
        text(tr(&state.lang, page.label_en()))
            .font(CINZEL_BLACK)
            .size(30),
        text(tr(&state.lang, page.subtitle_en()))
            .size(12)
            .style(text_muted),
    ]
    .spacing(4)
    // The title takes what Save leaves, so a long subtitle wraps instead of squeezing Save.
    .width(Fill);
    // Right to left the header is mirrored: Save at the left, the title against the right edge.
    let header = if state.rtl {
        row![save_button(state), title.align_x(Right)]
    } else {
        row![title, save_button(state)]
    }
    .spacing(18)
    .align_y(Center);
    // The scrollbar takes its own lane beside the page instead of covering its right edge.
    let body = scrollable(pages::view(state, page))
        .spacing(SCROLLBAR_GAP)
        .height(Fill);
    let page = container(column![header, body].spacing(18))
        .padding(28)
        .width(Fill)
        .height(Fill);
    // The red line on the side that faces the Home (mockup `#drawer` border-right).
    let edge = hairline(EDGE_ID).width(HAIRLINE).height(Fill);
    let inside = if state.rtl {
        row![edge, page]
    } else {
        row![page, edge]
    };
    let panel = container(inside)
        .id(panel_id())
        .width(width)
        .height(Fill)
        .style(container_panel);
    let offset = slide_offset(progress, width);
    let x = if state.rtl {
        state.window_size.width - width - offset
    } else {
        offset
    };
    pin(opaque(panel)).x(x).into()
}

/// Save, at the right of the header: active only while the window differs from the files. The
/// state is worked out on each draw (`persist::dirty` compares two small maps, without I/O).
fn save_button(state: &State) -> Element<'_, Message> {
    button(text(tr(&state.lang, "Save")).font(CINZEL_BOLD).size(14))
        .padding([8, 22])
        .style(button_play)
        .on_press_maybe(dirty(state).then_some(Message::Save))
        .into()
}
