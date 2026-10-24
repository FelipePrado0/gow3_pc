#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Tab bar, drawer and keyboard navigation.
//!
//! Clicks go through `iced_test::simulator` over `app::view`. Key presses in the simulator reach
//! the widget tree, not the application's keyboard subscription, so the keyboard tests feed
//! `Message::Key(...)` to `app::update` directly, as the subscription does at run time.

mod common;

use std::path::Path;
use std::time::Duration;

use common::natural_size;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Location, Modifiers};
use iced::time::Instant;
use iced::{Background, Point, Rectangle, Size};
use iced_test::core::Settings;
use iced_test::simulator::{self, simulator, Simulator};
use spartanport_launcher::core::i18n::tr;
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::core::release::{Channel, Release, ReleaseId};
use spartanport_launcher::ui::app::{
    self, home_interactive_enabled, home_interactive_opacity, key_message, Message, State,
};
use spartanport_launcher::ui::drawer::{self, drawer_width, DrawerState};
use spartanport_launcher::ui::pages::Page;
use spartanport_launcher::ui::theme::{self, palette, CINZEL_BOLD};
use spartanport_launcher::ui::update::UpdateMessage;
use spartanport_launcher::ui::widgets;
use spartanport_launcher::ui::widgets::modal::{Modal, ModalKind, ModalMessage};
use spartanport_launcher::VERSION;

/// The page order of the Global Constraints.
const ORDER: [Page; 9] = [
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

/// The pages whose drawer takes 72 % of the window (R95 adds Performance).
const WIDE: [Page; 5] = [
    Page::Performance,
    Page::Trophies,
    Page::Controls,
    Page::Mods,
    Page::Log,
];

/// A state over `dir` only: data, ini and the legacy config folder all live there.
fn state_in(dir: &Path) -> State {
    let root = dir.display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    let mut state = State::new(Paths::from_env(dir, &env));
    // These tests are about the tabs, not the splash that covers them at start.
    state.splash.done = true;
    state
}

fn click(state: &mut State, label: &str) {
    let mut ui = simulator(app::view(state));
    ui.click(label)
        .unwrap_or_else(|error| panic!("click {label:?}: {error:?}"));
    for message in ui.into_messages() {
        let _ = app::update(state, message);
    }
}

/// Presses the left button at `(x, y)` of a 1280 × 800 window and applies what it emitted.
fn click_at(state: &mut State, x: f32, y: f32) {
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(1280.0, 800.0),
        app::view(state),
    );
    ui.point_at(Point::new(x, y));
    let _ = ui.simulate(simulator::click());
    for message in ui.into_messages() {
        let _ = app::update(state, message);
    }
}

fn press(state: &mut State, key: Named) {
    let _ = app::update(
        state,
        Message::Key(Key::Named(key), Modifiers::default(), Location::Standard),
    );
}

#[test]
fn every_tab_opens_its_drawer_with_the_right_width() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    assert_eq!(state.lang, "en");
    assert_eq!(Page::ALL, ORDER);
    assert!(home_interactive_enabled(&state));
    assert_eq!(home_interactive_opacity(&state), 1.0);
    for page in Page::ALL.into_iter().filter(|page| *page != Page::Home) {
        click(&mut state, page.label_en());
        assert_eq!(state.drawer, DrawerState::Open(page));
        assert_eq!(state.page, page);
        let wide = WIDE.contains(&page);
        assert_eq!(page.wide(), wide, "{page:?}");
        // Once the width has slid to the page's size.
        let settled = state.now + Duration::from_millis(300);
        let _ = app::update(&mut state, Message::Tick(settled));
        let expected = 1280.0 * if wide { 0.72 } else { 0.46 };
        let width = drawer_width(&state);
        assert!((width - expected).abs() < 0.01, "{page:?}: {width}");
        // The Home block fades to 35 % and stops responding only behind a wide drawer.
        assert_eq!(home_interactive_enabled(&state), !wide, "{page:?}");
        let opacity = if wide { 0.35 } else { 1.0 };
        assert_eq!(home_interactive_opacity(&state), opacity, "{page:?}");
    }
}

#[test]
fn the_drawer_width_slides_between_the_two_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let tick = |state: &mut State, at| {
        let _ = app::update(state, Message::Tick(at));
    };
    let _ = app::update(&mut state, Message::ShowPage(Page::Graphics));
    // Opening from closed slides in at the page's own width.
    assert!((drawer_width(&state) - 0.46 * 1280.0).abs() < 0.01);
    let opened = state.now;
    tick(&mut state, opened + Duration::from_secs(1));
    assert!(!app::drawer_animating(&state));

    // Ruling R81: Graphics (46 %) to Log (72 %) in the slide's 220 ms.
    let _ = app::update(&mut state, Message::ShowPage(Page::Log));
    let switched = state.now;
    tick(&mut state, switched + Duration::from_millis(110));
    let width = drawer_width(&state);
    assert!(
        0.46 * 1280.0 < width && width < 0.72 * 1280.0,
        "width {width} at 110 ms"
    );
    // The frames keep coming while the width moves.
    assert!(app::drawer_animating(&state));
    tick(&mut state, switched + Duration::from_millis(220));
    assert!((drawer_width(&state) - 0.72 * 1280.0).abs() < 0.01);
    assert!(!app::drawer_animating(&state));
}

