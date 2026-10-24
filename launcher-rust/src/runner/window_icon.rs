//! The window icon (Python `run.py`, Pillow): a 64 x 64 BMP SDL loads without extra libraries.

use std::path::{Path, PathBuf};

use crate::core::fs::atomic_write;

/// Writes `out/window_icon.bmp` from the port's icon (64 x 64 RGBA, Lanczos). `None` when the icon
/// cannot be read or written: the window then keeps SDL's default.
///
/// The BMP is written here, as Pillow does: 40-byte info header, 32 bits, bottom-up BGRA rows.
/// The `image` encoder writes a larger V4 header for alpha, which Pillow's output does not use.
pub fn write(png: &Path, out: &Path) -> Option<PathBuf> {
    let icon = image::open(png).ok()?.to_rgba8();
    let small = image::imageops::resize(&icon, SIZE, SIZE, image::imageops::FilterType::Lanczos3);
    let target = out.join("window_icon.bmp");
    atomic_write(&target, &bmp_bytes(&small)).ok()?;
    Some(target)
}

const SIZE: u32 = 64;

fn bmp_bytes(icon: &image::RgbaImage) -> Vec<u8> {
    const HEADERS: u32 = 54;
    let pixels = SIZE * SIZE * 4;
    let mut bytes = Vec::with_capacity((HEADERS + pixels) as usize);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(HEADERS + pixels).to_le_bytes());
    bytes.extend_from_slice(&[0; 4]); // reserved
    bytes.extend_from_slice(&HEADERS.to_le_bytes()); // pixel data offset
    bytes.extend_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
    bytes.extend_from_slice(&SIZE.to_le_bytes()); // width
    bytes.extend_from_slice(&SIZE.to_le_bytes()); // height, positive: bottom-up
    bytes.extend_from_slice(&1u16.to_le_bytes()); // planes
    bytes.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    bytes.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    bytes.extend_from_slice(&pixels.to_le_bytes());
    bytes.extend_from_slice(&3780u32.to_le_bytes()); // 96 dpi, horizontal
    bytes.extend_from_slice(&3780u32.to_le_bytes()); // 96 dpi, vertical
    bytes.extend_from_slice(&[0; 8]); // palette colours, important colours
    for row in (0..SIZE).rev() {
        for column in 0..SIZE {
            let [r, g, b, a] = icon.get_pixel(column, row).0;
            bytes.extend_from_slice(&[b, g, r, a]);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_is_64_by_64_bmp() {
        let png = Path::new(env!("CARGO_MANIFEST_DIR")).join("../launcher/spartanport.png");
        let expected = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/runner/icon/expected/window_icon.bmp");
        let dir = tempfile::tempdir().unwrap();
        let written = write(&png, dir.path()).unwrap();
        assert_eq!(written, dir.path().join("window_icon.bmp"));
        let got = std::fs::read(&written).unwrap();
        let want = std::fs::read(&expected).unwrap();
        assert_eq!(got.len(), want.len());
        assert_eq!(got[..54], want[..54]);
    }

    #[test]
    fn the_icon_replaces_the_file_instead_of_writing_into_it() {
        // An atomic write: the new BMP takes the old file's place whole, so a second link to the
        // old file keeps its bytes.
        let png = Path::new(env!("CARGO_MANIFEST_DIR")).join("../launcher/spartanport.png");
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other.bmp");
        std::fs::write(&other, b"old").unwrap();
        std::fs::hard_link(&other, dir.path().join("window_icon.bmp")).unwrap();
        let written = write(&png, dir.path()).unwrap();
        assert_eq!(std::fs::read(&written).unwrap().len(), 54 + 64 * 64 * 4);
        assert!(
            std::fs::read(&other).unwrap() == b"old",
            "the BMP was written into the old file"
        );
    }

    #[test]
    fn missing_png_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(write(&dir.path().join("none.png"), dir.path()), None);
    }
}
