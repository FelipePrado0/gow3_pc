#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The splash phase: skip, fly to the corner, done.

mod common;

use std::time::Duration;

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Location, Modifiers};
use iced_test::simulator::simulator;
use spartanport_launcher::core::paths::Paths;
use spartanport_launcher::ui::app::{self, Message, State};
use spartanport_launcher::ui::fire;
use spartanport_launcher::ui::splash::{self, Check, SplashMessage};

fn state_in(dir: &std::path::Path) -> State {
    let root = dir.display().to_string();
    let env = move |key: &str| match key {
        "APPDATA" | "USERPROFILE" | "HOME" | "XDG_CONFIG_HOME" => Some(root.clone()),
        _ => None,
    };
    State::new(Paths::from_env(dir, &env))
}

#[test]
fn the_splash_shows_the_settings_then_the_update_check() {
    // Rulings R86 and R97: the splash checks the settings and the updates, each text for half of
    // the 0.8 s. The graphics cards and the game folder are read in the background without a text
    // of their own and do not hold the splash; their results still reach the Home.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let start = state.splash.started;
    let _ = app::update(
        &mut state,
        Message::Splash(SplashMessage::CheckDone(Check::Update(None))),
    );
    for (at, shown) in [
        (100, "Checking the settings…"),
        (500, "Checking for updates…"),
    ] {
        let _ = app::update(&mut state, Message::Tick(start + Duration::from_millis(at)));
        let mut ui = simulator(app::view(&state));
        assert!(ui.find(shown).is_ok(), "{shown} missing at {at} ms");
        assert!(!state.splash.flying(), "flying at {at} ms");
    }
    for gone in [
        "Checking Vulkan graphics cards…",
        "Checking the game folder…",
    ] {
        assert!(simulator(app::view(&state)).find(gone).is_err(), "{gone}");
    }
    let _ = app::update(
        &mut state,
        Message::Tick(start + Duration::from_millis(800)),
    );
    assert!(state.splash.flying());
    // The background checks answer later.
    let _ = app::update(
        &mut state,
        Message::Splash(SplashMessage::CheckDone(Check::GameFolder(true))),
    );
    assert_eq!(state.game_ready, Some(true));
}

#[test]
fn no_fire_burns_until_the_monogram_flies() {
    // R96: the fire does not start with the splash; it comes in with the Home, as the flight fades
    // the splash's dark background out.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let start = state.splash.started;
    let _ = app::update(
        &mut state,
        Message::Splash(SplashMessage::CheckDone(Check::Update(None))),
    );
    for at in [33, 400, 799] {
        let _ = app::update(&mut state, Message::Tick(start + Duration::from_millis(at)));
        assert_eq!(state.fire.intensity(), 0.0, "{at} ms");
        assert_eq!(state.fire.particle_count(), 0, "{at} ms");
    }
    // The flight starts at 0.8 s; from the next frame the Home's fire fills in.
    let _ = app::update(
        &mut state,
        Message::Tick(start + Duration::from_millis(800)),
    );
    assert!(state.splash.flying());
    let _ = app::update(
        &mut state,
        Message::Tick(start + Duration::from_millis(833)),
    );
    assert_eq!(state.fire.intensity(), 0.8);
    assert!(state.fire.particle_count() > 0);
}

#[test]
fn the_home_alone_draws_the_fire() {
    // R96: the splash layer has no fire of its own; the Home background holds the only one, under
    // the splash's dark background until the flight fades it out.
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let id = fire::layer_id();
    assert_eq!(common::count_ids(splash::view(&state, "en"), &id), 0);
    assert_eq!(common::count_ids(app::view(&state), &id), 1);
    state.splash.done = true;
    assert_eq!(common::count_ids(app::view(&state), &id), 1);
}

#[test]
fn a_key_skips_then_the_monogram_flies_and_the_splash_ends() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state_in(dir.path());
    let start = state.splash.started;
    drop(app::view(&state));

    let _ = app::update(
        &mut state,
        Message::Key(
            Key::Named(Named::Space),
            Modifiers::default(),
            Location::Standard,
        ),
    );
    assert!(state.splash.skipped);

    let _ = app::update(
        &mut state,
        Message::Tick(start + Duration::from_millis(300)),
    );
    assert!(!state
        .splash
        .fly
        .is_animating(start + Duration::from_millis(300)));

    let flying = start + Duration::from_millis(900);
    let _ = app::update(&mut state, Message::Tick(flying));
    assert!(state.splash.fly.is_animating(flying));
    assert!(!state.splash.done);
    drop(app::view(&state));

    let _ = app::update(
        &mut state,
        Message::Tick(start + Duration::from_millis(1700)),
    );
    assert!(state.splash.done);
    drop(app::view(&state));
}
