//! The Elm application: state, messages, update, view and subscriptions.

use std::path::PathBuf;
use std::time::Duration;

use iced::animation::{Animation, Easing};
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Location, Modifiers};
use iced::time::Instant;
use iced::widget::{button, column, container, mouse_area, row, space, stack, text};
use iced::{
    window, Background, Center, Color, Element, Fill, Point, Shrink, Size, Subscription, Task,
};
use serde_json::Value;

use crate::core::desktop::{linux_tools, open_path, LinuxTools};
use crate::core::gpu::{list_displays, monitor_options, monitor_selection, Display, GpuRow};
use crate::core::i18n::{is_rtl, launcher_language, system_language, tr};
use crate::core::ini::{read_ini, Ini, INI_DEFAULTS, INI_FLAGS};
use crate::core::log::{LogBuffer, MAX_LOG_LINES};
use crate::core::options::aspect_of;
use crate::core::paths::Paths;
use crate::core::release::ReleaseId;
use crate::core::settings::{read_app_settings, Settings, SettingsFile};
use crate::core::update::{can_self_update, installed_release, stamp_path};
use crate::ui::drawer::{self, DrawerState};
use crate::ui::fire::{intensity_for, probe_gpu, Fire};
use crate::ui::home::{self, HomeFacts};
use crate::ui::hover_menu::HoverMenu;
use crate::ui::lang_menu::{self, LangMenu, LangMessage};
use crate::ui::pages::advanced;
use crate::ui::pages::controls::{self, ControlsMessage, ControlsView};
use crate::ui::pages::game::{self, BackupMessage, PatchesView};
use crate::ui::pages::log::{self, LogMessage};
use crate::ui::pages::mods::{self, ModsMessage, ModsView};
use crate::ui::pages::performance;
use crate::ui::pages::trophies::{self, TrophiesMessage, TrophiesView};
use crate::ui::pages::Page;
use crate::ui::persist::{self, Persisted};
use crate::ui::play::{self, GameMessage, GameState};
use crate::ui::setup::{self, SetupMessage, SetupState};
use crate::ui::splash::{self, SplashMessage, SplashState};
use crate::ui::tasks::blocking;
use crate::ui::theme::{self, palette, window_zoom, CINZEL_BOLD};
use crate::ui::update::{self, UpdateMessage, UpdateState};
use crate::ui::version_menu::{self, VersionMessage};
use crate::ui::widgets;
use crate::ui::widgets::modal::{Modal, ModalKind, ModalMessage};
use crate::VERSION;

/// Starting and minimum window size, in logical pixels.
pub const WINDOW_SIZE: Size = Size::new(1280.0, 800.0);
/// Height of the bottom tab bar.
pub const BAR_HEIGHT: f32 = 52.0;
/// Clock period while nothing animates.
const IDLE_CLOCK: Duration = Duration::from_millis(100);
/// Drawer slide duration.
pub const DRAWER_SLIDE: Duration = Duration::from_millis(220);

