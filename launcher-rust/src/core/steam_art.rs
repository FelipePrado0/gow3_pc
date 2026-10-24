//! The pictures Steam shows for the shortcut, drawn as `scripts/steam_shortcut.py` draws them:
//! the vertical and horizontal capsules (with the logo), the hero background and the transparent
//! logo, made from the game dump's `sce_sys/pic1.png` and the launcher's icon.

use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use crate::core::draw::{self, Draw, Filter, Font, FontSize, Mode, Picture};
use crate::core::fs::atomic_write;
use crate::core::steam::SteamError;

/// Vertical capsule: `<id>p.png`.
pub const CAPSULE: (usize, usize) = (600, 900);
/// Horizontal capsule: `<id>.png`.
pub const WIDE: (usize, usize) = (920, 430);
/// Library background: `<id>_hero.png`.
pub const HERO: (usize, usize) = (1920, 620);
pub const GOLD: [u8; 3] = [233, 214, 164];

const TITLE: &str = "SPARTANPORT";

/// Fonts to try for the title, the port's Cinzel last.
pub fn fonts(port_dir: &Path) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = [
        "C:/Windows/Fonts/georgiab.ttf",
        "C:/Windows/Fonts/timesbd.ttf",
        "/usr/share/fonts/TTF/DejaVuSerif-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSerif-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf",
        "/usr/share/fonts/noto/NotoSerif-Bold.ttf",
        "/usr/share/fonts/truetype/noto/NotoSerif-Bold.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    list.push(port_dir.join("launcher").join("Cinzel-Black.ttf"));
    list
}

/// The first candidate that opens as a font.
pub fn open_font(candidates: &[PathBuf]) -> io::Result<Font> {
    candidates
        .iter()
        .find_map(|path| Font::open(path).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no font for the Steam pictures"))
}

/// Python's `round` of a float: halves to even.
fn round(value: f64) -> i64 {
    value.round_ties_even() as i64
}

/// `image` scaled to fill `width` x `height` and cropped around the focus point (0..1 each axis).
pub fn cover(image: &Picture, width: usize, height: usize, focus: (f64, f64)) -> Picture {
    let scale = (width as f64 / image.width as f64).max(height as f64 / image.height as f64);
    let resized = image.resize(
        round(image.width as f64 * scale).max(1) as usize,
        round(image.height as f64 * scale).max(1) as usize,
        Filter::Lanczos,
    );
    let left = round((resized.width as f64 - width as f64) * focus.0);
    let top = round((resized.height as f64 - height as f64) * focus.1);
    resized.crop(left, top, left + width as i64, top + height as i64)
}

fn spaced_width(text: &str, face: &FontSize, gap: f64) -> f64 {
    let letters: f64 = text.chars().map(|c| face.length(&c.to_string())).sum();
    letters + gap * (text.chars().count() as f64 - 1.0)
}

/// A font whose text (letters spaced by `gap_ratio` of the size) is `target` px wide: the face,
/// its integer size and the gap.
fn fit_face<'a>(
    font: &'a Font,
    text: &str,
    target: f64,
    gap_ratio: f64,
) -> (FontSize<'a>, i64, f64) {
    let mut size = target / 8.0;
    for _ in 0..3 {
        let face = font.at(round(size));
        let width = spaced_width(text, &face, size * gap_ratio);
        size *= target / width;
    }
    (font.at(round(size)), round(size), size * gap_ratio)
}

/// `text` with `gap` px between letters, centered on `center_x`.
fn spaced(
    draw: &mut Draw,
    center_x: f64,
    y: f64,
    text: &str,
    face: &FontSize,
    fill: &[u8],
    gap: f64,
) {
    let letters: Vec<String> = text.chars().map(String::from).collect();
    let widths: Vec<f64> = letters.iter().map(|c| face.length(c)).collect();
    let mut x = center_x - (widths.iter().sum::<f64>() + gap * (letters.len() as f64 - 1.0)) / 2.0;
    for (letter, width) in letters.iter().zip(&widths) {
        draw.text((x, y), letter, face, fill, 0);
        x += width + gap;
    }
}

/// Whether the icon can be the logo's emblem: large and with a transparent background.
pub fn usable_emblem(image: &Picture) -> bool {
    let rgba = image.convert(Mode::Rgba);
    rgba.width >= 200 && rgba.height >= 2 && rgba.pixel(1, 1)[3] < 40
}

