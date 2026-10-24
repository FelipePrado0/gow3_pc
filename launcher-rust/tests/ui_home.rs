#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The Home page (chips, quick line, PLAY) and the language menu.

mod common;

use std::path::Path;
use std::time::Duration;

use iced::time::Instant;
use iced::widget::image::Handle;
use iced::{Rectangle, Size, Task};
use iced_test::simulator::simulator;
use serde_json::json;
use spartanport_launcher::core::gpu::GpuRow;
use spartanport_launcher::core::i18n::tr;
use spartanport_launcher::core::release::{Channel, Release, ReleaseId};
use spartanport_launcher::ui::app::{self, Message, State};
use spartanport_launcher::ui::drawer::DrawerState;
use spartanport_launcher::ui::home;
use spartanport_launcher::ui::lang_menu::LangMessage;
use spartanport_launcher::ui::pages::Page;
use spartanport_launcher::ui::update::UpdateState;
use spartanport_launcher::ui::version_menu::VersionMessage;
use spartanport_launcher::ui::widgets::modal::{Modal, ModalKind};

/// A state over `dir` only, with the splash already gone.
fn state_in(dir: &Path) -> State {
    let root = dir.display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    let mut state = State::new(spartanport_launcher::core::paths::Paths::from_env(
        dir, &env,
    ));
    state.splash.done = true;
    state
}

fn lang(state: &mut State, message: LangMessage) {
    let _ = app::update(state, Message::Lang(message));
}

fn tick(state: &mut State, at: Instant) {
    let _ = app::update(state, Message::Tick(at));
}

/// Clicks `label` in the current view and returns what the click emitted.
fn click(state: &State, label: &str) -> Vec<Message> {
    let mut ui = simulator(app::view(state));
    ui.click(label)
        .unwrap_or_else(|error| panic!("click {label:?}: {error:?}"));
    ui.into_messages().collect()
}

#[test]
fn home_shows_play_and_the_three_chips() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_in(dir.path());
    // Saves, game, graphics card (ruling R77): the settings line replaced the requirements chip.
    let chips = home::chips(&state);
    assert_eq!(
        chips,
        [
            "• No saves yet",
            "✗ No eboot.bin in the game folder",
            "• GPU: not checked",
        ]
    );
    assert_eq!(
        home::quick_line(&state),
        "1920 × 1080 · Unlimited · Windowed · Monitor 1"
    );

    let mut ui = simulator(app::view(&state));
    assert!(ui.find("PLAY").is_ok());
    for chip in &chips {
        assert!(ui.find(chip.as_str()).is_ok(), "chip {chip:?} missing");
    }
    assert!(ui.find(iced_test::selector::id(home::QUICK_ID)).is_ok());
    // The requirements chip at rest, without a game folder.
    assert!(ui.find("• Choose the game folder first.").is_err());
}

#[test]
fn the_chips_read_saves_then_game_then_graphics_card_from_the_left() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_in(dir.path());
    let mut ui = simulator(app::view(&state));
    let mut x = |label: &str| ui.find(label).unwrap().bounds().x;
    let (saves, game, card) = (
        x("• No saves yet"),
        x("✗ No eboot.bin in the game folder"),
        x("• GPU: not checked"),
    );
    assert!(saves < game && game < card, "{saves}, {game}, {card}");
}

/// The ini and settings of the mockup's settings line (`launcher-mockup-v13.html`, `#quick`).
fn mockup_settings(state: &mut State) {
    for (key, value) in [
        ("upscaler", "fsr3"),
        ("preset", "1"),
        ("output_res", "2560x1440"),
        ("fps_limit", "120"),
        ("display_mode", "fullscreen"),
    ] {
        state.ini.values.insert(key.to_string(), value.to_string());
    }
    state.settings.set("monitor", json!("0"));
}