/// Everything the window shows; only `update` changes it.
pub struct State {
    pub paths: Paths,
    pub settings: Settings,
    pub ini: Ini,
    /// What the files held after the last save (or at the start): Save is active while the
    /// window differs from it (`persist::dirty`).
    pub saved: Option<Persisted>,
    pub page: Page,
    pub drawer: DrawerState,
    pub drawer_progress: Animation<bool>,
    /// The share of the window the drawer takes, sliding between the narrow and the wide size
    /// when the open page changes (ruling R81).
    pub drawer_fraction: Animation<f32>,
    pub lang: String,
    /// Whether the interface language reads right to left (Arabic).
    pub rtl: bool,
    /// The interface zoom (`theme::ui_scale`): screen pixels per logical pixel.
    pub scale: f32,
    /// The desktop's own scale where the window opened; 1.0 until the window reports it.
    pub desktop_scale: f32,
    pub splash: SplashState,
    pub game: GameState,
    /// The Home status line while the game starts, runs or its system files are copied; empty
    /// otherwise, when the line shows the in-game menu hint.
    pub status: String,
    /// What PLAY starts instead of `<exe> --run` (tests start the fake game); `None` in use.
    pub launch_command: Option<Vec<String>>,
    /// The program PLAY starts with `--run`; `None` is this program (tests name a copy in a port).
    pub launcher_exe: Option<PathBuf>,
    pub log: LogBuffer,
    pub gpus: Vec<GpuRow>,
    /// The Aspect ratio filter of the Performance page: not saved, it follows the render resolution.
    pub aspect: String,
    /// Sizes the enabled mods add to the render resolutions; see `performance::refresh_mod_resolutions`.
    pub mod_resolutions: Vec<String>,
    pub displays: Vec<Display>,
    /// The window system the monitors were listed under; `persist::refresh_monitors` lists them
    /// again when the pending one differs.
    pub displays_backend: String,
    pub monitor_pairs: Vec<(String, String)>,
    pub trophies: TrophiesView,
    pub mods: ModsView,
    pub controls: ControlsView,
    pub patches: PatchesView,
    /// The names of the save backups, newest first.
    pub backups: Vec<String>,
    /// The backup chosen in the Saves row.
    pub backup_choice: String,
    pub update: UpdateState,
    /// The installed release: the stamp beside the launcher, or this version.
    pub installed: ReleaseId,
    /// The percentages the update worker sends; `update::drain` empties it on each clock tick.
    pub update_progress: Option<std::sync::mpsc::Receiver<u8>>,
    pub lang_menu: LangMenu,
    pub version_menu: HoverMenu,
    /// What the Home chips read from the disk.
    pub home: HomeFacts,
    /// Another picture for the Home art layer instead of the key art; only the snapshot tests set
    /// it, to keep the reference images small.
    pub art_override: Option<home::Art>,
    pub modal: Option<Modal>,
    /// Whether `settings.json` or the ini could not be read at start and nothing was saved since:
    /// only Save and PLAY write then (spec §6), not closing, opening the ini or Add to Steam.
    pub settings_unreadable: bool,
    /// What `settings.json` was at the start; `State::open` opens the setup on `Missing`.
    pub settings_file: SettingsFile,
    /// The first-run setup while it is open; `None` on every other start.
    pub setup: Option<SetupState>,
    /// Whether the Home fire burns (the `launcher_effects` setting).
    pub effects: bool,

    /// What the Linux options of the Advanced page can use, probed once at the start.
    pub linux_tools: LinuxTools,
    /// Whether this install can replace itself (Windows, or an AppImage).
    pub self_update: bool,
    pub fire: Fire,
    /// Whether the window is shown; `close_on_play` hides it while the game runs.
    pub visible: bool,
    /// Whether the window has the focus.
    pub focused: bool,
    /// Whether the game folder passed the splash check; `None` until it ran.
    pub game_ready: Option<bool>,
    /// Logical window size, for the drawer width.
    pub window_size: Size,
    /// The launcher window, once it has opened.
    pub window: Option<window::Id>,
    /// Clock of the animations, advanced by `Tick` and by the messages that start them.
    pub now: Instant,
}

impl State {
    /// The state the program starts with: `new`, plus the first-run setup when there is no
    /// `settings.json` yet.
    pub fn open(paths: Paths) -> State {
        let mut state = State::new(paths);
        if state.settings_file == SettingsFile::Missing {
            state.setup = Some(SetupState::new(&state));
        }
        state
    }

