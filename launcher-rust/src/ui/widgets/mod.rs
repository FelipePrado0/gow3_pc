//! Settings widgets shared by the pages.
pub mod button_icon;
pub mod checkbox;
pub mod chip;
pub mod folder;
pub mod hairline;
pub mod min_width;
pub mod modal;
pub mod row;
pub mod section;
pub mod select;
pub mod slider;

pub use button_icon::{button_icon, spec, IconKind, IconSpec};
pub use checkbox::{checkbox, checkbox_beside, checkbox_ini, toggle_row};
pub use chip::{chip, ChipKind};
pub use folder::folder;
pub use hairline::{hairline, hairline_style, HAIRLINE};
pub use min_width::{leave_room, min_width};
pub use modal::{modal, Modal, ModalKind, ModalMessage};
pub use row::row;
pub use section::section;
pub use select::{
    choice_pairs, select, select_message, select_pairs, select_pairs_small, SELECT_MIN,
    SELECT_MIN_SMALL, SELECT_TEXT,
};
pub use slider::slider;
