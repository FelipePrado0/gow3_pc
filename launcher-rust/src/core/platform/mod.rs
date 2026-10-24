//! The few calls that need the operating system.

#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub use unix::{
    attach_parent_console, catch_interrupt, close_std_streams, create_junction, interrupted,
    no_console_window, no_window, prepare, show_error, steam_path_from_registry,
    user_default_ui_language, wait_for_process, Job,
};
#[cfg(windows)]
pub use windows::{
    attach_parent_console, catch_interrupt, close_std_streams, create_junction, has_console,
    interrupted, no_console_window, no_window, pipe_bytes_available, prepare, set_pipe_nowait,
    show_error, steam_path_from_registry, user_default_ui_language, wait_for_process, Job,
};