    /// Reads `settings.json` and the ini (one warning when either is there but unreadable);
    /// everything else starts empty.
    pub fn new(paths: Paths) -> State {
        let (settings, settings_file) = read_app_settings(&paths);
        let (ini, ini_unreadable) = read_ini(&paths.ini_path());
        let unreadable: Vec<PathBuf> = [
            (
                settings_file == SettingsFile::Unreadable,
                paths.config_file(),
            ),
            (ini_unreadable, paths.ini_path()),
        ]
        .into_iter()
        .filter_map(|(failed, file)| failed.then_some(file))
        .collect();
        let lang = launcher_language(&settings.get_str("ui_language"), system_language);
        // The desktop's scale, which `window_zoom` takes out, is known once the window opens.
        let scale = theme::ui_scale(&settings.get_str("ui_scale"));
        let now = Instant::now();
        let rtl = is_rtl(&lang);
        let aspect = aspect_of(
            ini.values
                .get("render_resolution")
                .map_or("", String::as_str),
        )
        .unwrap_or_else(|| "16:9".to_string());
        let controls = ControlsView::new(&settings);
        let effects = advanced::effects_enabled(&settings);
        // `start_monitor_listing` lists them under this one.
        let displays_backend = settings.get_str("video_backend");
        let installed = installed_release(&stamp_path(
            &paths.port_dir,
            std::env::var_os("APPDIR")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .as_deref(),
            cfg!(windows),
        ));
        let mut state = State {
            paths,
            settings,
            ini,
            saved: None,
            page: Page::Home,
            drawer: DrawerState::Closed,
            // EaseOutQuint follows the spec's cubic-bezier(.2, .8, .2, 1) within 0.01; lilt's
            // `Easing::Custom` can take the exact curve if it is ever needed.
            drawer_progress: Animation::new(false)
                .duration(DRAWER_SLIDE)
                .easing(Easing::EaseOutQuint),
            drawer_fraction: drawer::fraction_animation(drawer::drawer_fraction(Page::Home)),
            lang,
            rtl,
            scale,
            desktop_scale: 1.0,
            splash: SplashState::new(now),
            game: Default::default(),
            status: String::new(),
            launch_command: None,
            launcher_exe: None,
            log: LogBuffer::new(MAX_LOG_LINES),
            gpus: Vec::new(),
            aspect,
            mod_resolutions: Vec::new(),
            displays: Vec::new(),
            displays_backend,
            monitor_pairs: Vec::new(),
            trophies: Default::default(),
            mods: Default::default(),
            controls,
            patches: Default::default(),
            backups: Vec::new(),
            backup_choice: String::new(),
            update: Default::default(),
            installed,
            update_progress: None,
            lang_menu: Default::default(),
            version_menu: Default::default(),
            home: Default::default(),
            art_override: None,
            modal: None,
            settings_unreadable: !unreadable.is_empty(),
            settings_file,
            setup: None,
            effects,

            linux_tools: if cfg!(windows) {
                LinuxTools::default()
            } else {
                linux_tools()
            },
            self_update: can_self_update(&|key| std::env::var(key).ok(), cfg!(windows)),
            fire: Default::default(),
            visible: true,
            focused: true,
            game_ready: None,
            window_size: WINDOW_SIZE,
            window: None,
            now,
        };
        home::refresh_facts(&mut state);
        // The Mods page's lists now, as the Python launcher does, so the first visit does not count
        // as a change.
        state.mods = mods::load(&state.paths, &state.settings);
        performance::refresh_mod_resolutions(&mut state);
        game::refresh_game_patches(&mut state);
        game::refresh_backups(&mut state);
        // Nothing to save after opening.
        state.saved = Some(persist::gather(&state));
        state.modal = persist::unreadable_warning(&state.lang, &unreadable);
        state
    }
}

/// Everything that can happen: clicks, keys, the clock, the window and background work.
#[derive(Debug, Clone)]
pub enum Message {
    ShowPage(Page),
    CloseDrawer,
    NextPage,
    PreviousPage,
    Set(String, Value),
    /// The Aspect ratio box of the Performance page.
    Aspect(String),
    Browse(String),
    /// The folder of a folder setting: the dialog's answer, or the path typed into its field.
    Browsed(String, Option<PathBuf>),
    /// A game patch check box of the Game page.
    PatchToggled(String, bool),
    Backup(BackupMessage),
    /// The monitors the probe listed, in the background.
    MonitorsListed(Vec<Display>),
    /// Opens a file or folder with the desktop's program for it.
    OpenPath(PathBuf),
    /// Makes the folder when it is missing, then opens it (Python `open_path`).
    OpenFolder(PathBuf),
    /// Saves, then opens `spartanport.ini` (Python `open_path` of an `.ini`).
    OpenIni,
    /// The window's close button: saves first (Python `close`).
    CloseRequested(window::Id),
    Play,
    Stop,
    Save,
    Tick(Instant),
    /// A key press no widget took, with where the key sits (left, right or numpad).
    Key(Key, Modifiers, Location),
    WindowFocused(bool),
    Resized(Size),
    /// The window opened: its id and logical size.
    WindowOpened(window::Id, Size),
    /// The desktop's scale factor where the window opened.
    DesktopScale(f32),
    /// The desktop's scale factor changed under the window (another monitor, or the setting).
    DesktopRescaled(f32),
    /// The logical size of the monitor that shows the window, when the system tells it.
    MonitorSize(Option<Size>),
    Splash(SplashMessage),
    Lang(LangMessage),
    VersionMenu(VersionMessage),
    Game(GameMessage),
    Setup(SetupMessage),
    Update(UpdateMessage),
    Modal(ModalMessage),
    Trophies(TrophiesMessage),
    Mods(ModsMessage),
    Log(LogMessage),
    Controls(ControlsMessage),
    DesktopShortcut,
    /// The shortcut was made (the files it wrote; none on Windows) or why not.
    ShortcutDone(Result<Vec<String>, String>),
    AddToSteam,
    /// The Steam bridge finished: the dialog text.
    SteamDone(Result<String, String>),
    /// Delete shader cache (Performance) and Clear shader cache (Advanced): the size is read first.
    ClearShaderCache,
    /// The bytes the shader cache holds: none says so, any asks before deleting.
    ShaderCacheSized(u64),
    /// Yes to the question: the cache goes.
    ClearShaderCacheConfirmed,
    /// Done, or the cache folder that would not go.
    ShaderCacheCleared(Result<(), String>),
    Noop,
}