/// Transparent logo: the emblem (when there is one) over SPARTANPORT, `width` px wide.
pub fn make_logo(emblem: Option<&Picture>, width: usize, font: &Font) -> Picture {
    let scale = 2;
    let w = width * scale;
    let emblem_size = if emblem.is_some() {
        round(w as f64 * 0.30) as usize
    } else {
        0
    };
    let (title_face, title_size, title_gap) = fit_face(font, TITLE, w as f64 * 0.94, 0.04);
    let title_y = emblem_size as f64
        + if emblem.is_some() {
            w as f64 * 0.03
        } else {
            0.0
        };
    let height = round(title_y + title_size as f64 * 1.5).max(1) as usize;
    let mut canvas = Picture::new(Mode::Rgba, w, height, &[0, 0, 0, 0]);
    if let Some(emblem) = emblem {
        let mark = emblem
            .convert(Mode::Rgba)
            .resize(emblem_size, emblem_size, Filter::Lanczos);
        canvas.alpha_composite_at(&mark, ((w - emblem_size) / 2) as i64, 0);
    }
    let mut shadow = Picture::new(Mode::Rgba, w, height, &[0, 0, 0, 0]);
    let offset = (3 * scale) as f64;
    spaced(
        &mut Draw::new(&mut shadow),
        w as f64 / 2.0 + offset,
        title_y + offset,
        TITLE,
        &title_face,
        &[0, 0, 0, 220],
        title_gap,
    );
    spaced(
        &mut Draw::new(&mut canvas),
        w as f64 / 2.0,
        title_y,
        TITLE,
        &title_face,
        &[GOLD[0], GOLD[1], GOLD[2], 255],
        title_gap,
    );
    let mut blurred = shadow.gaussian_blur((2 * scale) as f32);
    blurred.alpha_composite(&canvas);
    blurred.resize(
        width,
        round(height as f64 / scale as f64) as usize,
        Filter::Lanczos,
    )
}