#[test]
fn the_settings_line_reads_like_the_mockup() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    mockup_settings(&mut state);
    // The mockup's line without the upscaler and the preset, which the 2.6.0 Graphics page does
    // not offer (rulings R35 and R78).
    assert_eq!(
        home::quick_line(&state),
        "2560 × 1440 · 120 FPS · Fullscreen · Monitor 1"
    );
    // `<b>` of the mockup: the resolution and the FPS stand out.
    let segments = home::quick_segments(&state);
    let bright: Vec<&str> = segments
        .iter()
        .filter(|(_, bright)| *bright)
        .map(|(text, _)| text.as_str())
        .collect();
    assert_eq!(bright, ["2560 × 1440", "120 FPS"]);
    let muted: String = segments
        .iter()
        .filter(|(_, bright)| !*bright)
        .map(|(text, _)| format!("[{text}]"))
        .collect();
    assert_eq!(muted, "[ · ][ · ][Fullscreen][ · ][Monitor 1]");
    state.lang = "pt".into();
    assert_eq!(
        home::quick_line(&state),
        "2560 × 1440 · 120 FPS · Tela cheia · Monitor 1"
    );
}

#[test]
fn the_settings_line_leaves_out_the_parked_keys() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    mockup_settings(&mut state);
    let before = home::quick_line(&state);
    // Keys the Graphics page does not show: the line does not follow them.
    for (key, value) in [("upscaler", "off"), ("preset", "3")] {
        state.ini.values.insert(key.to_string(), value.to_string());
    }
    assert_eq!(home::quick_line(&state), before);
}

#[test]
fn the_monitor_counts_from_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    mockup_settings(&mut state);
    for (saved, shown) in [("", "Monitor 1"), ("0", "Monitor 1"), ("1", "Monitor 2")] {
        state.settings.set("monitor", json!(saved));
        assert!(
            home::quick_line(&state).ends_with(&format!(" · {shown}")),
            "{saved:?}: {}",
            home::quick_line(&state)
        );
    }
}

#[test]
fn the_hint_sits_under_the_settings_line_in_the_mockup_words() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    for (lang, hint) in [
        ("en", "In the game: Insert or R3+L2 opens the port's menu."),
        ("pt", "No jogo: Insert ou R3+L2 abre o menu do port."),
    ] {
        state.lang = lang.into();
        let mut ui = simulator(app::view(&state));
        let line = ui
            .find(iced_test::selector::id(home::QUICK_ID))
            .unwrap()
            .bounds();
        let below = ui.find(hint).unwrap().bounds();
        // Mockup `#status`: 10 px under `#quick`.
        assert!(
            (below.y - (line.y + line.height) - 10.0).abs() < 0.5,
            "{lang}: line {line:?}, hint {below:?}"
        );
    }
}

/// Writes the last start's patch report with `skipped` patches left out.
fn last_start_skipped(state: &State, skipped: &[&str]) {
    let out = state.paths.data_dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let skipped: serde_json::Map<String, serde_json::Value> = skipped
        .iter()
        .map(|name| (name.to_string(), json!("site not found")))
        .collect();
    let report = json!({"sha256": "ab", "applied": ["Fix Alpha"], "skipped": skipped});
    std::fs::write(out.join("patch_status.json"), report.to_string()).unwrap();
}

#[test]
fn the_hint_line_names_the_patches_the_last_start_left_out() {
    // Ruling R85: the tail of the 2.6.0 status line, in the hint's place while the game is idle.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let dump = common::dump_with_patches(dir.path());
    state
        .settings
        .set("game_dir", json!(dump.display().to_string()));
    last_start_skipped(&state, &["Fix Beta", "Fix Gamma"]);
    home::refresh_facts(&mut state);
    let note = "2 patches not found in this eboot.bin at the last start (see Log)";
    assert_eq!(home::status_line(&state), note);
    assert!(simulator(app::view(&state)).find(note).is_ok());
    state.lang = "pt".into();
    assert_eq!(
        home::status_line(&state),
        "2 patches não encontrados neste eboot.bin na última inicialização (veja o Registro)"
    );
    state.lang = "en".into();
    let hint = "In the game: Insert or R3+L2 opens the port's menu.";
    // Every patch applied: the hint.
    last_start_skipped(&state, &[]);
    home::refresh_facts(&mut state);
    assert_eq!(home::status_line(&state), hint);
    // As the Python's summary: no note without a game in the game folder.
    last_start_skipped(&state, &["Fix Beta"]);
    state.settings.set("game_dir", json!(""));
    home::refresh_facts(&mut state);
    assert_eq!(home::status_line(&state), hint);
}