/// What a key press means here: Esc closes, the arrows change tab, Enter on Home plays.
pub fn key_message(state: &State, key: &Key) -> Option<Message> {
    match key {
        Key::Named(Named::Escape) => Some(Message::CloseDrawer),
        Key::Named(Named::ArrowLeft) => Some(Message::PreviousPage),
        Key::Named(Named::ArrowRight) => Some(Message::NextPage),
        Key::Named(Named::Enter)
            if state.page == Page::Home && state.drawer == DrawerState::Closed =>
        {
            Some(Message::Play)
        }
        _ => None,
    }
}

/// The only place the state changes.
pub fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::ShowPage(page) => return show_page(state, page),
        // A closed drawer shows Home, so the Home tab is the active one again.
        Message::CloseDrawer => return show_page(state, Page::Home),
        Message::NextPage => return show_page(state, state.page.next()),
        Message::PreviousPage => return show_page(state, state.page.previous()),
        Message::Key(key, _modifiers, location) => {
            // A dialog takes no keys, so none reaches what lies under it: Esc only closes a
            // dialog that asks nothing (ruling R66).
            if let Some(open) = &state.modal {
                let dismissible = matches!(open.kind, ModalKind::Info | ModalKind::Error);
                if dismissible && key == Key::Named(Named::Escape) {
                    state.modal = None;
                }
                return Task::none();
            }
            // While the splash shows, any key only skips it.
            if !state.splash.done {
                splash::apply(state, SplashMessage::Skip);
                return Task::none();
            }
            // During the setup the keyboard only answers Enter.
            if state.setup.is_some() {
                return setup::key_pressed(state, &key);
            }
            // While a key box of the Controls page waits, the key is its input.
            if state.controls.capturing.is_some() {
                return controls::key_pressed(state, &key, location);
            }
            return match key_message(state, &key) {
                Some(Message::Play) => Task::done(Message::Play),
                Some(message) => update(state, message),
                None => Task::none(),
            };
        }
        Message::Resized(size) => state.window_size = size,
        Message::WindowOpened(id, size) => {
            state.window = Some(id);
            state.window_size = size;
            return window::scale_factor(id).map(Message::DesktopScale);
        }
        // The monitor the window opened on: the zoom takes its scale out of GUI Scale.
        Message::DesktopScale(factor) => {
            state.desktop_scale = factor;
            state.scale = theme::ui_scale(&state.settings.get_str("ui_scale"));
            return fit_window(state);
        }
        // GUI Scale (1x or 1.5x) keeps its size on the new desktop; the window is neither refitted
        // nor moved. Its logical size changes with the zoom, so it is read again.
        Message::DesktopRescaled(factor) => {
            state.desktop_scale = factor;
            state.scale = theme::ui_scale(&state.settings.get_str("ui_scale"));
            if let Some(id) = state.window {
                return window::size(id).map(Message::Resized);
            }
        }
        Message::MonitorSize(monitor) => return size_window(state, monitor),
        Message::Tick(at) => {
            tick(state, at);
            lang_menu::update(state, LangMessage::Tick(at));
            let check = version_menu::update(state, VersionMessage::Tick(at));
            if state.version_menu.open {
                state.lang_menu.open = false;
            }
            controls::tick(state, at);
            update::drain(state);
            // The game's output arrives with the clock.
            return Task::batch([check, play::drain(state)]);
        }
        Message::Play => return play::play(state),
        Message::Stop => play::stop(state),
        Message::Save => return persist::save(state),
        // Save persists what `Set` stored.
        Message::Set(key, value) => apply_set(state, &key, value),
        Message::Aspect(aspect) => performance::choose_aspect(state, &aspect),
        Message::MonitorsListed(displays) => {
            let saved = state.settings.get_str("monitor");
            state.monitor_pairs = monitor_options(&displays);
            state.settings.set(
                "monitor",
                Value::String(monitor_selection(&displays, &saved)),
            );
            // So nothing is dirty right after opening.
            persist::follow_monitor_list(state, &displays);
            state.displays = displays;
        }
        // The result is ignored, as the Python launcher ignores a failed open.
        Message::OpenPath(path) => {
            return blocking(move || {
                let _ = open_path(&path);
            })
            .map(|()| Message::Noop);
        }
        // Task 21 filled Modal(Dismiss)
        Message::Modal(ModalMessage::Dismiss) => state.modal = None,
        Message::Browse(key) => return browse(state, key),
        Message::Browsed(key, Some(path)) => return game::folder_chosen(state, &key, &path),
        Message::Browsed(_, None) => {}
        Message::PatchToggled(name, on) => game::patch_toggled(state, &name, on),
        Message::Backup(message) => return game::backup_update(state, message),
        Message::WindowFocused(focused) => {
            state.focused = focused;
            // As the Python's <FocusOut>: a key box of the Controls page gives up.
            if !focused {
                return controls::update(state, ControlsMessage::CaptureTimeout);
            }
            // The player may have copied the game files in a file manager meanwhile.
            setup::refresh_folders(state);
        }
        Message::Splash(message) => splash::apply(state, message),
        Message::Lang(message) => {
            let picked = matches!(message, LangMessage::Pick(_));
            lang_menu::update(state, message);
            if state.lang_menu.open {
                state.version_menu.open = false;
            }
            // Picking a language stores it and rebuilds the window in it, as Save does; during
            // the setup nothing is written until Finish or Skip.
            if picked {
                if state.setup.is_some() {
                    return setup::language_picked(state);
                }
                return persist::save(state);
            }
        }
        Message::VersionMenu(message) => {
            let task = version_menu::update(state, message);
            if state.version_menu.open {
                state.lang_menu.open = false;
            }
            return task;
        }
        Message::Game(message) => return play::update(state, message),
        Message::Setup(message) => return setup::update(state, message),
        Message::Update(message) => return update::update(state, message),
        Message::Trophies(message) => return trophies::update(state, message),
        Message::Mods(message) => return mods::update(state, message),
        Message::Log(message) => return log::update(state, message),
        Message::Controls(message) => return controls::update(state, message),
        message @ (Message::DesktopShortcut
        | Message::ShortcutDone(_)
        | Message::AddToSteam
        | Message::SteamDone(_)
        | Message::ClearShaderCache
        | Message::ShaderCacheSized(_)
        | Message::ClearShaderCacheConfirmed
        | Message::ShaderCacheCleared(_)) => return advanced::update(state, message),
        Message::OpenFolder(folder) => {
            // As the Python's `mkdir`: Explorer would open Documents instead, and `xdg-open`
            // would fail without a word. A folder that cannot be made is opened all the same.
            let _ = std::fs::create_dir_all(&folder);
            return Task::done(Message::OpenPath(folder));
        }
        Message::OpenIni => {
            if persist::collect_unless_unreadable(state) {
                return Task::done(Message::OpenPath(state.paths.ini_path()));
            }
        }
        Message::CloseRequested(id) => {
            // Closing the window mid-setup saves nothing; the setup returns on the next start.
            if state.setup.is_some() {
                return window::close(id);
            }
            // A running game keeps running, as it does after the Python launcher closes.
            if persist::collect_unless_unreadable(state) {
                return window::close(id);
            }
        }
        Message::Noop => {}
    }
    Task::none()
}

