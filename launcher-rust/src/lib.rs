// `app_defaults` builds its 43 keys with `json!`, whose expansion exceeds the default limit.
#![recursion_limit = "256"]

pub mod core;
pub mod runner;
pub mod startup;
pub mod ui;

pub const APP_NAME: &str = "SpartanPort";
pub const VERSION: &str = "2.8.0";