#[test]
fn the_home_shows_the_launcher_key_art_without_a_game_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.settings.set("game_dir", "".into());
    home::refresh_facts(&mut state);
    let mut ui = simulator(app::view(&state));
    assert!(
        ui.find(iced_test::selector::id(home::ART_ID)).is_ok(),
        "no key art layer on the Home"
    );
}

#[test]
fn the_key_art_is_the_embedded_1920_by_1080_picture() {
    let art = home::art();
    assert_eq!(art.id(), home::art().id(), "one handle per process");
    let Handle::Bytes(_, bytes) = art else {
        panic!("the key art is not embedded: {art:?}");
    };
    let picture = image::load_from_memory(&bytes).unwrap();
    assert_eq!((picture.width(), picture.height()), (1920, 1080));
    assert_eq!(home::ART_SIZE, (1920, 1080));
    // A scene of the picture's own size shows all of it.
    assert_eq!(
        home::art_crop(home::ART_SIZE, Size::new(1920.0, 1080.0)),
        crop(0, 0, 1920, 1080)
    );
}

#[test]
fn the_glow_sprite_falls_off_smoothly_from_its_centre() {
    let red = iced::Color::from_rgb8(200, 30, 37);
    let sprite = home::glow_sprite(red);
    assert_eq!(
        sprite.id(),
        home::glow_sprite(red).id(),
        "one handle per colour"
    );
    assert_ne!(
        sprite.id(),
        home::glow_sprite(iced::Color::from_rgb8(130, 130, 130)).id()
    );
    let Handle::Rgba {
        width,
        height,
        pixels,
        ..
    } = sprite
    else {
        panic!("the sprite is not raw pixels: {sprite:?}");
    };
    assert_eq!(width, height);
    let pixel = |x: u32, y: u32| {
        let at = 4 * (y * width + x) as usize;
        [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
    };
    let middle = width / 2;
    // The colour everywhere, so the filtered edge does not darken.
    assert_eq!(pixel(0, 0)[..3], [200, 30, 37]);
    assert_eq!(pixel(middle, middle)[..3], [200, 30, 37]);
    // From the centre to the edge the alpha only falls, a few levels at a time, down to 0.
    let alphas: Vec<u8> = (middle..width).map(|x| pixel(x, middle)[3]).collect();
    assert!(alphas[0] >= 250, "centre {}", alphas[0]);
    assert_eq!(*alphas.last().unwrap(), 0);
    for (inner, outer) in alphas.iter().zip(&alphas[1..]) {
        assert!(outer <= inner && inner - outer <= 4, "{alphas:?}");
    }
    // Round: the same alpha along a column as along a row.
    assert_eq!(pixel(middle, width - 20)[3], pixel(width - 20, middle)[3]);
}

#[test]
fn the_home_draws_the_key_art_unless_a_test_sets_another_picture() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let shown = home::shown_art(&state);
    assert_eq!(shown.handle.id(), home::art().id());
    assert_eq!(shown.size, home::ART_SIZE);
    // The snapshots draw a small placeholder through the same layer.
    state.art_override = Some(home::Art {
        handle: Handle::from_rgba(2, 1, vec![0x2a; 8]),
        size: (2, 1),
    });
    assert_eq!(home::shown_art(&state).size, (2, 1));
}