/// Stores a setting: the ini keys go to the ini values as text, every other key to the app settings.
pub fn apply_set(state: &mut State, key: &str, value: Value) {
    let in_ini = INI_DEFAULTS.iter().any(|(name, _)| *name == key) || INI_FLAGS.contains(&key);
    if !in_ini {
        state.settings.set(key, value);
        // "Load mods" decides whether the sizes of the mods are offered.
        if key == "launcher_effects" {
            state.effects = advanced::effects_enabled(&state.settings);
        }
        if key == "mods_enabled" {
            performance::refresh_mod_resolutions(state);
        }
        // The setup checks a folder chosen in the dialog or typed into its field.
        if key == "game_dir" || key == "update_dir" {
            setup::refresh_folders(state);
        }
        return;
    }
    let text = match value {
        Value::Bool(on) => if on { "1" } else { "0" }.to_string(),
        Value::String(text) => text,
        other => other.to_string(),
    };
    state.ini.values.insert(key.to_string(), text);
    if key == "render_resolution" {
        performance::resolution_chosen(state);
    }
}

/// Opens the folder dialog for the setting `key`; the answer comes back as `Browsed`.
fn browse(state: &State, key: String) -> Task<Message> {
    let start = match state.settings.get_str(&key) {
        current if current.is_empty() => state.paths.port_dir.clone(),
        current => PathBuf::from(current),
    };
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(tr(&state.lang, game::browse_prompt(&key)))
        .set_directory(start)
        .pick_folder();
    Task::perform(dialog, move |handle| {
        Message::Browsed(key.clone(), handle.map(|h| h.path().to_path_buf()))
    })
}

/// Lists the monitors in the background; the splash does not wait for it.
pub fn start_monitor_listing(state: &State) -> Task<Message> {
    let paths = state.paths.clone();
    let backend = state.settings.get_str("video_backend");
    blocking(move || list_displays(&paths, &backend)).map(Message::MonitorsListed)
}