#[test]
fn escape_closes() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    click(&mut state, "Graphics");
    assert_eq!(state.drawer, DrawerState::Open(Page::Graphics));
    press(&mut state, Named::Escape);
    assert_eq!(state.drawer, DrawerState::Closed);
    assert_eq!(state.page, Page::Home);
}

fn dialog(kind: ModalKind) -> Option<Modal> {
    Some(Modal {
        kind,
        title: "SpartanPort".into(),
        text: "Install version 9.9.9 now?".into(),
    })
}

#[test]
fn keys_do_nothing_under_a_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.modal = dialog(ModalKind::YesNo {
        yes: Message::Update(UpdateMessage::Confirmed(Release {
            id: ReleaseId {
                channel: Channel::Release,
                version: "9.9.9".into(),
                build: None,
            },
            published: String::new(),
            url: "https://example.invalid/SpartanPort.zip".into(),
            page: String::new(),
        })),
        no: Message::Modal(ModalMessage::Dismiss),
    });
    // On Home, Enter presses PLAY; behind the update question it would start the game.
    let task = app::update(
        &mut state,
        Message::Key(
            Key::Named(Named::Enter),
            Modifiers::default(),
            Location::Standard,
        ),
    );
    assert_eq!(task.units(), 0);
    press(&mut state, Named::ArrowRight);
    assert_eq!(state.page, Page::Home);
    assert_eq!(state.drawer, DrawerState::Closed);
    // A question needs its answer: Esc does not close it.
    press(&mut state, Named::Escape);
    assert!(state.modal.is_some());
}

#[test]
fn escape_closes_an_info_dialog_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    click(&mut state, "Graphics");
    for kind in [ModalKind::Info, ModalKind::Error] {
        state.modal = dialog(kind);
        press(&mut state, Named::Escape);
        assert!(state.modal.is_none());
        assert_eq!(state.drawer, DrawerState::Open(Page::Graphics));
    }
}

#[test]
fn arrows_cycle_pages_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    assert_eq!(state.page, Page::Home);

    let mut forward = Vec::new();
    for _ in 0..ORDER.len() {
        press(&mut state, Named::ArrowRight);
        forward.push(state.page);
        let expected = if state.page == Page::Home {
            DrawerState::Closed
        } else {
            DrawerState::Open(state.page)
        };
        assert_eq!(state.drawer, expected);
    }
    let mut wrapped = ORDER[1..].to_vec();
    wrapped.push(Page::Home);
    assert_eq!(forward, wrapped);

    let mut backward = Vec::new();
    for _ in 0..ORDER.len() {
        press(&mut state, Named::ArrowLeft);
        backward.push(state.page);
    }
    let reversed: Vec<Page> = ORDER.iter().rev().copied().collect();
    assert_eq!(backward, reversed);
}

#[test]
fn home_tab_closes_the_drawer() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    click(&mut state, "Mods");
    assert_eq!(state.drawer, DrawerState::Open(Page::Mods));
    click(&mut state, "Home");
    assert_eq!(state.drawer, DrawerState::Closed);
    assert_eq!(state.page, Page::Home);
}

#[test]
fn enter_on_home_sends_play() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let enter = Key::Named(Named::Enter);

    assert!(matches!(key_message(&state, &enter), Some(Message::Play)));
    let task = app::update(
        &mut state,
        Message::Key(enter.clone(), Modifiers::default(), Location::Standard),
    );
    assert_eq!(task.units(), 1);

    click(&mut state, "Graphics");
    assert!(key_message(&state, &enter).is_none());
    let task = app::update(
        &mut state,
        Message::Key(enter, Modifiers::default(), Location::Standard),
    );
    assert_eq!(task.units(), 0);
    assert_eq!(state.drawer, DrawerState::Open(Page::Graphics));
}

#[test]
fn background_click_closes_but_the_drawer_body_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    click(&mut state, "Graphics");
    // Let the slide finish so the drawer sits at x = 0 .. 588.8.
    let _ = app::update(
        &mut state,
        Message::Tick(Instant::now() + Duration::from_secs(1)),
    );

    click_at(&mut state, 200.0, 300.0);
    assert_eq!(state.drawer, DrawerState::Open(Page::Graphics));

    click_at(&mut state, 1200.0, 300.0);
    assert_eq!(state.drawer, DrawerState::Closed);
}