fn crop(x: u32, y: u32, width: u32, height: u32) -> Rectangle<u32> {
    Rectangle {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn the_key_art_covers_the_scene_anchored_at_70_and_30_percent() {
    // A narrow scene shows a slice 70 % of the way across the picture.
    assert_eq!(
        home::art_crop(home::ART_SIZE, Size::new(960.0, 1080.0)),
        crop(672, 0, 960, 1080)
    );
    // A wide scene shows a band 30 % of the way down.
    assert_eq!(
        home::art_crop(home::ART_SIZE, Size::new(1920.0, 540.0)),
        crop(0, 162, 1920, 540)
    );
    // A larger scene of the same shape scales the whole picture up.
    assert_eq!(
        home::art_crop(home::ART_SIZE, Size::new(3840.0, 2160.0)),
        crop(0, 0, 1920, 1080)
    );
    // Any picture size: a 32 x 18 placeholder in a half-width scene.
    assert_eq!(
        home::art_crop((32, 18), Size::new(16.0, 18.0)),
        crop(11, 0, 16, 18)
    );
    // A minimised window has no scene: the whole picture, no division by zero.
    assert_eq!(
        home::art_crop(home::ART_SIZE, Size::ZERO),
        crop(0, 0, 1920, 1080)
    );
}

#[test]
fn the_rust_only_home_and_splash_texts_have_portuguese() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.lang = "pt".into();
    // The wording of the Python's "• No saves yet: the game creates them in {}".
    assert_eq!(home::chips(&state)[0], "• Nenhum save ainda");
    state.splash.done = false;
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("qualquer tecla pula").is_ok());
}

#[test]
fn the_graphics_card_chip_shows_only_the_name_of_the_card_the_game_uses() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let card = |index: &str, name: &str, chosen: bool| GpuRow {
        index: index.into(),
        name: name.into(),
        kind: "discrete".into(),
        msaa: None,
        chosen,
    };
    state.gpus = vec![
        card("0", "Intel(R) Arc(TM) Graphics", false),
        card("1", "NVIDIA GeForce RTX 4060", true),
    ];
    // The probe's pick, by its name alone, in every language.
    assert_eq!(home::chips(&state)[2], "✓ NVIDIA GeForce RTX 4060");
    // The Graphics card setting wins over the probe's pick.
    state.settings.set("gpu_id", json!("0"));
    assert_eq!(home::chips(&state)[2], "✓ Intel(R) Arc(TM) Graphics");
    state.lang = "pt".into();
    assert_eq!(home::chips(&state)[2], "✓ Intel(R) Arc(TM) Graphics");
    // No card known: the probe did not answer.
    state.gpus.clear();
    assert_eq!(home::chips(&state)[2], "• GPU: não verificada");
    state.lang = "en".into();
    assert_eq!(home::chips(&state)[2], "• GPU: not checked");
}

#[test]
fn saves_chip_follows_the_savedata_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let saves = state
        .paths
        .user_dir(&state.settings.get_str("user_dir"))
        .join("savedata");
    std::fs::create_dir_all(&saves).unwrap();
    std::fs::write(saves.join("slot0"), b"x").unwrap();
    home::refresh_facts(&mut state);
    // Only that saves exist: the folder is on the Game page.
    assert_eq!(home::chips(&state)[0], "✓ Saves found");
    state.lang = "pt".into();
    assert_eq!(home::chips(&state)[0], "✓ Saves encontrados");
}

#[test]
fn hover_opens_after_450ms_and_leave_closes_after_300ms() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let t0 = Instant::now();
    state.now = t0;
    let ms = |n: u64| t0 + Duration::from_millis(n);

    lang(&mut state, LangMessage::Enter);
    tick(&mut state, ms(449));
    assert!(!state.lang_menu.open);
    tick(&mut state, ms(450));
    assert!(state.lang_menu.open);

    lang(&mut state, LangMessage::Leave);
    let left = state.now;
    tick(&mut state, left + Duration::from_millis(299));
    assert!(state.lang_menu.open);
    tick(&mut state, left + Duration::from_millis(300));
    assert!(!state.lang_menu.open);
}