/// `image` darkened towards the bottom: black over it by a mask that grows from `start` down.
pub fn darken_bottom(image: &Picture, strength: f64, start: f64) -> Picture {
    let mut mask = Picture::new(Mode::L, image.width, image.height, &[]);
    for y in 0..image.height {
        let t = ((y as f64 / image.height as f64 - start) / (1.0 - start)).max(0.0);
        let value = round(255.0 * strength * t) as u8;
        mask.data[y * image.width..(y + 1) * image.width].fill(value);
    }
    let black = Picture::new(Mode::Rgba, image.width, image.height, &[0, 0, 0, 255]);
    draw::composite(&black, image, &mask)
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

fn load(path: &Path) -> io::Result<Picture> {
    let image = image::open(path).map_err(invalid)?;
    Ok(Picture::from_rgba(&image.to_rgba8()))
}

fn save(picture: &Picture, path: &Path) -> io::Result<()> {
    let mut bytes = Cursor::new(Vec::new());
    let result = match picture.mode {
        Mode::Rgba => picture
            .to_rgba_image()
            .write_to(&mut bytes, image::ImageFormat::Png),
        _ => {
            let rgb = picture.convert(Mode::Rgb);
            image::RgbImage::from_raw(rgb.width as u32, rgb.height as u32, rgb.data)
                .ok_or_else(|| image::ImageError::IoError(io::ErrorKind::InvalidData.into()))
                .and_then(|image| image.write_to(&mut bytes, image::ImageFormat::Png))
        }
    };
    result.map_err(invalid)?;
    atomic_write(path, &bytes.into_inner())
}

/// Writes the five pictures of shortcut `app_id` into `out_dir`; returns the icon's path.
pub fn make_art(
    game: &Path,
    icon: &Path,
    out_dir: &Path,
    app_id: u32,
    font: &Font,
) -> Result<PathBuf, SteamError> {
    let sce_sys = game.join("sce_sys");
    let background = ["pic1.png", "pic0.png"]
        .iter()
        .map(|name| sce_sys.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| SteamError::NoArt(sce_sys.clone()))?;
    let base = load(&background)?;
    let fallback = sce_sys.join("icon0.png");
    let picture = load(if icon.is_file() { icon } else { &fallback })?;
    let emblem = usable_emblem(&picture).then_some(&picture);
    std::fs::create_dir_all(out_dir)?;

    let hero = cover(&base, HERO.0, HERO.1, (0.5, 0.52));
    save(
        &hero.convert(Mode::Rgb),
        &out_dir.join(format!("{app_id}_hero.png")),
    )?;
    let logo = make_logo(emblem, 1000, font);
    save(&logo, &out_dir.join(format!("{app_id}_logo.png")))?;

    let mut wide = cover(&base, WIDE.0, WIDE.1, (0.5, 0.5));
    let small = make_logo(emblem, 380, font);
    wide.alpha_composite_at(&small, 48, ((WIDE.1 - small.height) / 2) as i64);
    save(
        &wide.convert(Mode::Rgb),
        &out_dir.join(format!("{app_id}.png")),
    )?;

    let mut tall = darken_bottom(&cover(&base, CAPSULE.0, CAPSULE.1, (0.9, 0.5)), 0.85, 0.45);
    let big = make_logo(emblem, 500, font);
    tall.alpha_composite_at(
        &big,
        ((CAPSULE.0 - big.width) / 2) as i64,
        CAPSULE.1 as i64 - big.height as i64 - 50,
    );
    save(
        &tall.convert(Mode::Rgb),
        &out_dir.join(format!("{app_id}p.png")),
    )?;

    let icon_path = out_dir.join(format!("{app_id}_icon.png"));
    save(&picture.resize(256, 256, Filter::Lanczos), &icon_path)?;
    atomic_write(
        &out_dir.join(format!("{app_id}.json")),
        br#"{"nVersion": 1, "logoPosition": {"pinnedPosition": "BottomLeft", "nWidthPct": 42, "nHeightPct": 56}}"#,
    )?;
    Ok(icon_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cinzel() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("launcher")
            .join("Cinzel-Black.ttf")
    }

    fn font() -> Font {
        open_font(&[cinzel()]).unwrap()
    }

    fn write_png(path: &Path, picture: &Picture) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        picture.to_rgba_image().save(path).unwrap();
    }

    fn dump(dir: &Path, name: &str) -> PathBuf {
        let game = dir.join("game");
        let background = Picture::new(Mode::Rgba, 480, 270, &[90, 20, 20]);
        write_png(&game.join("sce_sys").join(name), &background);
        game
    }

    #[test]
    fn only_a_large_icon_with_a_transparent_background_is_an_emblem() {
        let clear = |side| Picture::new(Mode::Rgba, side, side, &[0, 0, 0, 0]);
        assert!(usable_emblem(&clear(256)));
        assert!(!usable_emblem(&clear(128)));
        assert!(!usable_emblem(&Picture::new(
            Mode::Rgb,
            512,
            512,
            &[200, 200, 200]
        )));
    }

    #[test]
    fn the_logo_without_an_emblem_is_just_the_title_lines() {
        let font = font();
        let emblem = Picture::new(Mode::Rgba, 256, 256, &[230, 180, 40, 255]);
        let with = make_logo(Some(&emblem), 600, &font);
        let without = make_logo(None, 600, &font);
        assert_eq!(with.width, without.width);
        assert!(without.height < with.height);
        let bbox = without.band(3).bbox();
        assert!(bbox.is_some_and(|b| b.3 > 0));
    }

    #[test]
    fn make_art_writes_the_five_pictures() {
        let dir = tempfile::tempdir().unwrap();
        let game = dump(dir.path(), "pic1.png");
        let icon = dir.path().join("icon.png");
        write_png(&icon, &Picture::new(Mode::Rgba, 256, 256, &[0, 0, 0, 0]));
        let out = dir.path().join("out");
        let result = make_art(&game, &icon, &out, 77, &font()).unwrap();
        assert_eq!(result, out.join("77_icon.png"));
        let open = |name: &str| image::open(out.join(name)).unwrap();
        for (name, size) in [
            ("77p.png", CAPSULE),
            ("77.png", WIDE),
            ("77_hero.png", HERO),
        ] {
            let picture = open(name);
            assert!(
                matches!(picture, image::DynamicImage::ImageRgb8(_)),
                "{name}"
            );
            assert_eq!((picture.width() as usize, picture.height() as usize), size);
        }
        let logo = open("77_logo.png");
        assert!(matches!(logo, image::DynamicImage::ImageRgba8(_)));
        assert_eq!(logo.width(), 1000);
        let icon = open("77_icon.png");
        assert_eq!((icon.width(), icon.height()), (256, 256));
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.join("77.json")).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"nVersion": 1, "logoPosition": {
                "pinnedPosition": "BottomLeft", "nWidthPct": 42, "nHeightPct": 56}})
        );
    }

    #[test]
    fn pic0_stands_in_for_pic1() {
        let dir = tempfile::tempdir().unwrap();
        let game = dump(dir.path(), "pic0.png");
        write_png(
            &dir.path().join("icon.png"),
            &Picture::new(Mode::Rgba, 64, 64, &[1, 2, 3, 255]),
        );
        let result = make_art(
            &game,
            &dir.path().join("icon.png"),
            &dir.path().join("out"),
            5,
            &font(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn no_background_is_no_art() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        std::fs::create_dir_all(game.join("sce_sys")).unwrap();
        let result = make_art(
            &game,
            &dir.path().join("icon.png"),
            &dir.path().join("out"),
            5,
            &font(),
        );
        assert!(matches!(result, Err(SteamError::NoArt(p)) if p == game.join("sce_sys")));
    }

    #[test]
    fn a_missing_icon_falls_back_to_icon0() {
        let dir = tempfile::tempdir().unwrap();
        let game = dump(dir.path(), "pic1.png");
        write_png(
            &game.join("sce_sys").join("icon0.png"),
            &Picture::new(Mode::Rgba, 64, 64, &[1, 2, 3, 255]),
        );
        let result = make_art(
            &game,
            &dir.path().join("missing.png"),
            &dir.path().join("out"),
            5,
            &font(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn fonts_end_with_the_ports_cinzel() {
        let list = fonts(Path::new("/port"));
        assert_eq!(list.len(), 8);
        assert_eq!(list[0], PathBuf::from("C:/Windows/Fonts/georgiab.ttf"));
        assert_eq!(
            list[7],
            Path::new("/port").join("launcher").join("Cinzel-Black.ttf")
        );
    }
}
