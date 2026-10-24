//! Option tables and render resolutions.

use std::collections::BTreeSet;

/// One entry of a choice list: the stored value and its labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub value: &'static str,
    pub en: &'static str,
    pub ru: Option<&'static str>,
}

const fn c(value: &'static str, en: &'static str) -> Choice {
    Choice {
        value,
        en,
        ru: None,
    }
}

const fn cr(value: &'static str, en: &'static str, ru: &'static str) -> Choice {
    Choice {
        value,
        en,
        ru: Some(ru),
    }
}

/// A choice label without its trailing parenthetical, for the Home settings line
/// (`"Quality (×1.5)"` → `"Quality"`); Chinese and Japanese write the parenthesis full width. A
/// label that does not end with one, or has nothing before it, stays whole.
pub fn short_label(label: &str) -> &str {
    let open = if label.ends_with(')') {
        label.rfind('(')
    } else if label.ends_with('）') {
        label.rfind('（')
    } else {
        None
    };
    match open.map(|at| label[..at].trim_end()) {
        Some(head) if !head.is_empty() => head,
        _ => label,
    }
}

pub static DISPLAY_MODES: &[Choice] = &[
    c("windowed", "Windowed"),
    c("borderless", "Borderless"),
    c("fullscreen", "Fullscreen"),
];

pub static ANISOTROPY: &[Choice] = &[
    c("0", "Game default"),
    c("2", "2x"),
    c("4", "4x"),
    c("8", "8x"),
    c("16", "16x"),
];

pub static FPS_LIMITS: &[Choice] = &[
    c("30", "30"),
    c("60", "60"),
    c("100", "100"),
    c("120", "120"),
    c("144", "144"),
    c("180", "180"),
    c("240", "240"),
    c("0", "Unlimited"),
];

/// GUI Scale (R90): absolute zooms, 1x by default.
pub static UI_SCALES: &[Choice] = &[c("1", "1x"), c("1.5", "1.5x")];

/// The GUI Scale choice a stored value stands for: an older file's "2" is 1.5x, and its "auto"
/// (which followed the desktop) or anything unknown is 1x.
pub fn ui_scale_choice(stored: &str) -> &'static str {
    match stored {
        "1.5" | "2" => "1.5",
        _ => "1",
    }
}

pub static STICK_PROMPTS: &[Choice] = &[
    c("keyboard", "Keyboard (native)"),
    c("mouse", "Adaptive mouse (experimental)"),
];

pub static AIM_STYLES: &[Choice] = &[c("0", "Relative"), c("1", "Absolute")];

pub static GLYPH_STYLES: &[Choice] = &[
    c("off", "Original (default)"),
    c("auto", "Match my controller"),
    c("xbox", "Xbox"),
    c("nintendo", "Nintendo"),
    c("keyboard", "Keyboard"),
];

pub static UPSCALERS: &[Choice] = &[
    cr("fsr3", "FSR 3.1 (every GPU)", "FSR 3.1 (любая видеокарта)"),
    cr(
        "taa",
        "TAA (native resolution anti-aliasing)",
        "TAA (нативное сглаживание)",
    ),
    cr("off", "Off", "Выключен"),
];

pub static PRESETS: &[Choice] = &[
    c("0", "Native AA (×1.0)"),
    c("1", "Quality (×1.5)"),
    c("2", "Balanced (×1.7)"),
    c("3", "Performance (×2)"),
    c("4", "Ultra Performance (×3)"),
];

pub static OUTPUTS: &[Choice] = &[
    c("1280x720", "1280 × 720"),
    c("1920x1080", "1920 × 1080"),
    c("2560x1440", "2560 × 1440"),
    c("3840x2160", "3840 × 2160 (4K)"),
];

/// The Python `LIVE` table (live resolution change).
pub static LIVE_RESOLUTION: &[Choice] = &[
    cr("auto", "Auto (by graphics card)", "Авто (по видеокарте)"),
    cr("0", "Off (faster)", "Выключена (быстрее)"),
    cr(
        "1",
        "On (change without restarting)",
        "Включена (без перезапуска)",
    ),
];

