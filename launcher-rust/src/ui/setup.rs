//! The first-run setup (setup spec): three steps between the splash and the Home, shown while
//! `State::setup` is `Some`.

use iced::keyboard::key::Named;
use iced::keyboard::Key;
use iced::widget::{button, column, container, pick_list, row, space, stack, text};
use iced::{Background, Center, Element, Fill, Left, Task};
use serde_json::json;

use crate::core::game::{
    missing_game_files, same_folder, FileState, REQUIRED_GAME_FILES, REQUIRED_UPDATE_FILES,
};
use crate::core::i18n::{launcher_language, system_language, tr, tr_fmt};
use crate::core::options::DISPLAY_MODES;
use crate::core::settings::Settings;
use crate::ui::app::{Message, State};
use crate::ui::home;
use crate::ui::pages::game::{self, folder_row};
use crate::ui::pages::performance::{self, aspect_options, select_row};
use crate::ui::persist::{self, Persisted};
use crate::ui::play::describe_missing;
use crate::ui::splash::SPLASH_BG;
use crate::ui::theme::{self, mirror_x, palette, CINZEL_BOLD};
use crate::ui::widgets::{min_width, row as labeled_row, select, SELECT_MIN, SELECT_TEXT};

/// One step of the setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Folders,
    Gpu,
    Video,
}

impl Step {
    /// Every step, in the order they are shown.
    pub const ALL: [Step; 3] = [Step::Folders, Step::Gpu, Step::Video];

    /// The position of the step, counting from 1.
    pub fn number(self) -> usize {
        match self {
            Step::Folders => 1,
            Step::Gpu => 2,
            Step::Video => 3,
        }
    }

    /// The step after this one, or `None` on the last.
    pub fn next(self) -> Option<Step> {
        match self {
            Step::Folders => Some(Step::Gpu),
            Step::Gpu => Some(Step::Video),
            Step::Video => None,
        }
    }

    /// The step before this one, or `None` on the first.
    pub fn previous(self) -> Option<Step> {
        match self {
            Step::Folders => None,
            Step::Gpu => Some(Step::Folders),
            Step::Video => Some(Step::Gpu),
        }
    }

    /// The English title of the step, the key of its translation.
    pub fn title_en(self) -> &'static str {
        match self {
            Step::Folders => "Folders",
            Step::Gpu => "Graphics card",
            Step::Video => "Video",
        }
    }

    /// The English sentence that introduces the step, the key of its translation.
    pub fn intro_en(self) -> &'static str {
        match self {
            Step::Folders => "Choose the folder of your game dump. The update folder is optional.",
            Step::Gpu => "The game runs on this graphics card.",
            Step::Video => {
                "Resolution and frame rate. The in-game menu (Insert or R3+L2) changes the same values."
            }
        }
    }
}

/// The setup while it is open.
#[derive(Debug, Clone)]
pub struct SetupState {
    /// The step on screen.
    pub step: Step,
    /// What the launcher would save as it stands when the setup opened.
    pub defaults: Persisted,
    /// The required files the game folder lacks. The two folders are read from the disk when the
    /// setup opens, when either of them changes and when the window gets the focus back, never
    /// on a redraw.
    game_missing: Vec<(String, FileState)>,
    /// The required files the update folder lacks; none while it is unset or is the game folder.
    update_missing: Vec<(String, FileState)>,
    /// Whether the update folder is the game folder.
    same_folders: bool,
}

impl SetupState {
    /// A setup on its first step, with the current values of `state` as the defaults and its
    /// folders checked.
    pub fn new(state: &State) -> SetupState {
        let mut setup = SetupState {
            step: Step::Folders,
            defaults: persist::gather(state),
            game_missing: Vec::new(),
            update_missing: Vec::new(),
            same_folders: false,
        };
        setup.check_folders(&state.settings);
        setup
    }

    /// Reads the game and update folders of `settings` from the disk.
    fn check_folders(&mut self, settings: &Settings) {
        let game_dir = settings.get_str("game_dir");
        let update_dir = settings.get_str("update_dir");
        self.game_missing = missing_game_files(&game_dir, REQUIRED_GAME_FILES);
        self.same_folders =
            !update_dir.is_empty() && !game_dir.is_empty() && same_folder(&update_dir, &game_dir);
        self.update_missing = if update_dir.is_empty() || self.same_folders {
            Vec::new()
        } else {
            missing_game_files(&update_dir, REQUIRED_UPDATE_FILES)
        };
    }

    /// Whether both folders passed their last check.
    fn folders_pass(&self) -> bool {
        self.game_missing.is_empty() && self.update_missing.is_empty() && !self.same_folders
    }
}

/// Checks the game and update folders again while the setup is open; does nothing otherwise.
pub fn refresh_folders(state: &mut State) {
    if let Some(setup) = state.setup.as_mut() {
        setup.check_folders(&state.settings);
    }
}

/// What the setup screen can ask for.
#[derive(Debug, Clone)]
pub enum SetupMessage {
    Next,
    Back,
    Skip,
    Finish,
}