/// Advances the clock, the fire and the splash.
fn tick(state: &mut State, at: Instant) {
    let dt = at
        .saturating_duration_since(state.now)
        .as_secs_f32()
        .min(0.1);
    state.now = at;
    // No fire while the splash covers the window; it fills in during the flight to the corner.
    state.fire.set_intensity(intensity_for(
        !state.splash.done && !state.splash.flying(),
        state.setup.is_some(),
        state.focused,
        state.visible,
        state.effects,
    ));
    state.fire.tick(dt, scene_size(state));
    state.splash.advance(at);
}

/// The scene: the window minus the tab bar; the whole window under the setup, which has no bar.
pub fn scene_size(state: &State) -> Size {
    let bar = if state.setup.is_none() {
        BAR_HEIGHT
    } else {
        0.0
    };
    Size::new(
        state.window_size.width,
        (state.window_size.height - bar).max(0.0),
    )
}

/// Home closes the drawer; any other page opens it, sliding in when it was closed. The Trophies
/// page reloads its list each time it opens; the first open of Controls looks for a gamepad.
pub fn show_page(state: &mut State, page: Page) -> Task<Message> {
    let now = Instant::now();
    state.now = now;
    state.page = page;
    let open = page != Page::Home;
    // A drawer that opens comes in at its page's width; an open one slides to the new width.
    let fraction = drawer::drawer_fraction(page);
    match state.drawer {
        DrawerState::Open(_) if open => state.drawer_fraction.go_mut(fraction, now),
        DrawerState::Closed if open => state.drawer_fraction = drawer::fraction_animation(fraction),
        _ => {}
    }
    state.drawer = if open {
        DrawerState::Open(page)
    } else {
        DrawerState::Closed
    };
    state.drawer_progress.go_mut(open, now);
    match page {
        Page::Trophies => trophies::refresh(state),
        Page::Controls => controls::opened(state),
        Page::Mods => mods::refresh(state),
        _ => Task::none(),
    }
}

/// Whether the Home block (PLAY, chips, version and language buttons) responds: not behind a
/// wide drawer.
pub fn home_interactive_enabled(state: &State) -> bool {
    !matches!(state.drawer, DrawerState::Open(page) if page.wide())
}

/// Opacity of the Home block: 35 % behind a wide drawer, full otherwise.
pub fn home_interactive_opacity(state: &State) -> f32 {
    if home_interactive_enabled(state) {
        1.0
    } else {
        0.35
    }
}

/// The scene over the tab bar, bottom to top: the Home background, the dim layer that closes
/// the drawer when clicked (only while it is open), the Home block, and the drawer. The Home
/// block sits above the dim layer so it stays clickable next to a 46 % drawer. While the setup is
/// open the scene fills the whole window instead: the setup's background and, once the splash is
/// gone, its card, with no Home block, drawer or tab bar.
pub fn view(state: &State) -> Element<'_, Message> {
    let setup = state.setup.is_some();
    let base = if setup {
        setup::background(state)
    } else {
        home::background(state)
    };
    let mut scene = stack![base].width(Fill).height(Fill);
    if setup {
        if state.splash.done {
            scene = scene.push(setup::view(state));
        }
    } else {
        if let DrawerState::Open(_) = state.drawer {
            scene = scene.push(dim_close_catcher());
        }
        scene = scene.push(home::block(state));
    }
    if state.lang_menu.open {
        scene = scene.push(lang_menu::menu(state));
    }
    if state.version_menu.open && !setup {
        scene = scene.push(version_menu::menu(state));
    }
    if !state.splash.done {
        scene = scene.push(splash::view(state, &state.lang));
    }
    if !setup {
        if let DrawerState::Open(page) = state.drawer {
            scene = scene.push(drawer::view(state, page));
        }
    }
    // Always a stack with the page first, layers or not: a root that changed kind with them would
    // drop the state of the whole page (the drawer's scroll position) each time one comes or goes.
    let mut window = if setup {
        stack![column![scene]]
    } else {
        stack![column![scene, bar(state)]]
    };
    // While a key box of the Controls page waits, a click anywhere is its input.
    if state.controls.capturing.is_some() {
        window = window.push(controls::input_catcher());
    }
    if let Some(open) = &state.modal {
        window = window.push(widgets::modal(
            open.kind.clone(),
            &open.title,
            &open.text,
            &state.lang,
        ));
    }
    window.into()
}