#[test]
fn coming_back_before_300ms_keeps_the_menu_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    lang(&mut state, LangMessage::Toggle);
    assert!(state.lang_menu.open);
    let t0 = state.now;
    lang(&mut state, LangMessage::Leave);
    tick(&mut state, t0 + Duration::from_millis(200));
    lang(&mut state, LangMessage::Enter);
    tick(&mut state, t0 + Duration::from_millis(400));
    assert!(state.lang_menu.open);
}

#[test]
fn click_toggles_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    assert!(!state.lang_menu.open);
    for message in click(&state, "English ▾") {
        let _ = app::update(&mut state, message);
    }
    assert!(state.lang_menu.open);
    for message in click(&state, "English ▾") {
        let _ = app::update(&mut state, message);
    }
    assert!(!state.lang_menu.open);
}

#[test]
fn the_open_menu_lists_every_language_and_marks_the_current_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    lang(&mut state, LangMessage::Toggle);
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("✓ English").is_ok());
    for name in [
        "Español",
        "Português (Brasil)",
        "Français",
        "Русский",
        "العربية",
        "日本語",
        "Bahasa Indonesia",
    ] {
        assert!(ui.find(name).is_ok(), "{name} missing");
    }
    let messages = {
        ui.click("Español").unwrap();
        ui.into_messages().collect::<Vec<_>>()
    };
    assert!(matches!(
        messages.as_slice(),
        [Message::Lang(LangMessage::Pick(code))] if code == "es"
    ));
}

#[test]
fn picking_portuguese_changes_the_tab_labels() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    lang(&mut state, LangMessage::Toggle);
    lang(&mut state, LangMessage::Pick("pt".to_string()));
    assert_eq!(state.lang, "pt");
    assert_eq!(state.settings.get_str("ui_language"), "pt");
    assert!(!state.lang_menu.open);
    assert_eq!(tr("pt", "Graphics"), "Gráficos");
    {
        let mut ui = simulator(app::view(&state));
        assert!(ui.find("Gráficos").is_ok());
        assert!(ui.find("Português (Brasil) ▾").is_ok());
    }

    lang(&mut state, LangMessage::Toggle);
    let mut ui = simulator(app::view(&state));
    for header in ["Américas", "Europa", "Ásia e Oriente Médio"] {
        assert!(ui.find(header).is_ok(), "{header} missing");
    }
}

#[test]
fn arabic_sets_rtl() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    assert!(!state.rtl);
    lang(&mut state, LangMessage::Pick("ar".to_string()));
    assert!(state.rtl);
    lang(&mut state, LangMessage::Pick("en".to_string()));
    assert!(!state.rtl);
    assert_eq!(state.settings.get("ui_language"), Some(&json!("en")));
}

#[test]
fn play_gets_the_click_under_the_narrow_drawer() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let _ = app::update(&mut state, Message::ShowPage(Page::Graphics));
    // Let the slide finish.
    tick(&mut state, Instant::now() + Duration::from_secs(1));
    assert_eq!(state.drawer, DrawerState::Open(Page::Graphics));
    let messages = click(&state, "PLAY");
    assert!(messages.iter().any(|m| matches!(m, Message::Play)));
    // The dim layer under the drawer does not take the click too.
    assert!(!messages.iter().any(|m| matches!(m, Message::CloseDrawer)));
    for message in messages {
        let _ = app::update(&mut state, message);
    }
    // Without a game folder PLAY refuses and opens the Game page in the drawer.
    assert_eq!(state.drawer, DrawerState::Open(Page::Game));
}

#[test]
fn play_is_disabled_behind_the_wide_drawer() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let _ = app::update(&mut state, Message::ShowPage(Page::Trophies));
    tick(&mut state, Instant::now() + Duration::from_secs(1));
    let mut ui = simulator(app::view(&state));
    let _ = ui.click("PLAY");
    assert!(!ui
        .into_messages()
        .any(|message| matches!(message, Message::Play)));
}

fn version(state: &mut State, message: VersionMessage) -> Task<Message> {
    app::update(state, Message::VersionMenu(message))
}