/// The red lines under the folder `key` (`game_dir` or `update_dir`) from the last check of the
/// folders; empty when the folder passes or the setup is closed.
pub fn folder_problems(state: &State, key: &str) -> Vec<String> {
    let lang = state.lang.as_str();
    let Some(setup) = state.setup.as_ref() else {
        return Vec::new();
    };
    let missing = match key {
        "game_dir" => &setup.game_missing,
        "update_dir" if setup.same_folders => {
            return vec![tr(
                lang,
                "Choose two separate folders for the game and its update, or leave the update folder empty.",
            )];
        }
        "update_dir" => &setup.update_missing,
        _ => return Vec::new(),
    };
    if missing.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![tr(lang, "Missing or empty files:")];
    lines.extend(describe_missing(lang, missing));
    lines
}

/// Whether Next may leave the current step: both folders pass on Folders, always on the others.
pub fn can_advance(state: &State) -> bool {
    match state.setup.as_ref() {
        None => false,
        Some(setup) if setup.step == Step::Folders => setup.folders_pass(),
        Some(_) => true,
    }
}

/// Applies one setup message to `state`.
pub fn update(state: &mut State, message: SetupMessage) -> Task<Message> {
    if matches!(message, SetupMessage::Next) && !can_advance(state) {
        return Task::none();
    }
    let Some(setup) = state.setup.as_mut() else {
        return Task::none();
    };
    match message {
        SetupMessage::Finish => return finish(state),
        SetupMessage::Skip => {
            let defaults = setup.defaults.clone();
            state.settings = Settings(defaults.app);
            state.ini.values = defaults.ini;
            // The Aspect ratio box follows the restored resolution.
            performance::resolution_chosen(state);
            state.settings.set("ui_language", json!(state.lang));
            // A failed save leaves the setup open over the restored folders.
            refresh_folders(state);
            return Task::batch([game::game_changed(state), finish(state)]);
        }
        SetupMessage::Next => {
            if let Some(step) = setup.step.next() {
                setup.step = step;
            }
        }
        SetupMessage::Back => {
            if let Some(step) = setup.step.previous() {
                setup.step = step;
            }
        }
    }
    Task::none()
}

/// Enter is Next (when allowed) or Finish on the last step; every other key does nothing.
pub fn key_pressed(state: &mut State, key: &Key) -> Task<Message> {
    if !matches!(key, Key::Named(Named::Enter)) {
        return Task::none();
    }
    match state.setup.as_ref().map(|setup| setup.step) {
        Some(Step::Video) => finish(state),
        Some(_) if can_advance(state) => update(state, SetupMessage::Next),
        _ => Task::none(),
    }
}

/// A language was picked in the menu: the window is rebuilt in it, without writing anything.
pub fn language_picked(state: &mut State) -> Task<Message> {
    let language = launcher_language(&state.settings.get_str("ui_language"), system_language);
    let scale = state.scale;
    // The baseline stays what the files held when the setup opened, so Finish writes every edit.
    let saved = state.saved.clone();
    let task = persist::rebuild(state, language, scale);
    state.saved = saved;
    task
}

/// Writes the files as Save does; on success the setup closes and the Home shows.
pub fn finish(state: &mut State) -> Task<Message> {
    // A failed save leaves the setup open under the error dialog.
    if persist::collect_or_report(state) {
        state.setup = None;
    }
    Task::none()
}

/// Width of the card.
const CARD_WIDTH: f32 = 640.0;

/// The splash's dark background with the fire over it, which fills in while the monogram flies to
/// the corner.
pub fn background(state: &State) -> Element<'_, Message> {
    stack![
        container(space())
            .width(Fill)
            .height(Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(SPLASH_BG)),
                ..container::Style::default()
            }),
        state.fire.view(),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