#[test]
fn rtl_puts_the_drawer_on_the_right() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), r#"{"ui_language": "ar"}"#).unwrap();
    let mut state = state_in(dir.path());
    assert_eq!(state.lang, "ar");
    assert!(state.rtl);
    // The tab labels are in Arabic, so the page is opened by its message, not by its label.
    let _ = app::update(&mut state, Message::ShowPage(Page::Graphics));
    let _ = app::update(
        &mut state,
        Message::Tick(Instant::now() + Duration::from_secs(1)),
    );
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(1280.0, 800.0),
        app::view(&state),
    );
    let panel = ui.find(drawer::panel_id()).unwrap().bounds();
    assert!(panel.x >= 640.0, "drawer at x = {}", panel.x);
    assert!((panel.x + panel.width - 1280.0).abs() < 1.0, "{panel:?}");
    // The bar is mirrored too: the version sits at the left, the tabs at the right.
    let version = ui.find(app::version_label(cfg!(windows))).unwrap().bounds();
    assert!(version.x < 640.0, "version at x = {}", version.x);
    // Arabic needs advanced shaping: iced's default (`Auto`) gives it to every non-ASCII text.
    assert!(matches!(
        iced::widget::text::Shaping::default(),
        iced::widget::text::Shaping::Auto | iced::widget::text::Shaping::Advanced
    ));
}

#[test]
fn the_bar_names_the_version_and_the_system() {
    // Ruling R82; there is no Steam Deck detection to tell SteamOS apart.
    assert_eq!(app::version_label(true), format!("v{VERSION} · Windows"));
    assert_eq!(app::version_label(false), format!("v{VERSION} · Linux"));
    let dir = tempfile::tempdir().unwrap();
    let state = state_in(dir.path());
    let mut ui = simulator(app::view(&state));
    let system = if cfg!(windows) { "Windows" } else { "Linux" };
    let shown = format!("v{VERSION} · {system}");
    assert!(
        ui.find(shown.as_str()).is_ok(),
        "{shown} missing from the bar"
    );
}

#[test]
fn a_red_hairline_tops_the_bar_and_edges_the_drawer() {
    // Ruling R88: one 1 px line in `palette::RED`, shared by the bar, the drawer and the Log page.
    assert_eq!(
        widgets::hairline_style(&theme::theme()).background,
        Some(Background::Color(palette::RED))
    );
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.lang = "en".into();
    let _ = app::update(&mut state, Message::ShowPage(Page::Graphics));
    let mut ui = settled_ui(&mut state);
    let line = |ui: &mut Simulator<'_, Message>, id: &'static str| {
        ui.find(iced_test::selector::id(id)).unwrap().bounds()
    };
    // Mockup `#bar` border-top: across the window, the bar's first pixel row.
    let bar = line(&mut ui, app::BAR_EDGE_ID);
    assert_eq!(
        bar,
        Rectangle::new(Point::new(0.0, 748.0), Size::new(1280.0, 1.0))
    );
    // Mockup `#drawer` border-right: the drawer's inner edge, top to bottom.
    let panel = ui.find(drawer::panel_id()).unwrap().bounds();
    let edge = line(&mut ui, drawer::EDGE_ID);
    assert_eq!(
        edge,
        Rectangle::new(
            Point::new(panel.x + panel.width - 1.0, panel.y),
            Size::new(1.0, panel.height)
        )
    );
    drop(ui);
    // Right to left the drawer stands against the right edge: its inner edge is its left one.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), r#"{"ui_language": "ar"}"#).unwrap();
    let mut state = state_in(dir.path());
    let _ = app::update(&mut state, Message::ShowPage(Page::Graphics));
    let mut ui = settled_ui(&mut state);
    let panel = ui.find(drawer::panel_id()).unwrap().bounds();
    let edge = line(&mut ui, drawer::EDGE_ID);
    assert_eq!(
        edge,
        Rectangle::new(Point::new(panel.x, panel.y), Size::new(1.0, panel.height))
    );
}

/// The drawer subtitles of the mockup (`launcher-mockup-v13.html`), without its sample counts.
const SUBTITLES: [(Page, &str); 8] = [
    (
        Page::Graphics,
        "Stored in spartanport.ini; the in-game menu (Insert or R3+L2) changes the same values.",
    ),
    (
        Page::Performance,
        "Frame rate, shaders and GPU data paths. Applied when the game starts.",
    ),
    (Page::Game, "Your game dump, saves and the game patches."),
    (
        Page::Trophies,
        "The trophies this port recorded while you played.",
    ),
    (Page::Controls, "Keyboard, mouse and gamepad mapping."),
    (
        Page::Mods,
        "Change textures, sounds and text. Your game files stay untouched.",
    ),
    (
        Page::Advanced,
        "Launcher options, diagnostics and system tools.",
    ),
    (
        Page::Log,
        "Output of the last start. last_run.log holds the same text.",
    ),
];