fn release(channel: Channel, version: &str, build: Option<&str>) -> Release {
    Release {
        id: ReleaseId {
            channel,
            version: version.to_string(),
            build: build.map(str::to_string),
        },
        published: "2026-10-01".to_string(),
        url: "https://example.test/pkg".to_string(),
        page: "https://example.test/page".to_string(),
    }
}

fn ready_with(state: &mut State, releases: Vec<Release>) {
    state.update = UpdateState::Ready(releases);
}

/// The installed release the labels below name, whatever `VERSION` is.
fn stable_2_6_0() -> ReleaseId {
    ReleaseId {
        channel: Channel::Release,
        version: "2.6.0".into(),
        build: None,
    }
}

#[test]
fn the_version_button_names_the_installed_release() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = ReleaseId::fallback();
    let mut ui = simulator(app::view(&state));
    assert!(ui
        .find(format!("Release · {} ▾", spartanport_launcher::VERSION).as_str())
        .is_ok());
    drop(ui);
    state.installed = ReleaseId {
        channel: Channel::PreRelease,
        version: "2.6.0".into(),
        build: Some("issue209-test1".into()),
    };
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Pre-release · 2.6.0 · issue209-test1 ▾").is_ok());
}

#[test]
fn the_version_button_offers_a_newer_stable() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = stable_2_6_0();
    ready_with(&mut state, vec![release(Channel::Release, "2.7.0", None)]);
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Release · 2.6.0 · 2.7.0 available ▾").is_ok());
    drop(ui);
    state.lang = "pt".into();
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Lançamento · 2.6.0 · 2.7.0 disponível ▾").is_ok());
    drop(ui);
    state.lang = "en".into();
    ready_with(
        &mut state,
        vec![release(Channel::PreRelease, "2.6.0", Some("x"))],
    );
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("Release · 2.6.0 ▾").is_ok());
}

#[test]
fn the_version_button_sits_before_the_language_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = stable_2_6_0();
    let label = "Release · 2.6.0 ▾";
    let x = |state: &State, label: &str| {
        let mut ui = simulator(app::view(state));
        ui.find(label).unwrap().bounds().x
    };
    assert!(x(&state, label) < x(&state, "English ▾"));
    state.rtl = true;
    assert!(x(&state, label) > x(&state, "English ▾"));
}

#[test]
fn the_version_button_is_not_pressable_while_downloading_or_dimmed() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = stable_2_6_0();
    let newest = release(Channel::Release, "2.7.0", None);
    state.update = UpdateState::Downloading {
        releases: vec![newest.clone()],
        release: newest.clone(),
        percent: 42,
    };
    let label = "Downloading version 2.7.0… 42%";
    assert!(simulator(app::view(&state)).find(label).is_ok());
    assert!(!click(&state, label)
        .iter()
        .any(|message| matches!(message, Message::VersionMenu(VersionMessage::Toggle))));

    state.update = UpdateState::None;
    state.modal = Some(Modal {
        kind: ModalKind::Info,
        title: "T".into(),
        text: "X".into(),
    });
    // The dialog's backdrop takes the press.
    let sent = click(&state, "Release · 2.6.0 ▾");
    assert!(!sent
        .iter()
        .any(|message| matches!(message, Message::VersionMenu(VersionMessage::Toggle))));

    // Dimmed behind a wide drawer: the dim layer takes the click, the button hears nothing.
    state.modal = None;
    state.drawer = DrawerState::Open(Page::Log);
    let sent = click(&state, "Release · 2.6.0 ▾");
    assert!(
        !sent
            .iter()
            .any(|message| matches!(message, Message::VersionMenu(_))),
        "{sent:?}"
    );
}