/// Corner monogram, language button and the centred card of the current step.
pub fn view(state: &State) -> Element<'_, Message> {
    let centred = container(card(state))
        .width(Fill)
        .height(Fill)
        .align_x(Center)
        .align_y(Center);
    stack![
        home::corner_monogram(state),
        home::top_right(state, false),
        centred
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

/// The two folder rows, each followed by its red problem lines.
fn folders_body(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let rows = [
        (
            "game_dir",
            tr(lang, "Game folder"),
            tr(
                lang,
                "Your own dump of the game (eboot.bin, sce_module, sce_sys): SpartanPort, any region and version.",
            ),
        ),
        (
            "update_dir",
            tr(lang, "Update folder (optional)"),
            tr(
                lang,
                "Only if the update is dumped apart from the game: it is laid over the game folder without changing either. Leave empty when the game folder already has the update.",
            ),
        ),
    ];
    let mut body = column![]
        .spacing(8)
        .width(Fill)
        .align_x(mirror_x(Left, state.rtl));
    for (key, label, hint) in rows {
        body = body.push(folder_row(state, key, &label, &hint));
        for line in folder_problems(state, key) {
            body = body.push(text(line).size(12).color(palette::RED_HI));
        }
    }
    body.into()
}

/// The graphics card selector, with a note while the cards have not been listed.
fn gpu_body(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let hint = tr(
        lang,
        "The GPU the game runs on. Choose your dedicated card if the game uses the integrated one.",
    );
    let mut body = column![labeled_row(
        state.rtl,
        &tr(lang, "Graphics card"),
        select_row(state, lang, "gpu_id"),
        Some(&hint),
    )]
    .spacing(8)
    .width(Fill)
    .align_x(mirror_x(Left, state.rtl));
    if state.gpus.is_empty() {
        body = body.push(
            text(tr(lang, "Graphics card not checked: the game picks one."))
                .size(13)
                .style(theme::text_muted),
        );
    }
    body.into()
}

/// The display mode, the aspect ratio and resolution, and the two frame rate limits.
fn video_body(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let hint = |english: &str| tr(lang, english);
    let display_mode = state
        .ini
        .values
        .get("display_mode")
        .cloned()
        .unwrap_or_default();
    let aspect = min_width(
        SELECT_MIN,
        pick_list(
            aspect_options(state),
            Some(state.aspect.clone()),
            Message::Aspect,
        )
        .text_size(SELECT_TEXT)
        .style(theme::pick_list_dark),
    );
    let resolution = format!(
        "{}  ·  {}",
        tr(lang, "Render resolution"),
        tr(lang, "restart")
    );
    column![
        labeled_row(
            state.rtl,
            &tr(lang, "Display mode"),
            select("display_mode", DISPLAY_MODES, &display_mode, lang),
            Some(&hint(
                "Borderless covers the screen without changing the display mode."
            )),
        ),
        labeled_row(
            state.rtl,
            &tr(lang, "Aspect ratio"),
            aspect.into(),
            Some(&hint("Shows the render resolutions of this shape only.")),
        ),
        labeled_row(
            state.rtl,
            &resolution,
            select_row(state, lang, "render_resolution"),
            Some(&hint(
                "The size the game draws at; higher is sharper and slower. A resolution patch replaces the texture fix and reserves the memory it needs."
            )),
        ),
        labeled_row(
            state.rtl,
            &tr(lang, "Frame rate limit"),
            select_row(state, lang, "fps_limit"),
            Some(&hint("The engine frame rate (below) is the highest it can go.")),
        ),
        labeled_row(
            state.rtl,
            &tr(lang, "Engine frame rate"),
            select_row(state, lang, "engine_fps"),
            Some(&hint(
                "The highest frame rate the game itself runs at (a game patch)."
            )),
        ),
    ]
    .spacing(8)
    .width(Fill)
    .align_x(mirror_x(Left, state.rtl))
    .into()
}

/// The card of the step on screen: title, step line, intro, body and footer.
fn card(state: &State) -> Element<'_, Message> {
    let lang = state.lang.as_str();
    let step = state
        .setup
        .as_ref()
        .map_or(Step::Folders, |setup| setup.step);
    let number = step.number().to_string();
    let total = Step::ALL.len().to_string();
    let step_line = format!(
        "{} · {}",
        tr_fmt(lang, "Step {} of {}", &[&number, &total]),
        tr(lang, step.title_en())
    );
    let body: Element<'_, Message> = match step {
        Step::Folders => folders_body(state),
        Step::Gpu => gpu_body(state),
        Step::Video => video_body(state),
    };
    // Left to right: Skip at the start, then Back and Next or Finish; right to left the row is
    // reversed, so Skip is at the right.
    let mut buttons: Vec<Element<'_, Message>> = vec![
        button(text(tr(lang, "Skip")).style(theme::text_muted))
            .style(theme::button_flat)
            .on_press(Message::Setup(SetupMessage::Skip))
            .into(),
        space().width(Fill).into(),
    ];
    if step.previous().is_some() {
        buttons.push(
            button(text(tr(lang, "Back")))
                .style(theme::button_flat)
                .on_press(Message::Setup(SetupMessage::Back))
                .into(),
        );
    }
    buttons.push(if step.next().is_some() {
        button(text(tr(lang, "Next")))
            .style(theme::button_flat)
            .on_press_maybe(can_advance(state).then_some(Message::Setup(SetupMessage::Next)))
            .into()
    } else {
        button(text(tr(lang, "Finish")))
            .style(theme::button_play)
            .on_press(Message::Setup(SetupMessage::Finish))
            .into()
    });
    if state.rtl {
        buttons.reverse();
    }
    let footer = row(buttons).spacing(8);
    container(
        column![
            text(tr(lang, "Initial setup")).font(CINZEL_BOLD).size(22),
            text(step_line).style(theme::text_muted),
            text(tr(lang, step.intro_en())).style(theme::text_muted),
            body,
            footer,
            text(tr(
                lang,
                "You can change all of this later in the Graphics, Performance and Game tabs."
            ))
            .style(theme::text_muted),
        ]
        .spacing(14)
        .width(Fill)
        .align_x(mirror_x(Left, state.rtl)),
    )
    .padding(24)
    .width(CARD_WIDTH)
    .style(theme::container_card)
    .into()
}
