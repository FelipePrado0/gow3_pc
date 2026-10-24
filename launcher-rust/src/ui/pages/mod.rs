//! The nine pages of the tab bar and the drawer body of each.

pub mod advanced;
pub mod controls;
pub mod game;
pub mod graphics;
pub mod log;
pub mod mods;
pub mod performance;
pub mod trophies;

use iced::widget::text;
use iced::Element;

use crate::ui::app::{Message, State};

/// A tab of the bottom bar. Home is the background; every other page opens in the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Page {
    #[default]
    Home,
    Graphics,
    Performance,
    Game,
    Trophies,
    Controls,
    Mods,
    Advanced,
    Log,
}

impl Page {
    /// Tab order, which is also the order of the arrow keys.
    pub const ALL: [Page; 9] = [
        Page::Home,
        Page::Graphics,
        Page::Performance,
        Page::Game,
        Page::Trophies,
        Page::Controls,
        Page::Mods,
        Page::Advanced,
        Page::Log,
    ];

    /// The English tab label, which is also the translation key.
    pub fn label_en(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Graphics => "Graphics",
            Page::Performance => "Performance",
            Page::Game => "Game",
            Page::Trophies => "Trophies",
            Page::Controls => "Controls",
            Page::Mods => "Mods",
            Page::Advanced => "Advanced",
            Page::Log => "Log",
        }
    }

    /// The English subtitle under the drawer's title (the mockup's, without its sample counts),
    /// which is also the translation key; Home has no drawer.
    pub fn subtitle_en(self) -> &'static str {
        match self {
            Page::Home => "",
            Page::Graphics => {
                "Stored in spartanport.ini; the in-game menu (Insert or R3+L2) changes the same values."
            }
            Page::Performance => {
                "Frame rate, shaders and GPU data paths. Applied when the game starts."
            }
            Page::Game => "Your game dump, saves and the game patches.",
            Page::Trophies => "The trophies this port recorded while you played.",
            Page::Controls => "Keyboard, mouse and gamepad mapping.",
            Page::Mods => "Change textures, sounds and text. Your game files stay untouched.",
            Page::Advanced => "Launcher options, diagnostics and system tools.",
            Page::Log => "Output of the last start. last_run.log holds the same text.",
        }
    }

    /// Pages whose drawer takes 72 % of the window instead of 46 % (W1, R95).
    pub fn wide(self) -> bool {
        matches!(
            self,
            Page::Performance | Page::Trophies | Page::Controls | Page::Mods | Page::Log
        )
    }

    /// The tab to the right, wrapping from Log to Home.
    pub fn next(self) -> Page {
        Page::ALL[(self.index() + 1) % Page::ALL.len()]
    }

    /// The tab to the left, wrapping from Home to Log.
    pub fn previous(self) -> Page {
        Page::ALL[(self.index() + Page::ALL.len() - 1) % Page::ALL.len()]
    }

    fn index(self) -> usize {
        Page::ALL
            .iter()
            .position(|page| *page == self)
            .unwrap_or_default()
    }
}

/// The drawer body of `page`.
pub fn view(state: &State, page: Page) -> Element<'_, Message> {
    match page {
        Page::Home => text("…").into(),
        Page::Graphics => graphics::view(state),
        Page::Performance => performance::view(state),
        Page::Game => game::view(state),
        Page::Trophies => trophies::view(state),
        Page::Controls => controls::view(state),
        Page::Mods => mods::view(state),
        Page::Advanced => advanced::view(state),
        Page::Log => log::view(state),
    }
}