pub static LANGUAGES: &[Choice] = &[
    cr("1", "English", "Английский"),
    cr("8", "Russian", "Русский"),
    cr("0", "Japanese", "Японский"),
    cr("2", "French", "Французский"),
    cr("3", "Spanish", "Испанский"),
    cr("4", "German", "Немецкий"),
    cr("5", "Italian", "Итальянский"),
    cr("16", "Polish", "Польский"),
    cr("17", "Portuguese (Brazil)", "Португальский (Бразилия)"),
];

pub static DRAW_PIPE: &[Choice] = &[
    cr("", "Auto (8+ threads)", "Авто (8+ потоков)"),
    cr("1", "On", "Включён"),
    cr("0", "Off (more stable)", "Выключен (стабильнее)"),
];

pub static READBACKS: &[Choice] = &[
    cr("", "Relaxed (default)", "Relaxed (по умолчанию)"),
    cr("0", "Off", "Выключены"),
    c("2", "Precise"),
];

pub static NO_MSAA: &[Choice] = &[
    c("", "Automatic"),
    c("1", "Off (fixes black lines)"),
    c("0", "On"),
];

pub static STREAM_MEMORY: &[Choice] = &[
    c("cached", "System RAM (cached, default)"),
    c("host", "System RAM (uncached)"),
    c("device", "GPU memory (previous behavior)"),
];

pub static FRAMES_QUEUED: &[Choice] = &[
    c("1", "1 (lowest latency)"),
    c("2", "2"),
    c("0", "Unlimited"),
];

/// The launcher's own interface languages (`LANGUAGE_NAMES` of the Python `spartanport_lang`).
pub static UI_LANGUAGES: &[Choice] = &[
    cr("", "System", "Как в системе"),
    c("en", "English"),
    c("ru", "Русский"),
    c("ar", "العربية"),
    c("es", "Español"),
    c("pt", "Português (Brasil)"),
    c("fr", "Français"),
    c("de", "Deutsch"),
    c("it", "Italiano"),
    c("pl", "Polski"),
    c("tr", "Türkçe"),
    c("zh", "简体中文"),
    c("ja", "日本語"),
    c("ko", "한국어"),
    c("id", "Bahasa Indonesia"),
];

pub static VIDEO_BACKENDS: &[Choice] = &[
    c("auto", "Automatic"),
    c("wayland", "Wayland"),
    c("x11", "X11 (XWayland)"),
];

pub static ENGINE_FPS_CHOICES: &[Choice] = &[
    c("60", "60 (original)"),
    c("100", "100"),
    c("120", "120"),
    c("144", "144"),
    c("180", "180"),
    c("240", "240"),
];

pub static RESOLUTIONS: &[&str] = &[
    "native",
    "480p",
    "720p",
    "900p",
    "1440p",
    "1800p",
    "4K",
    "2560x1080",
    "3440x1440",
    "3840x1600",
    "5120x1440",
    "1920x1200",
    "2560x1600",
    "1280x800",
    "1440x900",
    "1360x768",
    "1280x960",
    "1440x1080",
    "1920x1440",
    "2048x1536",
    "2560x1920",
    "5120x2160",
    "8K",
];

/// Custom sizes confirmed in game; the other ones after the first seven are beta.
pub static TESTED_RESOLUTIONS: &[&str] = &[
    "3440x1440",
    "3840x1600",
    "5120x1440",
    "5120x2160",
    "1920x1200",
    "2560x1600",
    "1280x800",
    "1440x900",
    "1360x768",
    "1280x960",
    "1920x1440",
];

/// Sizes of the named resolutions (the custom ones are `<width>x<height>`).
pub static SIZES: &[(&str, (u32, u32))] = &[
    ("native", (1920, 1080)),
    ("480p", (854, 480)),
    ("720p", (1280, 720)),
    ("900p", (1600, 900)),
    ("1440p", (2560, 1440)),
    ("1800p", (3200, 1800)),
    ("4K", (3840, 2160)),
    ("8K", (7680, 4320)),
];

const ASPECTS: &[(&str, f64)] = &[
    ("16:9", 16.0 / 9.0),
    ("16:10", 16.0 / 10.0),
    ("21:9", 2.37),
    ("32:9", 32.0 / 9.0),
    ("4:3", 4.0 / 3.0),
];