#[test]
fn the_version_menu_stays_closed_while_downloading() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = ReleaseId::fallback();
    let newest = release(Channel::Release, "2.7.0", None);
    let downloading = UpdateState::Downloading {
        releases: vec![newest.clone()],
        release: newest.clone(),
        percent: 42,
    };
    state.update = downloading.clone();
    let t0 = Instant::now();
    state.now = t0;
    for message in [
        VersionMessage::Enter,
        VersionMessage::Tick(t0 + Duration::from_millis(450)),
        VersionMessage::Toggle,
    ] {
        let task = version(&mut state, message.clone());
        assert!(!state.version_menu.open, "{message:?}");
        assert!(state.version_menu.hover_since.is_none(), "{message:?}");
        assert_eq!(task.units(), 0, "{message:?}");
    }
    // The bare button: neither the pointer nor a press reaches the menu.
    let sent = click(&state, "Downloading version 2.7.0… 42%");
    assert!(
        !sent
            .iter()
            .any(|message| matches!(message, Message::VersionMenu(_))),
        "{sent:?}"
    );

    // A download that starts closes an open list.
    ready_with(&mut state, vec![newest]);
    open_menu(&mut state);
    state.update = downloading;
    tick(&mut state, t0 + Duration::from_millis(500));
    assert!(!state.version_menu.open);
}

#[test]
fn hovering_the_version_button_opens_and_fetches() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let t0 = Instant::now();
    state.now = t0;
    assert!(matches!(state.update, UpdateState::None));
    let _ = version(&mut state, VersionMessage::Enter);
    tick(&mut state, t0 + Duration::from_millis(450));
    assert!(state.version_menu.open);
    assert!(matches!(state.update, UpdateState::Checking));

    let mut state = state_in(dir.path());
    ready_with(&mut state, vec![release(Channel::Release, "2.7.0", None)]);
    let _ = version(&mut state, VersionMessage::Toggle);
    assert!(state.version_menu.open);
    assert!(matches!(state.update, UpdateState::Ready(_)));
}

#[test]
fn opening_one_menu_closes_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    lang(&mut state, LangMessage::Toggle);
    let _ = version(&mut state, VersionMessage::Toggle);
    assert!(state.version_menu.open && !state.lang_menu.open);
    lang(&mut state, LangMessage::Toggle);
    assert!(state.lang_menu.open && !state.version_menu.open);
}

#[test]
fn sliding_from_language_to_version_opens_only_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let t0 = Instant::now();
    state.now = t0;
    lang(&mut state, LangMessage::Toggle);
    assert!(state.lang_menu.open);
    lang(&mut state, LangMessage::Leave);
    let _ = version(&mut state, VersionMessage::Enter);
    tick(&mut state, t0 + Duration::from_millis(450));
    assert!(state.version_menu.open);
    assert!(!state.lang_menu.open);
}

fn five_stables_and_ten_prereleases() -> Vec<Release> {
    let mut list: Vec<Release> = ["2.9.0", "2.8.0", "2.7.0", "2.6.0", "2.5.0"]
        .into_iter()
        .map(|v| release(Channel::Release, v, None))
        .collect();
    list.extend(
        (1..=10).map(|n| release(Channel::PreRelease, "2.6.0", Some(format!("t{n}").as_str()))),
    );
    list
}

fn open_menu(state: &mut State) {
    let _ = version(state, VersionMessage::Toggle);
    assert!(state.version_menu.open);
}

#[test]
fn the_open_menu_lists_three_stables_and_every_prerelease() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = stable_2_6_0();
    ready_with(&mut state, five_stables_and_ten_prereleases());
    open_menu(&mut state);
    let mut ui = simulator(app::view(&state));
    for label in [
        "Releases",
        "Pre-releases",
        "2.9.0",
        "2.8.0",
        "2.7.0",
        "2.6.0 · t1",
        "2.6.0 · t10",
    ] {
        assert!(ui.find(label).is_ok(), "{label:?} missing");
    }
    assert!(ui.find("2.6.0").is_err());
}