/// Darkens the Home view behind an open drawer; a press on it closes the drawer. The spec also
/// blurs the background, but iced has no cheap backdrop blur, so only the darkening is kept.
fn dim_close_catcher<'a>() -> Element<'a, Message> {
    let dim = container(space())
        .width(Fill)
        .height(Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color {
                a: 0.45,
                ..palette::BG
            })),
            ..container::Style::default()
        });
    mouse_area(dim).on_press(Message::CloseDrawer).into()
}

/// The version and the system at the right of the bar (ruling R82): `v2.6.0 · Windows`, or
/// `· Linux` elsewhere (nothing tells a Steam Deck apart yet). `windows` is a parameter so tests
/// can cover both systems.
pub fn version_label(windows: bool) -> String {
    let system = if windows { "Windows" } else { "Linux" };
    format!("v{VERSION} · {system}")
}

/// The id of the red line along the bar's top edge.
pub const BAR_EDGE_ID: &str = "bar-edge";

/// The bottom bar: the red line on top (mockup `#bar` border-top), the nine tabs and the version.
fn bar(state: &State) -> Element<'_, Message> {
    let tabs = Page::ALL
        .into_iter()
        .fold(row![].spacing(2).height(Fill), |tabs, page| {
            tabs.push(tab(state, page))
        });
    let version = text(version_label(cfg!(windows)))
        .size(13)
        .style(theme::text_muted);
    // Right to left, the bar is mirrored: the tabs sit at the right, the version at the left.
    let line = if state.rtl {
        row![version, space::horizontal(), tabs]
    } else {
        row![tabs, space::horizontal(), version]
    };
    let line = line.align_y(Center).padding([0, 16]).height(Fill);
    container(column![widgets::hairline(BAR_EDGE_ID), line])
        .width(Fill)
        .height(BAR_HEIGHT)
        .style(theme::container_panel)
        .into()
}

/// One tab: the label button and, when active, the red underline. The tab is as wide as its label,
/// so the labels never overlap.
fn tab(state: &State, page: Page) -> Element<'_, Message> {
    let active = state.page == page;
    let label = text(tr(&state.lang, page.label_en()))
        .font(CINZEL_BOLD)
        .size(14)
        .height(Fill)
        .align_y(Center);
    let tab_button = button(label)
        .padding([0, 14])
        .height(Fill)
        .style(theme::tab(active))
        .on_press(Message::ShowPage(page));
    let underline = container(space())
        .width(Fill)
        .height(2)
        .style(move |_| container::Style {
            background: active.then_some(Background::Color(palette::RED_HI)),
            ..container::Style::default()
        });
    column![tab_button, underline].width(Shrink).into()
}

/// Keys and the window (opening, size, focus); animation frames while the drawer slides; the mouse
/// while a key box of the Controls page waits. The clock also carries the game's output, so it
/// keeps running while a game is live, even with the window hidden.
pub fn subscription(state: &State) -> Subscription<Message> {
    let keys = keyboard::listen().filter_map(|event| match event {
        keyboard::Event::KeyPressed {
            key,
            modifiers,
            location,
            ..
        } => Some(Message::Key(key, modifiers, location)),
        _ => None,
    });
    let window_events = window::events().filter_map(window_message);
    // The close button is ours: the window closes once the settings are saved.
    let close = window::close_requests().map(Message::CloseRequested);
    let frames = if drawer_animating(state) {
        window::frames().map(Message::Tick)
    } else {
        Subscription::none()
    };
    // The splash needs the clock even when the fire is off.
    // The hover timers of the language menu need it too, and a slow clock keeps `now` fresh for
    // their stamps while the fire is off.
    let clock = if (state.effects && state.visible)
        || !state.splash.done
        || lang_menu::timing(state)
        || version_menu::timing(state)
    {
        iced::time::every(Duration::from_millis(33)).map(Message::Tick)
    } else if state.visible || play::game_live(state) {
        iced::time::every(IDLE_CLOCK).map(Message::Tick)
    } else {
        Subscription::none()
    };
    Subscription::batch([
        keys,
        window_events,
        close,
        frames,
        clock,
        controls::subscription(state),
    ])
}

/// Whether the drawer moves: it slides in or out, or its width changes.
pub fn drawer_animating(state: &State) -> bool {
    state.drawer_progress.is_animating(state.now) || state.drawer_fraction.is_animating(state.now)
}

/// What a window event means here: the window opened, changed size or desktop scale, or gained
/// or lost the focus.
pub fn window_message((id, event): (window::Id, window::Event)) -> Option<Message> {
    match event {
        window::Event::Opened { size, .. } => Some(Message::WindowOpened(id, size)),
        window::Event::Resized(size) => Some(Message::Resized(size)),
        window::Event::Rescaled(factor) => Some(Message::DesktopRescaled(factor)),
        window::Event::Focused => Some(Message::WindowFocused(true)),
        window::Event::Unfocused => Some(Message::WindowFocused(false)),
        _ => None,
    }
}