/// `RESOLUTIONS[7..]` without the tested ones.
pub fn beta_resolutions() -> Vec<&'static str> {
    RESOLUTIONS[7..]
        .iter()
        .copied()
        .filter(|r| !TESTED_RESOLUTIONS.contains(r))
        .collect()
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// The marketing name of the aspect ratio ("21:9" covers 2560x1080 to 3840x1600), else the reduced ratio.
pub fn aspect_name(w: u32, h: u32) -> String {
    if w == 0 || h == 0 {
        return format!("{w}:{h}");
    }
    let ratio = f64::from(w) / f64::from(h);
    for (name, value) in ASPECTS {
        if (ratio - value).abs() < 0.05 {
            return (*name).to_string();
        }
    }
    let d = gcd(w, h);
    format!("{}:{}", w / d, h / d)
}

/// Width and height of a named size or a `<width>x<height>` name, zero allowed.
fn parse_size(name: &str) -> Option<(u32, u32)> {
    if let Some((_, size)) = SIZES.iter().find(|(n, _)| *n == name) {
        return Some(*size);
    }
    let mut parts = name.split('x');
    let (w, h) = (parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// (width, height) of a built-in name or a mod's `<width>x<height>`; None for anything else.
pub fn resolution_size(name: &str) -> Option<(u32, u32)> {
    parse_size(name).filter(|(w, h)| *w > 0 && *h > 0)
}

/// What the lists show for a render resolution; anything unparseable is shown as it is.
pub fn resolution_label(name: &str, aspect: bool) -> String {
    let Some((w, h)) = parse_size(name) else {
        return name.to_string();
    };
    let mut parts: Vec<String> = Vec::new();
    if name == "native" {
        parts.push("Native".to_string());
    } else if SIZES.iter().any(|(n, _)| *n == name) {
        parts.push(name.to_string());
    }
    parts.push(format!("{w} × {h}"));
    if aspect && w > 0 && h > 0 {
        parts.push(aspect_name(w, h));
    }
    let mut label = parts.join(" · ");
    if beta_resolutions().contains(&name) {
        label.push_str(" (beta)");
    }
    label
}

/// The aspect ratio a render resolution belongs to, None if unknown.
pub fn aspect_of(name: &str) -> Option<String> {
    resolution_size(name).map(|(w, h)| aspect_name(w, h))
}

/// The aspect ratios of these resolutions: the named ones in table order, then the others sorted.
pub fn aspects(names: &[String]) -> Vec<String> {
    let present: BTreeSet<String> = names.iter().filter_map(|n| aspect_of(n)).collect();
    let mut out: Vec<String> = ASPECTS
        .iter()
        .filter(|(a, _)| present.contains(*a))
        .map(|(a, _)| (*a).to_string())
        .collect();
    out.extend(
        present
            .iter()
            .filter(|a| !ASPECTS.iter().any(|(n, _)| n == &a.as_str()))
            .cloned(),
    );
    out
}

fn pixels(name: &str) -> u64 {
    resolution_size(name).map_or(0, |(w, h)| u64::from(w) * u64::from(h))
}

/// The resolutions of one aspect ratio, smallest first, with native on top of its list.
pub fn resolutions_for(aspect: &str, names: &[String]) -> Vec<String> {
    let mut chosen: Vec<String> = names
        .iter()
        .filter(|n| aspect_of(n).as_deref() == Some(aspect))
        .cloned()
        .collect();
    chosen.sort_by_key(|n| (n != "native", pixels(n)));
    chosen
}

/// The resolution of `aspect` nearest to `current` in pixels (`current` itself when it is of it).
pub fn closest_resolution(aspect: &str, current: &str, names: &[String]) -> Option<String> {
    let chosen = resolutions_for(aspect, names);
    if chosen.iter().any(|n| n == current) {
        return Some(current.to_string());
    }
    let (w, h) = resolution_size(current).unwrap_or((1920, 1080));
    let target = u64::from(w) * u64::from(h);
    chosen
        .into_iter()
        .min_by_key(|n| pixels(n).abs_diff(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn a_short_label_drops_the_trailing_parenthetical() {
        assert_eq!(short_label("FSR 3.1 (every GPU)"), "FSR 3.1");
        assert_eq!(short_label("Quality (×1.5)"), "Quality");
        assert_eq!(short_label("3840 × 2160 (4K)"), "3840 × 2160");
        // Chinese and Japanese write the parenthesis full width.
        assert_eq!(short_label("FSR 3.1（任何显卡）"), "FSR 3.1");
        // Nothing to drop, or nothing left without it: the label as it is.
        assert_eq!(short_label("2560 × 1440"), "2560 × 1440");
        assert_eq!(short_label("Fullscreen"), "Fullscreen");
        assert_eq!(short_label("(4K)"), "(4K)");
        assert_eq!(
            short_label("Off (fixes black lines) now"),
            "Off (fixes black lines) now"
        );
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn labels_match_python() {
        assert_eq!(
            resolution_label("1440p", true),
            "1440p · 2560 × 1440 · 16:9"
        );
        assert_eq!(
            resolution_label("native", true),
            "Native · 1920 × 1080 · 16:9"
        );
        // 3440x1440 is in TESTED_RESOLUTIONS, so the Python gives no "(beta)" for it.
        assert_eq!(resolution_label("3440x1440", true), "3440 × 1440 · 21:9");
        assert_eq!(
            resolution_label("2560x1080", true),
            "2560 × 1080 · 21:9 (beta)"
        );
        assert_eq!(resolution_label("1440p", false), "1440p · 2560 × 1440");
        assert_eq!(resolution_label("bogus", true), "bogus");
    }

    #[test]
    fn aspect_of_2560x1080_is_21_9() {
        assert_eq!(aspect_of("2560x1080").as_deref(), Some("21:9"));
        assert_eq!(aspect_of("bogus"), None);
        assert_eq!(aspect_of("1000x800").as_deref(), Some("5:4"));
    }

    #[test]
    fn closest_resolution_moves_to_the_nearest_pixel_count() {
        let names = strings(RESOLUTIONS);
        // 1440p is 3.69M pixels; 21:9 sizes: 2560x1080 = 2.76M, 3440x1440 = 4.95M, 3840x1600 = 6.14M.
        assert_eq!(
            closest_resolution("21:9", "1440p", &names).as_deref(),
            Some("2560x1080")
        );
        assert_eq!(
            closest_resolution("21:9", "3840x1600", &names).as_deref(),
            Some("3840x1600")
        );
        assert_eq!(closest_resolution("5:4", "1440p", &names), None);
        assert_eq!(
            resolutions_for("16:9", &names)[0],
            "native",
            "native stays on top"
        );
        assert_eq!(aspects(&names)[0], "16:9");
    }

    #[test]
    fn every_table_has_unique_values() {
        let tables: [&[Choice]; 20] = [
            DISPLAY_MODES,
            ANISOTROPY,
            FPS_LIMITS,
            UI_SCALES,
            STICK_PROMPTS,
            AIM_STYLES,
            GLYPH_STYLES,
            UPSCALERS,
            PRESETS,
            OUTPUTS,
            LIVE_RESOLUTION,
            LANGUAGES,
            DRAW_PIPE,
            READBACKS,
            NO_MSAA,
            STREAM_MEMORY,
            FRAMES_QUEUED,
            UI_LANGUAGES,
            VIDEO_BACKENDS,
            ENGINE_FPS_CHOICES,
        ];
        for table in tables {
            let values: HashSet<_> = table.iter().map(|c| c.value).collect();
            assert_eq!(values.len(), table.len());
        }
        let names: HashSet<_> = RESOLUTIONS.iter().collect();
        assert_eq!(names.len(), RESOLUTIONS.len());
        assert_eq!(RESOLUTIONS.len(), 23);
    }

    #[test]
    fn gui_scale_is_1x_or_1_5x() {
        // R90: two choices, 1x first; what an older file stored is shown as the closest one.
        let values: Vec<&str> = UI_SCALES.iter().map(|choice| choice.value).collect();
        assert_eq!(values, ["1", "1.5"]);
        for (stored, shown) in [
            ("1", "1"),
            ("1.5", "1.5"),
            ("auto", "1"),
            ("2", "1.5"),
            ("", "1"),
            ("bogus", "1"),
        ] {
            assert_eq!(ui_scale_choice(stored), shown, "{stored:?}");
        }
    }

    #[test]
    fn beta_resolutions_are_the_untested_custom_ones() {
        assert_eq!(
            beta_resolutions(),
            ["2560x1080", "1440x1080", "2048x1536", "2560x1920", "8K"]
        );
    }
}