#[test]
fn the_menu_marks_the_installed_and_the_new() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = ReleaseId {
        channel: Channel::Release,
        version: "2.7.0".into(),
        build: None,
    };
    ready_with(
        &mut state,
        vec![
            release(Channel::Release, "2.8.0", None),
            release(Channel::Release, "2.7.0", None),
            release(Channel::Release, "2.6.0", None),
        ],
    );
    open_menu(&mut state);
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("✓ 2.7.0").is_ok());
    assert!(ui.find("2.8.0").is_ok());
    assert!(ui.find("new").is_ok());
    assert!(ui.find("2.6.0").is_ok());
    assert!(ui.find("✓ 2.6.0").is_err());
    assert!(ui.find("✓ 2.8.0").is_err());
    drop(ui);

    // A stamp that says pre-release still matches the stable row of its version.
    state.installed.channel = Channel::PreRelease;
    let mut ui = simulator(app::view(&state));
    assert!(ui.find("✓ 2.7.0").is_ok());
}

#[test]
fn an_unmatched_install_has_no_tick_but_still_a_new() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = stable_2_6_0();
    ready_with(
        &mut state,
        vec![
            release(Channel::Release, "2.8.0", None),
            release(Channel::Release, "2.7.0", None),
        ],
    );
    open_menu(&mut state);
    let mut ui = simulator(app::view(&state));
    for label in ["✓ 2.8.0", "✓ 2.7.0", "✓ 2.6.0"] {
        assert!(ui.find(label).is_err(), "{label:?} present");
    }
    assert!(ui.find("new").is_ok());
}

fn menu_height(prereleases: usize) -> f32 {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let mut list = vec![release(Channel::Release, "2.7.0", None)];
    list.extend(
        (1..=prereleases)
            .map(|n| release(Channel::PreRelease, "2.6.0", Some(format!("t{n}").as_str()))),
    );
    ready_with(&mut state, list);
    open_menu(&mut state);
    let mut ui = simulator(app::view(&state));
    ui.find(iced_test::selector::id(
        spartanport_launcher::ui::version_menu::MENU_ID,
    ))
    .unwrap()
    .bounds()
    .height
}

#[test]
fn the_prerelease_list_scrolls_after_eight_rows() {
    // A row is about 28.25 pt against ROW_HEIGHT's 29, so eight rows sit just under the cap:
    // the card stops growing once the ninth arrives.
    let capped = menu_height(20);
    assert_eq!(capped, menu_height(9));
    assert!(menu_height(8) < capped);
    assert!(menu_height(7) < menu_height(8));
}

#[test]
fn the_menu_says_when_there_are_no_test_builds() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    ready_with(&mut state, vec![release(Channel::Release, "2.7.0", None)]);
    open_menu(&mut state);
    let expected = if cfg!(windows) {
        "No test builds"
    } else {
        "No test builds for Linux"
    };
    assert!(simulator(app::view(&state)).find(expected).is_ok());
}

#[test]
fn the_menu_says_fetching_and_failed() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.version_menu.open = true;
    state.update = UpdateState::None;
    assert!(simulator(app::view(&state)).find("Fetching…").is_ok());
    state.update = UpdateState::Failed("offline".into());
    assert!(simulator(app::view(&state))
        .find("Could not fetch the versions")
        .is_ok());
}

#[test]
fn picking_a_row_closes_and_installs() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    state.installed = stable_2_6_0();
    ready_with(&mut state, five_stables_and_ten_prereleases());
    open_menu(&mut state);
    let sent = click(&state, "2.8.0");
    let picked = match sent.as_slice() {
        [Message::VersionMenu(VersionMessage::Pick(r))] => r.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(picked.id.version, "2.8.0");
    let task = version(&mut state, VersionMessage::Pick(picked));
    assert!(!state.version_menu.open);
    if cfg!(windows) {
        assert!(task.units() > 0 || state.modal.is_some());
    }

    let mut state = state_in(dir.path());
    state.installed = ReleaseId::fallback();
    ready_with(&mut state, five_stables_and_ten_prereleases());
    open_menu(&mut state);
    // The row's channel differs from the stamp's: still the installed release.
    let installed = release(Channel::PreRelease, &state.installed.version, None);
    let task = version(&mut state, VersionMessage::Pick(installed));
    assert!(!state.version_menu.open);
    assert_eq!(task.units(), 0);
    assert!(state.modal.is_none());
}