/// The monitor's size when it is a real one: Wayland reports 0 × 0 for an output without a current
/// mode (the Python checks `if width:`).
fn known_monitor(monitor: Option<Size>) -> Option<Size> {
    monitor.filter(|size| {
        size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
    })
}

/// Python `size_window`: `WINDOW_SIZE`, at most 90 % of the monitor (in logical pixels) when its
/// size is known.
pub fn fitted_window(monitor: Option<Size>) -> Size {
    known_monitor(monitor).map_or(WINDOW_SIZE, |monitor| {
        Size::new(
            WINDOW_SIZE.width.min((monitor.width * 0.9).floor()),
            WINDOW_SIZE.height.min((monitor.height * 0.9).floor()),
        )
    })
}

/// Asks for the monitor's size, to which `MonitorSize` fits the window; nothing before the window
/// opened.
pub fn fit_window(state: &State) -> Task<Message> {
    match state.window {
        Some(id) => window::monitor_size(id).map(Message::MonitorSize),
        None => Task::none(),
    }
}

/// Where the window of `size` sits centred on `monitor` (both in logical pixels at `zoom`), in the
/// desktop's units that moving a window takes. The monitor is taken to start at the screen's
/// origin: the window opens centred on the primary monitor, whose corner that is.
pub fn centred_position(monitor: Size, size: Size, zoom: f32) -> Point {
    Point::new(
        ((monitor.width - size.width) / 2.0).max(0.0) * zoom,
        ((monitor.height - size.height) / 2.0).max(0.0) * zoom,
    )
}

/// Sizes the window for the current zoom, its minimum included, and centres it: the zoom changes
/// what a logical pixel measures, and the window was opened before the desktop's scale was known.
fn size_window(state: &mut State, monitor: Option<Size>) -> Task<Message> {
    let monitor = known_monitor(monitor);
    let size = fitted_window(monitor);
    state.window_size = size;
    let Some(id) = state.window else {
        return Task::none();
    };
    // The new minimum first: the old one would stop the window from shrinking to a lower zoom.
    let sized = window::set_min_size(id, Some(size)).chain(window::resize(id, size));
    match monitor {
        Some(monitor) => {
            let zoom = window_zoom(state.scale, state.desktop_scale);
            sized.chain(window::move_to(id, centred_position(monitor, size, zoom)))
        }
        None => sized,
    }
}

/// The window as it opens: icon, starting and minimum size and place (`size_window` adjusts them
/// once the desktop's scale and the monitor are known) and, on Linux, the application id the menu
/// entry matches (`StartupWMClass`, the Python's Tk class). The close button only asks
/// (`CloseRequested`), so the settings are saved before the window goes.
fn window_settings() -> window::Settings {
    window::Settings {
        size: WINDOW_SIZE,
        min_size: Some(WINDOW_SIZE),
        position: window::Position::Centered,
        exit_on_close_request: false,
        icon: window::icon::from_file_data(home::MONOGRAM_PNG, None).ok(),
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: "spartanport".to_string(),
            ..Default::default()
        },
        ..window::Settings::default()
    }
}

/// Opens the launcher window and runs until it closes.
pub fn run(paths: Paths) -> iced::Result {
    // Asked once, before the window: the shader fire needs iced to render with wgpu.
    let shader_fire = probe_gpu();
    let boot = move || {
        let mut state = State::open(paths.clone());
        state.fire = Fire::new(shader_fire);
        let checks = splash::start_checks(&state);
        let monitors = start_monitor_listing(&state);
        (state, Task::batch([checks, monitors]))
    };
    let app = iced::application(boot, update, view)
        .title("SpartanPort (PC)")
        .theme(|_: &State| theme::theme())
        .subscription(subscription)
        .scale_factor(|state: &State| window_zoom(state.scale, state.desktop_scale))
        .window(window_settings());
    theme::font_bytes()
        .into_iter()
        .fold(app, |app, font| app.font(font))
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::image::Handle;

    #[test]
    fn the_window_icon_is_the_monogram_artwork() {
        let (pixels, size) = window_settings().icon.unwrap().into_raw();
        let Handle::Bytes(_, bytes) = home::monogram() else {
            panic!("the monogram is not embedded");
        };
        let monogram = ::image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!((size.width, size.height), monogram.dimensions());
        assert!(pixels == monogram.into_raw(), "another picture");
    }
}
