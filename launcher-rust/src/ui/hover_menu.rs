//! Open/hover/leave timers shared by the Home's hover-opened menus (spec 4.5).

use std::time::Duration;

use iced::time::Instant;

/// Hovering the button this long opens the list (spec 4.5).
pub const HOVER_OPEN: Duration = Duration::from_millis(450);
/// Leaving the button and the list this long closes it (spec 4.5).
pub const LEAVE_CLOSE: Duration = Duration::from_millis(300);

/// Open state and hover timers.
#[derive(Debug, Clone, Default)]
pub struct HoverMenu {
    pub open: bool,
    /// When the pointer entered the button or the list; opens the list after `HOVER_OPEN`.
    pub hover_since: Option<Instant>,
    /// When the pointer left both; closes the list after `LEAVE_CLOSE`.
    pub leave_since: Option<Instant>,
}

/// Pointer and clock events of a hover menu.
#[derive(Debug, Clone, Copy)]
pub enum HoverMessage {
    Enter,
    Leave,
    Toggle,
    Tick(Instant),
}

/// Applies an event; `Enter` and `Leave` are stamped with `now`, `Tick` opens the menu after
/// `HOVER_OPEN` and closes it after `LEAVE_CLOSE`.
pub fn apply(menu: &mut HoverMenu, message: HoverMessage, now: Instant) {
    match message {
        HoverMessage::Enter => {
            menu.hover_since = Some(now);
            menu.leave_since = None;
        }
        HoverMessage::Leave => {
            menu.leave_since = Some(now);
            menu.hover_since = None;
        }
        HoverMessage::Toggle => {
            menu.open = !menu.open;
            menu.hover_since = None;
            menu.leave_since = None;
        }
        HoverMessage::Tick(now) => {
            if let Some(since) = menu.hover_since {
                if now.saturating_duration_since(since) >= HOVER_OPEN {
                    menu.open = true;
                    menu.hover_since = None;
                }
            }
            if let Some(since) = menu.leave_since {
                if now.saturating_duration_since(since) >= LEAVE_CLOSE {
                    menu.open = false;
                    menu.leave_since = None;
                }
            }
        }
    }
}

/// Whether a timer is running, so the app keeps the clock ticking.
pub fn timing(menu: &HoverMenu) -> bool {
    menu.hover_since.is_some() || menu.leave_since.is_some()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use iced::time::Instant;

    use super::*;

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
    }

    #[test]
    fn hover_opens_at_450ms_and_leave_closes_at_300ms() {
        let t0 = Instant::now();
        let mut menu = HoverMenu::default();
        apply(&mut menu, HoverMessage::Enter, t0);
        apply(&mut menu, HoverMessage::Tick(ms(t0, 449)), ms(t0, 449));
        assert!(!menu.open);
        apply(&mut menu, HoverMessage::Tick(ms(t0, 450)), ms(t0, 450));
        assert!(menu.open);
        let t1 = ms(t0, 1000);
        apply(&mut menu, HoverMessage::Leave, t1);
        apply(&mut menu, HoverMessage::Tick(ms(t1, 299)), ms(t1, 299));
        assert!(menu.open);
        apply(&mut menu, HoverMessage::Tick(ms(t1, 300)), ms(t1, 300));
        assert!(!menu.open);
    }

    #[test]
    fn coming_back_before_300ms_keeps_it_open() {
        let t0 = Instant::now();
        let mut menu = HoverMenu::default();
        apply(&mut menu, HoverMessage::Toggle, t0);
        apply(&mut menu, HoverMessage::Leave, t0);
        apply(&mut menu, HoverMessage::Enter, ms(t0, 100));
        apply(&mut menu, HoverMessage::Tick(ms(t0, 400)), ms(t0, 400));
        assert!(menu.open);
    }

    #[test]
    fn toggle_flips_and_clears_the_timers() {
        let t0 = Instant::now();
        let mut menu = HoverMenu::default();
        apply(&mut menu, HoverMessage::Enter, t0);
        apply(&mut menu, HoverMessage::Toggle, t0);
        assert!(menu.open);
        assert!(menu.hover_since.is_none() && menu.leave_since.is_none());
        apply(&mut menu, HoverMessage::Toggle, t0);
        assert!(!menu.open);
    }

    #[test]
    fn timing_is_true_only_while_a_timer_runs() {
        let t0 = Instant::now();
        let mut menu = HoverMenu::default();
        assert!(!timing(&menu));
        apply(&mut menu, HoverMessage::Enter, t0);
        assert!(timing(&menu));
        apply(&mut menu, HoverMessage::Leave, t0);
        assert!(timing(&menu));
        apply(&mut menu, HoverMessage::Toggle, t0);
        assert!(!timing(&menu));
    }
}