#[test]
fn every_drawer_has_its_subtitle() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    for (page, english) in SUBTITLES {
        for lang in ["en", "pt"] {
            state.lang = lang.into();
            let shown = tr(lang, english);
            if lang == "pt" {
                assert_ne!(shown, english, "{page:?} has no Portuguese subtitle");
            }
            let _ = app::update(&mut state, Message::ShowPage(page));
            let mut ui = settled_ui(&mut state);
            assert!(ui.find(shown.as_str()).is_ok(), "{page:?} {lang}: {shown}");
        }
    }
}

#[test]
fn a_long_subtitle_leaves_the_save_button_its_width() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    for lang in ["en", "pt", "ar"] {
        state.lang = lang.into();
        state.rtl = lang == "ar";
        let label = tr(lang, "Save");
        let alone = natural_size(&label, Some(CINZEL_BOLD), 14.0).width;
        for page in Page::ALL.into_iter().filter(|page| *page != Page::Home) {
            let _ = app::update(&mut state, Message::ShowPage(page));
            let mut ui = settled_ui(&mut state);
            let shown = ui.find(label.as_str()).unwrap().bounds().width;
            assert!(
                shown >= alone - 0.5,
                "{page:?} {lang}: Save is {shown} wide, needs {alone}"
            );
        }
    }
}

#[test]
fn rtl_mirrors_the_drawer_rows_and_header() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), r#"{"ui_language": "ar"}"#).unwrap();
    let mut state = state_in(dir.path());
    let _ = app::update(&mut state, Message::ShowPage(Page::Graphics));
    let mut ui = settled_ui(&mut state);
    // A row: its control (the screenshots button) sits to the left of its name.
    let name = ui.find(tr("ar", "F12 in the game")).unwrap().bounds();
    let control = ui
        .find(tr("ar", "Open the screenshots folder"))
        .unwrap()
        .bounds();
    assert!(
        control.x < name.x,
        "control at {}, name at {}",
        control.x,
        name.x
    );
    // The header: Save at the left, the title and its subtitle at the right.
    let save = ui.find(tr("ar", "Save")).unwrap().bounds();
    let subtitle = ui
        .find(tr("ar", Page::Graphics.subtitle_en()))
        .unwrap()
        .bounds();
    assert!(
        save.x < subtitle.x,
        "Save at {}, subtitle at {}",
        save.x,
        subtitle.x
    );
    drop(ui);

    // Left to right nothing moves: the name first, Save at the right.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.lang = "en".into();
    let _ = app::update(&mut state, Message::ShowPage(Page::Graphics));
    let mut ui = settled_ui(&mut state);
    let name = ui.find("F12 in the game").unwrap().bounds();
    let control = ui.find("Open the screenshots folder").unwrap().bounds();
    assert!(name.x < control.x);
    let save = ui.find("Save").unwrap().bounds();
    let subtitle = ui.find(Page::Graphics.subtitle_en()).unwrap().bounds();
    assert!(subtitle.x < save.x);
}

/// The window of `state` at 1280 × 800, once the drawer has finished sliding.
fn settled_ui(state: &mut State) -> Simulator<'_, Message> {
    let _ = app::update(
        state,
        Message::Tick(Instant::now() + Duration::from_secs(1)),
    );
    Simulator::with_size(
        Settings::default(),
        Size::new(1280.0, 800.0),
        app::view(state),
    )
}

#[test]
fn tab_labels_keep_their_width() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.lang = "en".into();
    let mut ui = settled_ui(&mut state);
    // Squeezed into a narrower tab, a label spills over its neighbours.
    for page in Page::ALL {
        let label = page.label_en();
        let shown = ui.find(label).unwrap().bounds().width;
        let alone = natural_size(label, Some(CINZEL_BOLD), 14.0).width;
        assert!(shown >= alone - 0.5, "{label}: {shown} wide, needs {alone}");
    }
}

#[test]
fn the_drawer_scrollbar_sits_beside_the_page() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.lang = "en".into();
    let _ = app::update(&mut state, Message::ShowPage(Page::Game));
    let mut ui = settled_ui(&mut state);
    let panel = ui.find(drawer::panel_id()).unwrap().bounds();
    // The Game page is taller than the window, so its body scrolls. The panel pads its body by 28
    // and iced's scrollbar is 10 wide; the Open buttons end the folder rows at the right, 10 past
    // their text.
    let scrollbar = panel.x + panel.width - 28.0 - 10.0;
    let open = ui.find("Open").unwrap().bounds();
    assert!(
        open.x + open.width + 10.0 <= scrollbar + 0.5,
        "the Open button ends at {}, under the scrollbar at {scrollbar}",
        open.x + open.width + 10.0
    );
}
