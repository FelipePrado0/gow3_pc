//! The picture operations `scripts/glyphs.py` asks of Pillow 12.3.0, ported so the drawings come
//! out as the Python's: `ImageDraw` shapes (`src/libImaging/Draw.c`), resampling (`Resample.c`),
//! the box-blur Gaussian (`BoxBlur.c`), rank filters (`RankFilter.c`), rotation (`Geometry.c`),
//! `ImageChops`, pasting (`Paste.c`), alpha compositing (`AlphaComposite.c`) and the conversions
//! between modes (`Convert.c`) (license in `assets/LICENSE-Pillow.txt`). The arithmetic keeps the
//! C's types (`f32` where the C uses `float`, fixed point where it does), so those give the same
//! pixels.
//!
//! Text is the exception: Pillow draws it with FreeType, which auto-hints these fonts (they carry
//! no hinting program). The hinted outlines and advances come from `skrifa`'s port of FreeType's
//! auto-hinter, placed and measured as Pillow's basic layout places and measures them (26.6
//! positions, bounding boxes of the control points), and `ab_glyph_rasterizer` fills them. The
//! coverage differs from FreeType's by a level here and there; a stroke is drawn by widening the
//! coverage.

use std::collections::VecDeque;
use std::f64::consts::PI;
use std::io;
use std::path::Path;

use ab_glyph_rasterizer::{point, Point, Rasterizer};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{
    DrawSettings, Engine, GlyphStyles, HintingInstance, HintingOptions, OutlinePen, SmoothMode,
};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

// -- Pictures -------------------------------------------------------------------------------------

/// Pillow's image modes used here: one band, red green blue, and with alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    L,
    Rgb,
    Rgba,
}

impl Mode {
    pub fn bands(self) -> usize {
        match self {
            Mode::L => 1,
            Mode::Rgb => 3,
            Mode::Rgba => 4,
        }
    }
}

/// A picture as Pillow holds it: a mode and its bands, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub mode: Mode,
    pub data: Vec<u8>,
}

/// `Image.transpose` operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transpose {
    Rotate90,
    Rotate180,
    Rotate270,
}

/// The resampling filters of `Image.resize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Bicubic,
    Lanczos,
}

/// Part of a picture: columns `x0..x1`, rows `y0..y1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

/// `ImagingAlphaComposite` for one RGBA pixel: `src` over `dst`.
fn composite_pixel(dst: &mut [u8], src: &[u8]) {
    const PRECISION: u32 = 7;
    if src[3] == 0 {
        return;
    }
    let (sa, da) = (u32::from(src[3]), u32::from(dst[3]));
    let mix = da * (255 - sa);
    let outa255 = sa * 255 + mix;
    let coef1 = sa * 255 * 255 * (1 << PRECISION) / outa255;
    let coef2 = 255 * (1 << PRECISION) - coef1;
    for band in 0..3 {
        let tmp = u32::from(src[band]) * coef1 + u32::from(dst[band]) * coef2;
        dst[band] = (shift_div255(tmp + (0x80 << PRECISION)) >> PRECISION) as u8;
    }
    dst[3] = shift_div255(outa255 + 0x80) as u8;
}

impl Picture {
    /// `Image.new(mode, (width, height), color)`.
    pub fn new(mode: Mode, width: usize, height: usize, color: &[u8]) -> Picture {
        let bands = mode.bands();
        let mut pixel = vec![0u8; bands];
        for (band, value) in pixel.iter_mut().zip(color) {
            *band = *value;
        }
        if mode == Mode::Rgba && color.len() == 3 {
            pixel[3] = 255;
        }
        Picture {
            width,
            height,
            mode,
            data: pixel.repeat(width * height),
        }
    }

    pub fn from_rgba(image: &image::RgbaImage) -> Picture {
        Picture {
            width: image.width() as usize,
            height: image.height() as usize,
            mode: Mode::Rgba,
            data: image.as_raw().clone(),
        }
    }

    pub fn from_rgb(image: &image::RgbImage) -> Picture {
        Picture {
            width: image.width() as usize,
            height: image.height() as usize,
            mode: Mode::Rgb,
            data: image.as_raw().clone(),
        }
    }

    /// The picture as RGBA (alpha 255 without one).
    pub fn to_rgba_image(&self) -> image::RgbaImage {
        let rgba = self.convert(Mode::Rgba);
        image::RgbaImage::from_raw(self.width as u32, self.height as u32, rgba.data)
            .unwrap_or_default()
    }

    pub fn bands(&self) -> usize {
        self.mode.bands()
    }

    fn at(&self, x: usize, y: usize) -> usize {
        (y * self.width + x) * self.bands()
    }

    /// The bands of the pixel at `x`, `y`.
    pub fn pixel(&self, x: usize, y: usize) -> &[u8] {
        let at = self.at(x, y);
        &self.data[at..at + self.bands()]
    }

    /// `Image.crop((x0, y0, x1, y1))`: what lies outside the picture is zero.
    pub fn crop(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> Picture {
        let (width, height) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
        let bands = self.bands();
        let mut out = Picture::new(self.mode, width, height, &[]);
        for y in 0..height {
            let sy = y0 + y as i64;
            if sy < 0 || sy >= self.height as i64 {
                continue;
            }
            let from = x0.max(0);
            let to = x1.min(self.width as i64);
            if from >= to {
                continue;
            }
            let source = self.at(from as usize, sy as usize);
            let target = out.at((from - x0) as usize, y);
            let count = (to - from) as usize * bands;
            out.data[target..target + count].copy_from_slice(&self.data[source..source + count]);
        }
        out
    }

    /// `Image.convert(mode)` between these modes (RGB and RGBA to L by ITU-R 601-2, as Pillow).
    pub fn convert(&self, mode: Mode) -> Picture {
        if mode == self.mode {
            return self.clone();
        }
        let bands = self.bands();
        let mut data = Vec::with_capacity(self.width * self.height * mode.bands());
        for pixel in self.data.chunks_exact(bands) {
            match (self.mode, mode) {
                (Mode::L, Mode::Rgb) => data.extend_from_slice(&[pixel[0]; 3]),
                (Mode::L, Mode::Rgba) => {
                    data.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255])
                }
                (_, Mode::L) => data.push(luma(pixel)),
                (Mode::Rgb, Mode::Rgba) => {
                    data.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255])
                }
                (Mode::Rgba, Mode::Rgb) => data.extend_from_slice(&pixel[..3]),
                _ => data.extend_from_slice(pixel),
            }
        }
        Picture {
            width: self.width,
            height: self.height,
            mode,
            data,
        }
    }

    /// Band `index` as an L picture (`Image.split()[index]`).
    pub fn band(&self, index: usize) -> Picture {
        let bands = self.bands();
        Picture {
            width: self.width,
            height: self.height,
            mode: Mode::L,
            data: self
                .data
                .iter()
                .skip(index)
                .step_by(bands)
                .copied()
                .collect(),
        }
    }

    /// `Image.merge(mode, bands)`.
    pub fn merge(mode: Mode, bands: &[&Picture]) -> Picture {
        let (width, height) = bands
            .first()
            .map(|band| (band.width, band.height))
            .unwrap_or((0, 0));
        let mut data = Vec::with_capacity(width * height * bands.len());
        for i in 0..width * height {
            for band in bands {
                data.push(band.data.get(i).copied().unwrap_or(0));
            }
        }
        Picture {
            width,
            height,
            mode,
            data,
        }
    }

    /// `Image.paste(source, (x, y), mask)`: `source` in this picture's mode, blended through the L
    /// `mask` (of the source's size) where there is one.
    pub fn paste(&mut self, source: &Picture, x: i64, y: i64, mask: Option<&Picture>) {
        let source = source.convert(self.mode);
        let bands = self.bands();
        let Some((dx, dy, sx, sy, width, height)) =
            clip_region(self, x, y, source.width, source.height)
        else {
            return;
        };
        for row in 0..height {
            let out = self.at(dx, dy + row);
            let input = source.at(sx, sy + row);
            match mask {
                None => {
                    self.data[out..out + width * bands]
                        .copy_from_slice(&source.data[input..input + width * bands]);
                }
                Some(mask) => {
                    for column in 0..width {
                        let m = mask.data[mask.at(sx + column, sy + row)];
                        for band in 0..bands {
                            let at = out + column * bands + band;
                            self.data[at] =
                                blend(m, self.data[at], source.data[input + column * bands + band]);
                        }
                    }
                }
            }
        }
    }

    /// `Image.paste(color, (x0, y0, x1, y1), mask)` (`ImagingFill2`): RGB and RGBA colours fill as
    /// Pillow's `fill_mask_L`, which paints a transparent pixel's colour fully.
    pub fn fill(
        &mut self,
        color: &[u8],
        x0: i64,
        y0: i64,
        x1: i64,
        y1: i64,
        mask: Option<&Picture>,
    ) {
        let bands = self.bands();
        let ink = ink(self.mode, color);
        let Some((dx, dy, sx, sy, width, height)) = clip_region(
            self,
            x0,
            y0,
            (x1 - x0).max(0) as usize,
            (y1 - y0).max(0) as usize,
        ) else {
            return;
        };
        for row in 0..height {
            for column in 0..width {
                let out = self.at(dx + column, dy + row);
                match mask {
                    None => self.data[out..out + bands].copy_from_slice(&ink),
                    Some(mask) => {
                        let m = mask.data[mask.at(sx + column, sy + row)];
                        fill_pixel(&mut self.data[out..out + bands], &ink, m);
                    }
                }
            }
        }
    }

    /// `Image.point(function)`: every band through the same table.
    pub fn point(&self, function: impl Fn(u8) -> u8) -> Picture {
        let table: Vec<u8> = (0..=255u8).map(function).collect();
        Picture {
            data: self.data.iter().map(|v| table[*v as usize]).collect(),
            ..self.clone()
        }
    }

    /// `Image.histogram(mask)` of an L picture: how many pixels of each value, under `mask` where
    /// it is not zero.
    pub fn histogram(&self, mask: Option<&Picture>) -> [u64; 256] {
        let mut counts = [0u64; 256];
        for (i, value) in self.data.iter().enumerate() {
            if mask.is_none_or(|m| m.data.get(i).is_some_and(|v| *v != 0)) {
                counts[*value as usize] += 1;
            }
        }
        counts
    }

    /// `Image.getbbox()` of an L picture: the box of its non-zero pixels, `None` when all are zero.
    pub fn bbox(&self) -> Option<(usize, usize, usize, usize)> {
        let bands = self.bands();
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..self.height {
            for x in 0..self.width {
                let at = self.at(x, y);
                if self.data[at..at + bands].iter().any(|v| *v != 0) {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        (x1 > 0).then_some((x0, y0, x1, y1))
    }

    /// `Image.transpose(method)`.
    pub fn transpose(&self, method: Transpose) -> Picture {
        let bands = self.bands();
        let (width, height) = match method {
            Transpose::Rotate180 => (self.width, self.height),
            _ => (self.height, self.width),
        };
        let mut out = Picture::new(self.mode, width, height, &[]);
        for y in 0..self.height {
            for x in 0..self.width {
                let (tx, ty) = match method {
                    Transpose::Rotate90 => (y, self.width - 1 - x),
                    Transpose::Rotate180 => (self.width - 1 - x, self.height - 1 - y),
                    Transpose::Rotate270 => (self.height - 1 - y, x),
                };
                let (from, to) = (self.at(x, y), out.at(tx, ty));
                out.data[to..to + bands].copy_from_slice(&self.data[from..from + bands]);
            }
        }
        out
    }

    /// Swaps rows and columns (`ImagingTranspose`).
    fn swap_axes(&self) -> Picture {
        let bands = self.bands();
        let (w, h) = (self.width, self.height);
        let mut out = Picture::new(self.mode, h, w, &[]);
        for y in 0..h {
            for x in 0..w {
                let (from, to) = ((y * w + x) * bands, (x * h + y) * bands);
                for band in 0..bands {
                    out.data[to + band] = self.data[from + band];
                }
            }
        }
        out
    }

    /// `ImageChops.offset(image, dx, dy)`: the picture moved by `dx`, `dy`, wrapping around.
    pub fn offset(&self, dx: i64, dy: i64) -> Picture {
        let bands = self.bands();
        let (w, h) = (self.width as i64, self.height as i64);
        let mut out = Picture::new(self.mode, self.width, self.height, &[]);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = ((x - dx).rem_euclid(w), (y - dy).rem_euclid(h));
                let (from, to) = (
                    self.at(sx as usize, sy as usize),
                    out.at(x as usize, y as usize),
                );
                out.data[to..to + bands].copy_from_slice(&self.data[from..from + bands]);
            }
        }
        out
    }

    /// `self.alpha_composite(over)` of two RGBA pictures of one size (`ImagingAlphaComposite`).
    pub fn alpha_composite(&mut self, over: &Picture) {
        for (dst, src) in self.data.chunks_exact_mut(4).zip(over.data.chunks_exact(4)) {
            composite_pixel(dst, src);
        }
    }

    /// `self.alpha_composite(over, dest=(x, y))` of two RGBA pictures: the part of `over` inside
    /// `self` goes through the arithmetic of [`Picture::alpha_composite`]; what falls outside is
    /// cut.
    pub fn alpha_composite_at(&mut self, over: &Picture, x: i64, y: i64) {
        let (width, height) = (self.width as i64, self.height as i64);
        let (over_width, over_height) = (over.width as i64, over.height as i64);
        for oy in (-y).max(0)..over_height.min(height - y) {
            for ox in (-x).max(0)..over_width.min(width - x) {
                let from = self.at((x + ox) as usize, (y + oy) as usize);
                let src = over.at(ox as usize, oy as usize);
                composite_pixel(&mut self.data[from..from + 4], &over.data[src..src + 4]);
            }
        }
    }

    /// `Image.resize((width, height), filter)`: RGBA premultiplied for the resampling, the same
    /// size copied.
    pub fn resize(&self, width: usize, height: usize, filter: Filter) -> Picture {
        self.resize_window(
            width,
            height,
            filter,
            Window {
                x0: 0,
                y0: 0,
                x1: width,
                y1: height,
            },
        )
    }

    /// [`Picture::resize`] computing only the pixels of `window` (the others stay zero): the
    /// same values a whole resize gives there, for a fraction of the work.
    pub fn resize_window(
        &self,
        width: usize,
        height: usize,
        filter: Filter,
        window: Window,
    ) -> Picture {
        if (width, height) == (self.width, self.height) {
            return self.clone();
        }
        let mut out = resample(&[Canvas::whole(self)], width, height, filter, window);
        out.pop()
            .unwrap_or_else(|| Picture::new(self.mode, width, height, &[]))
    }

    /// [`Picture::resize_window`] of this picture and `other` (same mode and size), the rows they
    /// have in common resampled once.
    pub fn resize_window_pair(
        &self,
        other: &Picture,
        width: usize,
        height: usize,
        filter: Filter,
        window: Window,
    ) -> (Picture, Picture) {
        if (width, height) == (self.width, self.height) {
            return (self.clone(), other.clone());
        }
        let sources = [Canvas::whole(self), Canvas::whole(other)];
        let mut out = resample(&sources, width, height, filter, window);
        let second = out
            .pop()
            .unwrap_or_else(|| Picture::new(other.mode, width, height, &[]));
        let first = out
            .pop()
            .unwrap_or_else(|| Picture::new(self.mode, width, height, &[]));
        (first, second)
    }

    /// [`Picture::resize`] of a `full_width` x `full_height` picture that is zero but for this one,
    /// placed at `left`, `top` (a letter on a transparent layer): only the outputs its non-zero
    /// pixels reach are computed; the others are zero, as in a whole resize.
    #[allow(clippy::too_many_arguments)]
    pub fn resize_placed(
        &self,
        left: usize,
        top: usize,
        full_width: usize,
        full_height: usize,
        width: usize,
        height: usize,
        filter: Filter,
    ) -> Picture {
        let Some((x0, y0, x1, y1)) = self.bbox() else {
            return Picture::new(self.mode, width, height, &[]);
        };
        let (x0, y0, x1, y1) = (x0 + left, y0 + top, x1 + left, y1 + top);
        let reach = |in_size: usize, out_size: usize, from: usize, to: usize| -> (usize, usize) {
            if in_size == out_size {
                return (from, to);
            }
            let coefficients = Coefficients::new(in_size, out_size, filter);
            let hits: Vec<usize> = (0..out_size)
                .filter(|i| {
                    let (start, count) = coefficients.bounds[*i];
                    start < to && start + count > from
                })
                .collect();
            (
                hits.first().copied().unwrap_or(0),
                hits.last().map_or(0, |last| last + 1),
            )
        };
        let (wx0, wx1) = reach(full_width, width, x0, x1);
        let (wy0, wy1) = reach(full_height, height, y0, y1);
        let window = Window {
            x0: wx0,
            y0: wy0,
            x1: wx1,
            y1: wy1,
        };
        let canvas = Canvas {
            picture: self,
            left,
            top,
            width: full_width,
            height: full_height,
        };
        let mut out = resample(&[canvas], width, height, filter, window);
        out.pop()
            .unwrap_or_else(|| Picture::new(self.mode, width, height, &[]))
    }

    /// `Image.rotate(angle, Image.BICUBIC)` about the centre, without expanding: square pictures
    /// turned by 90, 180 or 270 degrees are transposed, as Pillow does.
    pub fn rotate(&self, angle: f64) -> Picture {
        let angle = py_mod(angle, 360.0);
        if angle == 0.0 {
            return self.clone();
        }
        if angle == 180.0 {
            return self.transpose(Transpose::Rotate180);
        }
        if (angle == 90.0 || angle == 270.0) && self.width == self.height {
            return self.transpose(if angle == 90.0 {
                Transpose::Rotate90
            } else {
                Transpose::Rotate270
            });
        }
        if self.mode == Mode::Rgba {
            let mut premultiplied = self.clone();
            for pixel in premultiplied.data.chunks_exact_mut(4) {
                for band in 0..3 {
                    pixel[band] = muldiv255(pixel[band], pixel[3]);
                }
            }
            let mut turned = premultiplied.rotate_bicubic(angle);
            for pixel in turned.data.chunks_exact_mut(4) {
                unpremultiply(pixel);
            }
            return turned;
        }
        self.rotate_bicubic(angle)
    }

    fn rotate_bicubic(&self, angle: f64) -> Picture {
        let (w, h) = (self.width as f64, self.height as f64);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let radians = -angle.to_radians();
        let mut m = [
            round15(radians.cos()),
            round15(radians.sin()),
            0.0,
            round15(-radians.sin()),
            round15(radians.cos()),
            0.0,
        ];
        let (tx, ty) = (
            m[0] * -cx + m[1] * -cy + m[2],
            m[3] * -cx + m[4] * -cy + m[5],
        );
        m[2] = tx + cx;
        m[5] = ty + cy;
        let bands = self.bands();
        let (width, height) = (self.width as i32, self.height as i32);
        let (wf, hf) = (self.width as f64, self.height as f64);
        let mut out = Picture::new(self.mode, self.width, self.height, &[]);
        for y in 0..self.height {
            for x in 0..self.width {
                // `affine_transform`, then `bicubic_filter8` / `bicubic_filter32RGB`.
                let (xin, yin) = (x as f64 + 0.5, y as f64 + 0.5);
                let xx = m[0] * xin + m[1] * yin + m[2];
                let yy = m[3] * xin + m[4] * yin + m[5];
                if xx < 0.0 || xx >= wf || yy < 0.0 || yy >= hf {
                    continue;
                }
                let (xs, ys) = (xx - 0.5, yy - 0.5);
                let (fx, fy) = (floor_int(xs), floor_int(ys));
                let (dx, dy) = (xs - f64::from(fx), ys - f64::from(fy));
                let (sx, sy) = (fx - 1, fy - 1);
                let column = |i: i32| (sx + i).clamp(0, width - 1) as usize * bands;
                let columns = [column(0), column(1), column(2), column(3)];
                let row = |i: i32| {
                    ((sy + i) >= 0 && (sy + i) < height)
                        .then(|| (sy + i) as usize * self.width * bands)
                };
                let first = sy.clamp(0, height - 1) as usize * self.width * bands;
                let rows = [Some(first), row(1), row(2), row(3)];
                let at = (y * self.width + x) * bands;
                for band in 0..bands {
                    let mut values = [0.0f64; 4];
                    for (i, start) in rows.iter().enumerate() {
                        values[i] = match start {
                            Some(start) => {
                                let d = &self.data[start + band..];
                                cubic(
                                    f64::from(d[columns[0]]),
                                    f64::from(d[columns[1]]),
                                    f64::from(d[columns[2]]),
                                    f64::from(d[columns[3]]),
                                    dx,
                                )
                            }
                            None => values[i - 1],
                        };
                    }
                    let v = cubic(values[0], values[1], values[2], values[3], dy);
                    out.data[at + band] = if v <= 0.0 {
                        0
                    } else if v >= 255.0 {
                        255
                    } else {
                        v as u8
                    };
                }
            }
        }
        out
    }

    /// `ImageFilter.GaussianBlur(radius)`: Pillow's three passes of an extended box blur.
    pub fn gaussian_blur(&self, radius: f32) -> Picture {
        if radius == 0.0 || self.width == 0 || self.height == 0 {
            return self.clone();
        }
        let box_radius = gaussian_box_radius(radius, 3);
        let mut out = self.clone();
        if box_radius != 0.0 {
            for _ in 0..3 {
                out = out.box_blur_rows(box_radius);
            }
            let mut turned = out.swap_axes();
            for _ in 0..3 {
                turned = turned.box_blur_rows(box_radius);
            }
            out = turned.swap_axes();
        }
        out
    }

    /// `ImagingHorizontalBoxBlur`: one box blur pass along every row.
    fn box_blur_rows(&self, float_radius: f32) -> Picture {
        let radius = float_radius as i32;
        let ww = (16_777_216.0f32 / (float_radius * 2.0 + 1.0)) as u32;
        let fw = (1u32 << 24).wrapping_sub(((radius * 2 + 1) as u32).wrapping_mul(ww)) / 2;
        let xsize = self.width as i32;
        let edge_a = (radius + 1).min(xsize);
        let edge_b = (xsize - radius - 1).max(0);
        let bands = self.bands();
        let mut out = self.clone();
        let row = self.width * bands;
        let mut values = vec![0u32; self.width];
        let mut result = vec![0u8; self.width];
        for y in 0..self.height {
            for band in 0..bands {
                for (x, value) in values.iter_mut().enumerate() {
                    *value = u32::from(self.data[y * row + x * bands + band]);
                }
                box_blur_line(
                    &mut result,
                    &values,
                    xsize - 1,
                    radius,
                    edge_a,
                    edge_b,
                    ww,
                    fw,
                );
                for (x, value) in result.iter().enumerate() {
                    out.data[y * row + x * bands + band] = *value;
                }
            }
        }
        out
    }

    /// `ImageFilter.MaxFilter(size)` band by band (the edges repeated, as Pillow's `expand`).
    pub fn max_filter(&self, size: usize) -> Picture {
        self.rank_filter(size, true)
    }

    /// `ImageFilter.MinFilter(size)`.
    pub fn min_filter(&self, size: usize) -> Picture {
        self.rank_filter(size, false)
    }

    /// The extreme over the square window, row by row then column by column: with the edges
    /// repeated, a window's extreme is the extreme of its part inside the picture.
    fn rank_filter(&self, size: usize, max: bool) -> Picture {
        let half = size / 2;
        let bands = self.bands();
        let (w, h) = (self.width, self.height);
        let mut line = vec![0u8; w.max(h)];
        let mut result = vec![0u8; w.max(h)];
        let mut rows = self.clone();
        for y in 0..h {
            for band in 0..bands {
                for (x, value) in line[..w].iter_mut().enumerate() {
                    *value = self.data[(y * w + x) * bands + band];
                }
                sliding_extreme(&line[..w], half, max, &mut result[..w]);
                for (x, value) in result[..w].iter().enumerate() {
                    rows.data[(y * w + x) * bands + band] = *value;
                }
            }
        }
        let mut out = rows.clone();
        for x in 0..w {
            for band in 0..bands {
                for (y, value) in line[..h].iter_mut().enumerate() {
                    *value = rows.data[(y * w + x) * bands + band];
                }
                sliding_extreme(&line[..h], half, max, &mut result[..h]);
                for (y, value) in result[..h].iter().enumerate() {
                    out.data[(y * w + x) * bands + band] = *value;
                }
            }
        }
        out
    }
}

/// The part of a `width` x `height` source placed at `x`, `y` that lands on `target`: (target x,
/// target y, source x, source y, width, height).
fn clip_region(
    target: &Picture,
    x: i64,
    y: i64,
    width: usize,
    height: usize,
) -> Option<(usize, usize, usize, usize, usize, usize)> {
    let (mut dx, mut dy, mut sx, mut sy) = (x, y, 0i64, 0i64);
    let (mut w, mut h) = (width as i64, height as i64);
    if dx < 0 {
        w += dx;
        sx = -dx;
        dx = 0;
    }
    if dx + w > target.width as i64 {
        w = target.width as i64 - dx;
    }
    if dy < 0 {
        h += dy;
        sy = -dy;
        dy = 0;
    }
    if dy + h > target.height as i64 {
        h = target.height as i64 - dy;
    }
    (w > 0 && h > 0).then_some((
        dx as usize,
        dy as usize,
        sx as usize,
        sy as usize,
        w as usize,
        h as usize,
    ))
}

/// A colour as an ink of `mode`: RGB colours on RGBA pictures get alpha 255.
fn ink(mode: Mode, color: &[u8]) -> Vec<u8> {
    let mut ink: Vec<u8> = (0..mode.bands())
        .map(|i| color.get(i).copied().unwrap_or(0))
        .collect();
    if mode == Mode::Rgba && color.len() == 3 {
        ink[3] = 255;
    }
    ink
}

fn luma(pixel: &[u8]) -> u8 {
    let (r, g, b) = (
        u32::from(pixel[0]),
        u32::from(pixel[1]),
        u32::from(pixel[2]),
    );
    ((r * 19595 + g * 38470 + b * 7471 + 0x8000) >> 16) as u8
}

fn shift_div255(a: u32) -> u32 {
    ((a >> 8) + a) >> 8
}

fn muldiv255(a: u8, b: u8) -> u8 {
    shift_div255(u32::from(a) * u32::from(b) + 128) as u8
}

/// Pillow's `BLEND(mask, in1, in2)`: `in2` over `in1` by `mask`.
fn blend(mask: u8, in1: u8, in2: u8) -> u8 {
    let (m, a, b) = (u32::from(mask), u32::from(in1), u32::from(in2));
    shift_div255(a * (255 - m) + b * m + 128) as u8
}

/// `fill_mask_L` for one pixel: `ink` over `pixel` by `m`, where a transparent RGBA pixel takes
/// the ink's colour fully.
fn fill_pixel(pixel: &mut [u8], ink: &[u8], m: u8) {
    let transparent = pixel.len() == 4 && pixel[3] == 0;
    for (band, (value, ink)) in pixel.iter_mut().zip(ink).enumerate() {
        let channel = if band != 3 && m != 0 && transparent {
            255
        } else {
            m
        };
        *value = blend(channel, *value, *ink);
    }
}

/// `rgba2rgbA`: undoes the premultiplication of an RGBa pixel.
fn unpremultiply(pixel: &mut [u8]) {
    let alpha = u32::from(pixel[3]);
    if alpha != 255 && alpha != 0 {
        for band in pixel.iter_mut().take(3) {
            *band = (255 * u32::from(*band) / alpha).min(255) as u8;
        }
    }
}

/// Python's `%` on floats: the sign of the divisor.
fn py_mod(a: f64, b: f64) -> f64 {
    let m = a % b;
    if m != 0.0 && (m < 0.0) != (b < 0.0) {
        m + b
    } else if m == 0.0 {
        0.0f64.copysign(b)
    } else {
        m
    }
}

/// Python's `round(value, 15)`.
fn round15(value: f64) -> f64 {
    format!("{value:.15}").parse().unwrap_or(value)
}

/// Geometry.c's `FLOOR`.
fn floor_int(v: f64) -> i32 {
    if v < 0.0 {
        v.floor() as i32
    } else {
        v as i32
    }
}

/// Geometry.c's `BICUBIC`.
fn cubic(v1: f64, v2: f64, v3: f64, v4: f64, d: f64) -> f64 {
    let p1 = v2;
    let p2 = -v1 + v3;
    let p3 = 2.0 * (v1 - v2) + v3 - v4;
    let p4 = -v1 + v2 - v3 + v4;
    p1 + d * (p2 + d * (p3 + d * p4))
}

// -- Resampling (Resample.c) ----------------------------------------------------------------------

const PRECISION_BITS: u32 = 32 - 8 - 2;

fn clip8(ss: i64) -> u8 {
    (ss >> PRECISION_BITS).clamp(0, 255) as u8
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let x = x * PI;
    x.sin() / x
}

fn lanczos(x: f64) -> f64 {
    if (-3.0..3.0).contains(&x) {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

fn bicubic_kernel(x: f64) -> f64 {
    const A: f64 = -0.5;
    let x = x.abs();
    if x < 1.0 {
        ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        (((x - 5.0) * x + 8.0) * x - 4.0) * A
    } else {
        0.0
    }
}

/// `precompute_coeffs` and `normalize_coeffs_8bpc`: for each output pixel, the first source
/// pixel and how many, and the fixed-point weights.
struct Coefficients {
    bounds: Vec<(usize, usize)>,
    weights: Vec<i32>,
    ksize: usize,
}

impl Coefficients {
    fn new(in_size: usize, out_size: usize, filter: Filter) -> Coefficients {
        let (support, kernel): (f64, fn(f64) -> f64) = match filter {
            Filter::Bicubic => (2.0, bicubic_kernel),
            Filter::Lanczos => (3.0, lanczos),
        };
        let scale = f64::from(in_size as f32) / out_size as f64;
        let filterscale = scale.max(1.0);
        let support = support * filterscale;
        let ksize = support.ceil() as usize * 2 + 1;
        let inv_filterscale = 1.0 / filterscale;
        let mut bounds = Vec::with_capacity(out_size);
        let mut weights = vec![0i32; out_size * ksize];
        for xx in 0..out_size {
            let center = (xx as f64 + 0.5) * scale;
            let xmin = ((center - support + 0.5) as i64).max(0);
            let xmax = ((center + support + 0.5) as i64).min(in_size as i64) - xmin;
            let count = xmax.max(0) as usize;
            let mut k: Vec<f64> = (0..count)
                .map(|x| kernel(((x as i64 + xmin) as f64 - center + 0.5) * inv_filterscale))
                .collect();
            let ww: f64 = k.iter().sum();
            if ww != 0.0 {
                for w in &mut k {
                    *w /= ww;
                }
            }
            for (x, w) in k.iter().enumerate() {
                let fixed = w * f64::from(1u32 << PRECISION_BITS);
                weights[xx * ksize + x] = if *w < 0.0 {
                    (-0.5 + fixed) as i32
                } else {
                    (0.5 + fixed) as i32
                };
            }
            bounds.push((xmin as usize, count));
        }
        Coefficients {
            bounds,
            weights,
            ksize,
        }
    }

    fn row(&self, out: usize) -> &[i32] {
        &self.weights[out * self.ksize..(out + 1) * self.ksize]
    }

    /// The source pixels outputs `from..to` read.
    fn span(&self, from: usize, to: usize) -> (usize, usize) {
        let first = self.bounds[from..to].iter().map(|b| b.0).min().unwrap_or(0);
        let last = self.bounds[from..to]
            .iter()
            .map(|b| b.0 + b.1)
            .max()
            .unwrap_or(first);
        (first, last.max(first))
    }
}

/// A source of the resampling: `picture` at `left`, `top` of a `width` x `height` picture that is
/// zero elsewhere.
struct Canvas<'a> {
    picture: &'a Picture,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

impl<'a> Canvas<'a> {
    fn whole(picture: &'a Picture) -> Canvas<'a> {
        Canvas {
            picture,
            left: 0,
            top: 0,
            width: picture.width,
            height: picture.height,
        }
    }

    fn is_whole(&self) -> bool {
        self.left == 0
            && self.top == 0
            && (self.picture.width, self.picture.height) == (self.width, self.height)
    }

    /// Columns `cols0..cols1` of row `y`, premultiplied when RGBA (`RGBa`), into `line`.
    fn line(&self, y: usize, cols0: usize, cols1: usize, line: &mut [i64]) {
        let picture = self.picture;
        let bands = picture.bands();
        line.fill(0);
        if y < self.top || y >= self.top + picture.height {
            return;
        }
        let (from, to) = (cols0.max(self.left), cols1.min(self.left + picture.width));
        if from >= to {
            return;
        }
        let start = picture.at(from - self.left, y - self.top);
        let row = &picture.data[start..start + (to - from) * bands];
        let base = (from - cols0) * bands;
        if picture.mode == Mode::Rgba {
            for (i, pixel) in row.chunks_exact(4).enumerate() {
                let at = base + i * 4;
                line[at] = i64::from(muldiv255(pixel[0], pixel[3]));
                line[at + 1] = i64::from(muldiv255(pixel[1], pixel[3]));
                line[at + 2] = i64::from(muldiv255(pixel[2], pixel[3]));
                line[at + 3] = i64::from(pixel[3]);
            }
        } else {
            for (target, value) in line[base..base + row.len()].iter_mut().zip(row) {
                *target = i64::from(*value);
            }
        }
    }

    /// Whether row `y`, columns `cols0..cols1`, holds the same pixels in both whole pictures.
    fn same_row(&self, other: &Canvas, y: usize, cols0: usize, cols1: usize) -> bool {
        if !self.is_whole() || !other.is_whole() || cols0 >= cols1 {
            return false;
        }
        let (a, b) = (self.picture, other.picture);
        if (a.width, a.height, a.mode) != (b.width, b.height, b.mode) {
            return false;
        }
        let (start, end) = (a.at(cols0, y), a.at(cols1 - 1, y) + a.bands());
        a.data[start..end] == b.data[start..end]
    }
}

/// `ImagingResampleInner` (horizontal pass, then vertical) of each source, over `window` of the
/// `width` x `height` output; the outputs outside it stay zero.
fn resample(
    sources: &[Canvas],
    width: usize,
    height: usize,
    filter: Filter,
    window: Window,
) -> Vec<Picture> {
    let Some(first) = sources.first() else {
        return Vec::new();
    };
    let mode = first.picture.mode;
    let bands = mode.bands();
    let mut outs: Vec<Picture> = sources
        .iter()
        .map(|_| Picture::new(mode, width, height, &[]))
        .collect();
    let window = Window {
        x0: window.x0.min(width),
        y0: window.y0.min(height),
        x1: window.x1.min(width),
        y1: window.y1.min(height),
    };
    let (in_width, in_height) = (first.width, first.height);
    if window.x0 >= window.x1 || window.y0 >= window.y1 || in_width == 0 || in_height == 0 {
        return outs;
    }
    let vertical = Coefficients::new(in_height, height, filter);
    let horizontal = Coefficients::new(in_width, width, filter);
    let need_horizontal = width != in_width;
    let need_vertical = height != in_height;
    let (rows0, rows1) = if need_vertical {
        vertical.span(window.y0, window.y1)
    } else {
        (window.y0, window.y1)
    };
    let (cols0, cols1) = if need_horizontal {
        horizontal.span(window.x0, window.x1)
    } else {
        (window.x0, window.x1)
    };
    // Horizontal pass over the rows the vertical pass needs, the window's columns only.
    let row_len = (window.x1 - window.x0) * bands;
    let mut temps: Vec<Vec<u8>> = sources
        .iter()
        .map(|_| vec![0u8; (rows1 - rows0) * row_len])
        .collect();
    let mut line = vec![0i64; (cols1 - cols0) * bands];
    for y in rows0..rows1 {
        let target = (y - rows0) * row_len;
        for (s, source) in sources.iter().enumerate() {
            if s > 0 && source.same_row(first, y, cols0, cols1) {
                let (done, rest) = temps.split_at_mut(s);
                rest[0][target..target + row_len]
                    .copy_from_slice(&done[0][target..target + row_len]);
                continue;
            }
            source.line(y, cols0, cols1, &mut line);
            let out = &mut temps[s][target..target + row_len];
            for xx in window.x0..window.x1 {
                let at = (xx - window.x0) * bands;
                if need_horizontal {
                    let (xmin, count) = horizontal.bounds[xx];
                    let k = horizontal.row(xx);
                    let mut ss = [1i64 << (PRECISION_BITS - 1); 4];
                    let mut p = (xmin - cols0) * bands;
                    for weight in &k[..count] {
                        let weight = i64::from(*weight);
                        for band in 0..bands {
                            ss[band] += line[p + band] * weight;
                        }
                        p += bands;
                    }
                    for band in 0..bands {
                        out[at + band] = clip8(ss[band]);
                    }
                } else {
                    let p = (xx - cols0) * bands;
                    for band in 0..bands {
                        out[at + band] = line[p + band] as u8;
                    }
                }
            }
        }
    }
    // Vertical pass, a row of the window at a time.
    let mut acc = vec![0i64; row_len];
    for (out, temp) in outs.iter_mut().zip(&temps) {
        for yy in window.y0..window.y1 {
            let start = out.at(window.x0, yy);
            if need_vertical {
                let (ymin, count) = vertical.bounds[yy];
                let k = vertical.row(yy);
                acc.fill(1i64 << (PRECISION_BITS - 1));
                for (t, weight) in k[..count].iter().enumerate() {
                    let weight = i64::from(*weight);
                    let row = &temp[(ymin + t - rows0) * row_len..(ymin + t - rows0 + 1) * row_len];
                    for (sum, value) in acc.iter_mut().zip(row) {
                        *sum += i64::from(*value) * weight;
                    }
                }
                for (target, sum) in out.data[start..start + row_len].iter_mut().zip(&acc) {
                    *target = clip8(*sum);
                }
            } else {
                let row = &temp[(yy - rows0) * row_len..(yy - rows0 + 1) * row_len];
                out.data[start..start + row_len].copy_from_slice(row);
            }
            if mode == Mode::Rgba {
                for pixel in out.data[start..start + row_len].chunks_exact_mut(4) {
                    unpremultiply(pixel);
                }
            }
        }
    }
    outs
}

// -- Box blur (BoxBlur.c) -------------------------------------------------------------------------

/// `_gaussian_blur_radius`: the box radius of `passes` box blurs that make a Gaussian of `radius`.
fn gaussian_box_radius(radius: f32, passes: i32) -> f32 {
    let sigma2: f32 = radius * radius / passes as f32;
    let length = (12.0 * f64::from(sigma2) + 1.0).sqrt() as f32;
    let l = ((f64::from(length) - 1.0) / 2.0).floor() as f32;
    let mut a: f32 = (2.0 * l + 1.0) * (l * (l + 1.0) - 3.0 * sigma2);
    a /= 6.0 * (sigma2 - (l + 1.0) * (l + 1.0));
    l + a
}

/// `ImagingLineBoxBlur8`/`32` for one band of a row (`line`, its values in order): the C's three
/// loops, each pixel taking the indices of the loop it falls in.
#[allow(clippy::too_many_arguments)]
fn box_blur_line(
    out: &mut [u8],
    line: &[u32],
    lastx: i32,
    radius: i32,
    edge_a: i32,
    edge_b: i32,
    ww: u32,
    fw: u32,
) {
    let px = |x: i32| line[x as usize];
    let mut acc: u32 = px(0).wrapping_mul((radius + 1) as u32);
    for x in 0..edge_a - 1 {
        acc = acc.wrapping_add(px(x));
    }
    acc = acc.wrapping_add(px(lastx).wrapping_mul((radius - edge_a + 1) as u32));
    for x in 0..=lastx {
        let (subtract, add, left, right) = if edge_a <= edge_b {
            if x < edge_a {
                (0, x + radius, 0, x + radius + 1)
            } else if x < edge_b {
                (x - radius - 1, x + radius, x - radius - 1, x + radius + 1)
            } else {
                (x - radius - 1, lastx, x - radius - 1, lastx)
            }
        } else if x < edge_b {
            (0, x + radius, 0, x + radius + 1)
        } else if x < edge_a {
            (0, lastx, 0, lastx)
        } else {
            (x - radius - 1, lastx, x - radius - 1, lastx)
        };
        acc = acc
            .wrapping_add(line[add as usize])
            .wrapping_sub(line[subtract as usize]);
        let bulk = acc
            .wrapping_mul(ww)
            .wrapping_add((line[left as usize] + line[right as usize]).wrapping_mul(fw));
        out[x as usize] = (bulk.wrapping_add(1 << 23) >> 24) as u8;
    }
}

// -- ImageChops -------------------------------------------------------------------------------------

fn chop(a: &Picture, b: &Picture, operation: impl Fn(u8, u8) -> i32) -> Picture {
    Picture {
        data: a
            .data
            .iter()
            .zip(&b.data)
            .map(|(x, y)| operation(*x, *y).clamp(0, 255) as u8)
            .collect(),
        ..a.clone()
    }
}

/// `ImageChops.lighter`.
pub fn lighter(a: &Picture, b: &Picture) -> Picture {
    chop(a, b, |x, y| i32::from(x.max(y)))
}

/// `ImageChops.multiply`.
pub fn multiply(a: &Picture, b: &Picture) -> Picture {
    chop(a, b, |x, y| i32::from(x) * i32::from(y) / 255)
}

/// `ImageChops.difference`.
pub fn difference(a: &Picture, b: &Picture) -> Picture {
    chop(a, b, |x, y| (i32::from(x) - i32::from(y)).abs())
}

/// `ImageChops.add(a, b, scale, offset)`.
pub fn add(a: &Picture, b: &Picture, scale: f32, offset: i32) -> Picture {
    chop(a, b, |x, y| {
        ((i32::from(x) + i32::from(y)) as f32 / scale + offset as f32) as i32
    })
}

/// `ImageChops.subtract(a, b, scale, offset)`.
pub fn subtract(a: &Picture, b: &Picture, scale: f32, offset: i32) -> Picture {
    chop(a, b, |x, y| {
        ((i32::from(x) - i32::from(y)) as f32 / scale + offset as f32) as i32
    })
}

/// `ImageChops.invert`.
pub fn invert(a: &Picture) -> Picture {
    a.point(|v| 255 - v)
}

/// `Image.composite(a, b, mask)`: `a` over `b` through `mask`.
pub fn composite(a: &Picture, b: &Picture, mask: &Picture) -> Picture {
    let mut out = b.clone();
    out.paste(a, 0, 0, Some(mask));
    out
}

/// `Image.linear_gradient('L')`: 256 x 256, black at the top to white at the bottom.
pub fn linear_gradient() -> Picture {
    Picture {
        width: 256,
        height: 256,
        mode: Mode::L,
        data: (0..=255u8).flat_map(|y| [y; 256]).collect(),
    }
}

/// `ImageDraw.floodfill(image, seed, value)` with no border and no threshold: the region of the
/// seed's value (4-connected) painted `value`, nothing when the seed already has it.
pub fn floodfill(image: &mut Picture, seed: (usize, usize), value: u8) {
    let (width, height) = (image.width, image.height);
    if seed.0 >= width || seed.1 >= height {
        return;
    }
    let background = image.data[seed.1 * width + seed.0];
    if background == value {
        return;
    }
    let mut queue = VecDeque::from([seed]);
    image.data[seed.1 * width + seed.0] = value;
    while let Some((x, y)) = queue.pop_front() {
        let neighbours = [
            (x + 1 < width).then(|| (x + 1, y)),
            x.checked_sub(1).map(|x| (x, y)),
            (y + 1 < height).then(|| (x, y + 1)),
            y.checked_sub(1).map(|y| (x, y)),
        ];
        for (s, t) in neighbours.into_iter().flatten() {
            if image.data[t * width + s] == background {
                image.data[t * width + s] = value;
                queue.push_back((s, t));
            }
        }
    }
}

// -- ImageDraw (Draw.c) ---------------------------------------------------------------------------

/// `ImageDraw` on L and RGBA pictures: the ink replaces the pixels (no blending).
pub struct Draw<'a> {
    image: &'a mut Picture,
}

/// Draw.c's `Edge`.
#[derive(Clone, Copy)]
struct Edge {
    x0: i32,
    y0: i32,
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
    dx: f32,
}

impl Edge {
    fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Edge {
        Edge {
            x0,
            y0,
            xmin: x0.min(x1),
            xmax: x0.max(x1),
            ymin: y0.min(y1),
            ymax: y0.max(y1),
            dx: if y0 == y1 {
                0.0
            } else {
                (x1 - x0) as f32 / (y1 - y0) as f32
            },
        }
    }

    fn x_at(&self, y: i32) -> f32 {
        (y - self.y0) as f32 * self.dx + self.x0 as f32
    }
}

fn round_up32(f: f32) -> i32 {
    if f >= 0.0 {
        (f + 0.5).floor() as i32
    } else {
        -(f.abs() + 0.5).floor() as i32
    }
}

fn round_down32(f: f32) -> i32 {
    if f >= 0.0 {
        (f - 0.5).ceil() as i32
    } else {
        -(f.abs() - 0.5).ceil() as i32
    }
}

fn round_up64(f: f64) -> i32 {
    if f >= 0.0 {
        (f + 0.5).floor() as i32
    } else {
        -(f.abs() + 0.5).floor() as i32
    }
}

fn round_down64(f: f64) -> i32 {
    if f >= 0.0 {
        (f - 0.5).ceil() as i32
    } else {
        -(f.abs() - 0.5).ceil() as i32
    }
}

/// Python's `round` of a float to an integer: halves to even.
fn py_round(v: f64) -> i32 {
    v.round_ties_even() as i32
}

/// `ImagingDrawRectangle`'s and the shapes' rounding of Python coordinates: C's `(int)`.
fn trunc(v: f64) -> i32 {
    v as i32
}

impl<'a> Draw<'a> {
    pub fn new(image: &'a mut Picture) -> Draw<'a> {
        Draw { image }
    }

    fn ink(&self, color: &[u8]) -> Vec<u8> {
        ink(self.image.mode, color)
    }

    fn point(&mut self, x: i32, y: i32, ink: &[u8]) {
        if x >= 0 && y >= 0 && (x as usize) < self.image.width && (y as usize) < self.image.height {
            let at = self.image.at(x as usize, y as usize);
            self.image.data[at..at + ink.len()].copy_from_slice(ink);
        }
    }

    fn hline(&mut self, x0: i32, y: i32, x1: i32, ink: &[u8]) {
        let (width, height) = (self.image.width as i32, self.image.height as i32);
        if y < 0 || y >= height {
            return;
        }
        let x0 = if x0 < 0 {
            0
        } else if x0 >= width {
            return;
        } else {
            x0
        };
        let x1 = if x1 < 0 {
            return;
        } else if x1 >= width {
            width - 1
        } else {
            x1
        };
        for x in x0..=x1 {
            self.point(x, y, ink);
        }
    }

    /// Draw.c's Bresenham `line` (the last point left out).
    fn line1(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, ink: &[u8]) {
        let (mut x0, mut y0) = (x0, y0);
        let (mut dx, xs) = if x1 - x0 < 0 {
            (x0 - x1, -1)
        } else {
            (x1 - x0, 1)
        };
        let (mut dy, ys) = if y1 - y0 < 0 {
            (y0 - y1, -1)
        } else {
            (y1 - y0, 1)
        };
        if dx == 0 {
            for _ in 0..dy {
                self.point(x0, y0, ink);
                y0 += ys;
            }
        } else if dy == 0 {
            for _ in 0..dx {
                self.point(x0, y0, ink);
                x0 += xs;
            }
        } else if dx > dy {
            let n = dx;
            dy += dy;
            let mut e = dy - dx;
            dx += dx;
            for _ in 0..n {
                self.point(x0, y0, ink);
                if e >= 0 {
                    y0 += ys;
                    e -= dx;
                }
                e += dy;
                x0 += xs;
            }
        } else {
            let n = dy;
            dx += dx;
            let mut e = dx - dy;
            dy += dy;
            for _ in 0..n {
                self.point(x0, y0, ink);
                if e >= 0 {
                    x0 += xs;
                    e -= dy;
                }
                e += dx;
                y0 += ys;
            }
        }
    }

    /// `polygon_generic` without alpha blending.
    fn polygon_edges(&mut self, edges: &[Edge], ink: &[u8]) {
        if edges.is_empty() {
            return;
        }
        let mut ymin = self.image.height as i32 - 1;
        let mut ymax = 0;
        let mut table = Vec::new();
        for e in edges {
            ymin = ymin.min(e.ymin);
            ymax = ymax.max(e.ymax);
            if e.ymin == e.ymax {
                self.hline(e.xmin, e.ymin, e.xmax, ink);
                continue;
            }
            table.push(*e);
        }
        let ymin = ymin.max(0);
        let ymax = ymax.min(self.image.height as i32);
        let mut xx: Vec<f32> = Vec::with_capacity(table.len() * 2);
        for y in ymin..=ymax {
            xx.clear();
            for (i, current) in table.iter().enumerate() {
                if y < current.ymin || y > current.ymax {
                    continue;
                }
                xx.push(current.x_at(y));
                if y == current.ymax && y < ymax {
                    let last = xx[xx.len() - 1];
                    xx.push(last);
                } else if (y == current.ymin || y == current.ymax) && current.dx != 0.0 {
                    for other in &table[..i] {
                        if (y != other.ymin && y != other.ymax) || other.dx == 0.0 {
                            continue;
                        }
                        let last = xx.len() - 1;
                        if xx[last].round() == other.x_at(y).round() {
                            let offset = if y == current.ymax { -1 } else { 1 };
                            let adjacent = current.x_at(y + offset);
                            if y + offset >= other.ymin && y + offset <= other.ymax {
                                let adjacent_other = other.x_at(y + offset);
                                if xx[last] > adjacent + 1.0 && xx[last] > adjacent_other + 1.0 {
                                    xx[last] = (f64::from(adjacent).max(f64::from(adjacent_other))
                                        as f32)
                                        .round()
                                        + 1.0;
                                } else if xx[last] < adjacent - 1.0
                                    && xx[last] < adjacent_other - 1.0
                                {
                                    xx[last] = (f64::from(adjacent).min(f64::from(adjacent_other))
                                        as f32)
                                        .round()
                                        - 1.0;
                                }
                                break;
                            }
                        }
                    }
                }
            }
            xx.sort_by(|a, b| a.total_cmp(b));
            for pair in xx.chunks_exact(2) {
                self.hline(round_up32(pair[0]), y, round_down32(pair[1]), ink);
            }
        }
    }

    /// `ImageDraw.polygon(points, fill=color)`.
    pub fn polygon(&mut self, points: &[(f64, f64)], fill: &[u8]) {
        let ink = self.ink(fill);
        let xy: Vec<(i32, i32)> = points.iter().map(|(x, y)| (trunc(*x), trunc(*y))).collect();
        if xy.len() < 2 {
            return;
        }
        let mut edges: Vec<Edge> = Vec::new();
        for i in 0..xy.len() - 1 {
            let ((x0, y0), (x1, y1)) = (xy[i], xy[i + 1]);
            if y0 == y1 && i != 0 && y0 == xy[i - 1].1 {
                if let Some(last) = edges.last_mut() {
                    if x1 > x0 && x0 > xy[i - 1].0 {
                        last.xmax = x1;
                        continue;
                    } else if x1 < x0 && x0 < xy[i - 1].0 {
                        last.xmin = x1;
                        continue;
                    }
                }
            }
            edges.push(Edge::new(x0, y0, x1, y1));
        }
        let (first, end) = (xy[0], xy[xy.len() - 1]);
        if end != first {
            edges.push(Edge::new(end.0, end.1, first.0, first.1));
        }
        self.polygon_edges(&edges, &ink);
    }

    /// `ImagingDrawWideLine`.
    fn wide_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, ink: &[u8], width: i32) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        if dx == 0 && dy == 0 {
            self.point(x0, y0, ink);
            return;
        }
        let big = f64::from(dx).hypot(f64::from(dy));
        let small = f64::from(width - 1) / 2.0;
        let ratio_max = f64::from(round_up64(small)) / big;
        let ratio_min = f64::from(round_down64(small)) / big;
        let dxmin = round_down64(ratio_min * f64::from(dy));
        let dxmax = round_down64(ratio_max * f64::from(dy));
        let dymin = round_down64(ratio_min * f64::from(dx));
        let dymax = round_down64(ratio_max * f64::from(dx));
        let v = [
            (x0 - dxmin, y0 + dymax),
            (x1 - dxmin, y1 + dymax),
            (x1 + dxmax, y1 - dymin),
            (x0 + dxmax, y0 - dymin),
        ];
        let edges = [
            Edge::new(v[0].0, v[0].1, v[1].0, v[1].1),
            Edge::new(v[1].0, v[1].1, v[2].0, v[2].1),
            Edge::new(v[2].0, v[2].1, v[3].0, v[3].1),
            Edge::new(v[3].0, v[3].1, v[0].0, v[0].1),
        ];
        self.polygon_edges(&edges, ink);
    }

    /// `ImageDraw.line(points, fill, width, joint)`; `curve` rounds the joints as Pillow does.
    pub fn line(&mut self, points: &[(f64, f64)], fill: &[u8], width: i32, curve: bool) {
        let ink = self.ink(fill);
        if width == 0 {
            return;
        }
        for pair in points.windows(2) {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            if width == 1 {
                self.line1(trunc(x0), trunc(y0), trunc(x1), trunc(y1), &ink);
            } else {
                self.wide_line(trunc(x0), trunc(y0), trunc(x1), trunc(y1), &ink, width);
            }
        }
        if width == 1 {
            if let Some((x, y)) = points.last().filter(|_| points.len() > 1) {
                self.point(trunc(*x), trunc(*y), &ink);
            }
        }
        if !curve || width <= 4 {
            return;
        }
        let w = f64::from(width);
        for i in 1..points.len().saturating_sub(1) {
            let point = points[i];
            let angle = |start: (f64, f64), end: (f64, f64)| {
                py_mod((end.0 - start.0).atan2(start.1 - end.1).to_degrees(), 360.0)
            };
            let angles = [angle(points[i - 1], point), angle(point, points[i + 1])];
            if angles[0] == angles[1] {
                continue;
            }
            let coord_at_angle = |coord: (f64, f64), angle: f64| {
                let angle = (angle - 90.0).to_radians();
                let distance = w / 2.0 - 1.0;
                let step = |p_d: f64| if p_d > 0.0 { p_d.floor() } else { p_d.ceil() };
                (
                    coord.0 + step(distance * angle.cos()),
                    coord.1 + step(distance * angle.sin()),
                )
            };
            let flipped = (angles[1] > angles[0] && angles[1] - 180.0 > angles[0])
                || (angles[1] < angles[0] && angles[1] + 180.0 > angles[0]);
            let coords = [
                point.0 - w / 2.0 + 1.0,
                point.1 - w / 2.0 + 1.0,
                point.0 + w / 2.0 - 1.0,
                point.1 + w / 2.0 - 1.0,
            ];
            let (start, end) = if flipped {
                (angles[1] + 90.0, angles[0] + 90.0)
            } else {
                (angles[0] - 90.0, angles[1] - 90.0)
            };
            self.pieslice(coords, start - 90.0, end - 90.0, fill);
            if width > 8 {
                let gap = if flipped {
                    [
                        coord_at_angle(point, angles[0] + 90.0),
                        point,
                        coord_at_angle(point, angles[1] + 90.0),
                    ]
                } else {
                    [
                        coord_at_angle(point, angles[0] - 90.0),
                        point,
                        coord_at_angle(point, angles[1] - 90.0),
                    ]
                };
                self.line(&gap, fill, 3, false);
            }
        }
    }

    /// `ImageDraw.rectangle(box, fill=, outline=, width=)`.
    pub fn rectangle(
        &mut self,
        area: [f64; 4],
        fill: Option<&[u8]>,
        outline: Option<&[u8]>,
        width: i32,
    ) {
        let xy = area.map(trunc);
        if let Some(fill) = fill {
            let ink = self.ink(fill);
            self.rectangle_ints(xy, &ink, true, 0);
        }
        if let Some(outline) = outline.filter(|o| Some(*o) != fill && width != 0) {
            let ink = self.ink(outline);
            self.rectangle_ints(xy, &ink, false, width);
        }
    }

    /// `ImagingDrawRectangle`.
    fn rectangle_ints(&mut self, xy: [i32; 4], ink: &[u8], fill: bool, width: i32) {
        let [x0, mut y0, x1, mut y1] = xy;
        if y0 > y1 {
            std::mem::swap(&mut y0, &mut y1);
        }
        let height = self.image.height as i32;
        if fill {
            if y0 < 0 {
                y0 = 0;
            } else if y0 >= height {
                return;
            }
            if y1 < 0 {
                return;
            } else if y1 > height {
                y1 = height;
            }
            for y in y0..=y1 {
                self.hline(x0, y, x1, ink);
            }
        } else {
            let width = width.max(1);
            for i in 0..width {
                self.hline(x0, y0 + i, x1, ink);
                self.hline(x0, y1 - i, x1, ink);
                self.line1(x1 - i, y0 + width, x1 - i, y1 - width + 1, ink);
                self.line1(x0 + i, y0 + width, x0 + i, y1 - width + 1, ink);
            }
        }
    }

    /// `ImageDraw.ellipse(box, fill=, outline=, width=)`.
    pub fn ellipse(
        &mut self,
        area: [f64; 4],
        fill: Option<&[u8]>,
        outline: Option<&[u8]>,
        width: i32,
    ) {
        let [x0, y0, x1, y1] = area.map(trunc);
        if let Some(fill) = fill {
            let ink = self.ink(fill);
            self.ellipse_ints(x0, y0, x1, y1, &ink, true, 0);
        }
        if let Some(outline) = outline.filter(|o| Some(*o) != fill && width != 0) {
            let ink = self.ink(outline);
            self.ellipse_ints(x0, y0, x1, y1, &ink, false, width);
        }
    }

    /// `ellipseNew`.
    #[allow(clippy::too_many_arguments)]
    fn ellipse_ints(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        ink: &[u8],
        fill: bool,
        width: i32,
    ) {
        let (a, b) = (x1 - x0, y1 - y0);
        if a < 0 || b < 0 {
            return;
        }
        let width = if fill { a + b } else { width };
        let mut state = EllipseState::new(a, b, width);
        while let Some((sx0, sy, sx1)) = state.next() {
            self.hline(
                x0 + (sx0 + a) / 2,
                y0 + (sy + b) / 2,
                x0 + (sx1 + a) / 2,
                ink,
            );
        }
    }

    /// `clipEllipseNew` with the clipping `root`.
    #[allow(clippy::too_many_arguments)]
    fn clip_ellipse(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        width: i32,
        root: Option<Clip>,
        ink: &[u8],
    ) {
        let (a, b) = (x1 - x0, y1 - y0);
        if a < 0 || b < 0 {
            return;
        }
        let mut state = EllipseState::new(a, b, width);
        let mut head: VecDeque<i32> = VecDeque::new();
        let mut row = 0;
        loop {
            while head.is_empty() {
                let Some((sx0, sy, sx1)) = state.next() else {
                    return;
                };
                head = do_clip(root.as_ref(), sx0, sy, sx1)
                    .into_iter()
                    .map(|e| e.0)
                    .collect();
                row = sy;
            }
            let (Some(sx0), Some(sx1)) = (head.pop_front(), head.pop_front()) else {
                return;
            };
            self.hline(
                x0 + (sx0 + a) / 2,
                y0 + (row + b) / 2,
                x0 + (sx1 + a) / 2,
                ink,
            );
        }
    }

    /// `ImageDraw.pieslice(box, start, end, fill=color)`.
    pub fn pieslice(&mut self, area: [f64; 4], start: f64, end: f64, fill: &[u8]) {
        let ink = self.ink(fill);
        let [x0, y0, x1, y1] = area.map(trunc);
        let (start, end) = normalize_angles(start as f32, end as f32);
        if start + 360.0 == end {
            self.ellipse_ints(x0, y0, x1, y1, &ink, true, 0);
            return;
        }
        if start == end {
            return;
        }
        let (a, b) = (x1 - x0, y1 - y0);
        if a < 0 || b < 0 {
            return;
        }
        let root = pie_clip(a, b, start, end);
        self.clip_ellipse(x0, y0, x1, y1, x1 + y1 - x0 - y0, Some(root), &ink);
    }

    /// `ImageDraw.arc(box, start, end, fill=color, width=)`.
    pub fn arc(&mut self, area: [f64; 4], start: f64, end: f64, fill: &[u8], width: i32) {
        if width == 0 {
            return;
        }
        let ink = self.ink(fill);
        let [x0, y0, x1, y1] = area.map(trunc);
        let (start, end) = normalize_angles(start as f32, end as f32);
        if start + 360.0 == end {
            self.ellipse_ints(x0, y0, x1, y1, &ink, false, width);
            return;
        }
        if start == end {
            return;
        }
        let (a, b) = (x1 - x0, y1 - y0);
        if a < 0 || b < 0 {
            return;
        }
        let root = arc_clip(a, b, start, end);
        self.clip_ellipse(x0, y0, x1, y1, width, root, &ink);
    }

    /// `ImageDraw.rounded_rectangle(box, radius, fill=, outline=, width=)`, the Python method.
    pub fn rounded_rectangle(
        &mut self,
        area: [f64; 4],
        radius: f64,
        fill: Option<&[u8]>,
        outline: Option<&[u8]>,
        width: i32,
    ) {
        let [fx0, fy0, fx1, fy1] = area;
        let mut d = (fx1 - fx0).min(fy1 - fy0).min(radius * 2.0);
        let (x0, y0, x1, y1) = (py_round(fx0), py_round(fy0), py_round(fx1), py_round(fy1));
        let full_x = d >= f64::from(x1 - x0 - 1);
        if full_x {
            d = f64::from(x1 - x0);
        }
        let full_y = d >= f64::from(y1 - y0 - 1);
        if full_y {
            d = f64::from(y1 - y0);
        }
        if full_x && full_y {
            self.ellipse(area, fill, outline, width);
            return;
        }
        if d == 0.0 {
            self.rectangle(area, fill, outline, width);
            return;
        }
        let r = (d / 2.0).floor() as i32;
        let (fx0, fy0, fx1, fy1) = (f64::from(x0), f64::from(y0), f64::from(x1), f64::from(y1));
        let parts: Vec<([f64; 4], f64, f64)> = if full_x {
            vec![
                ([fx0, fy0, fx0 + d, fy0 + d], 180.0, 360.0),
                ([fx0, fy1 - d, fx0 + d, fy1], 0.0, 180.0),
            ]
        } else if full_y {
            vec![
                ([fx0, fy0, fx0 + d, fy0 + d], 90.0, 270.0),
                ([fx1 - d, fy0, fx1, fy0 + d], 270.0, 90.0),
            ]
        } else {
            vec![
                ([fx0, fy0, fx0 + d, fy0 + d], 180.0, 270.0),
                ([fx1 - d, fy0, fx1, fy0 + d], 270.0, 360.0),
                ([fx1 - d, fy1 - d, fx1, fy1], 0.0, 90.0),
                ([fx0, fy1 - d, fx0 + d, fy1], 90.0, 180.0),
            ]
        };
        if let Some(fill) = fill {
            let ink = self.ink(fill);
            for (part, start, end) in &parts {
                self.pieslice(*part, *start, *end, fill);
            }
            if full_x {
                self.rectangle_ints([x0, y0 + r + 1, x1, y1 - r - 1], &ink, true, 0);
            } else if x1 - r > x0 + r + 1 {
                self.rectangle_ints([x0 + r + 1, y0, x1 - r - 1, y1], &ink, true, 0);
            }
            if !full_x && !full_y {
                self.rectangle_ints([x0, y0 + r + 1, x0 + r, y1 - r - 1], &ink, true, 0);
                self.rectangle_ints([x1 - r, y0 + r + 1, x1, y1 - r - 1], &ink, true, 0);
            }
        }
        if let Some(outline) = outline.filter(|o| Some(*o) != fill && width != 0) {
            let ink = self.ink(outline);
            for (part, start, end) in &parts {
                self.arc(*part, *start, *end, outline, width);
            }
            if !full_x {
                self.rectangle_ints([x0 + r + 1, y0, x1 - r - 1, y0 + width - 1], &ink, true, 0);
                self.rectangle_ints([x0 + r + 1, y1 - width + 1, x1 - r - 1, y1], &ink, true, 0);
            }
            if !full_y {
                self.rectangle_ints([x0, y0 + r + 1, x0 + width - 1, y1 - r - 1], &ink, true, 0);
                self.rectangle_ints([x1 - width + 1, y0 + r + 1, x1, y1 - r - 1], &ink, true, 0);
            }
        }
    }

    /// `ImageDraw.text(xy, text, fill, font, stroke_width=stroke, stroke_fill=fill)` with the
    /// default anchor (left, ascender).
    pub fn text(
        &mut self,
        xy: (f64, f64),
        text: &str,
        font: &FontSize<'_>,
        fill: &[u8],
        stroke: i64,
    ) {
        let ink = self.ink(fill);
        let Some(mask) = font.render(text, stroke, xy) else {
            return;
        };
        let (x, y) = (xy.0 as i64 + mask.x, xy.1 as i64 + mask.y);
        let target = &mut *self.image;
        let bands = target.bands();
        let Some((dx, dy, sx, sy, width, height)) =
            clip_region(target, x, y, mask.picture.width, mask.picture.height)
        else {
            return;
        };
        for row in 0..height {
            for column in 0..width {
                let m = mask.picture.data[mask.picture.at(sx + column, sy + row)];
                let out = target.at(dx + column, dy + row);
                fill_pixel(&mut target.data[out..out + bands], &ink, m);
            }
        }
    }
}

/// `normalize_angles`: 0 <= start < 360, start <= end <= start + 360.
fn normalize_angles(al: f32, ar: f32) -> (f32, f32) {
    if ar - al >= 360.0 {
        return (0.0, 360.0);
    }
    let al = (if al < 0.0 {
        360.0 - f64::from(-al) % 360.0
    } else {
        f64::from(al)
    } % 360.0) as f32;
    let span = if ar < al {
        360.0 - f64::from(al - ar) % 360.0
    } else {
        f64::from(ar - al)
    };
    let ar = (f64::from(al) + span % 360.0) as f32;
    (al, ar)
}

/// Draw.c's `quarter_state`: a quarter of an ellipse on the grid of step 2.
struct Quarter {
    cx: i32,
    cy: i32,
    ex: i32,
    ey: i32,
    a2: i64,
    b2: i64,
    a2b2: i64,
    finished: bool,
}

impl Quarter {
    fn new(a: i32, b: i32) -> Quarter {
        if a < 0 || b < 0 {
            return Quarter {
                cx: 0,
                cy: 0,
                ex: 0,
                ey: 0,
                a2: 0,
                b2: 0,
                a2b2: 0,
                finished: true,
            };
        }
        let (a2, b2) = (i64::from(a) * i64::from(a), i64::from(b) * i64::from(b));
        Quarter {
            cx: a,
            cy: b % 2,
            ex: a % 2,
            ey: b,
            a2,
            b2,
            a2b2: a2 * b2,
            finished: false,
        }
    }

    fn delta(&self, x: i32, y: i32) -> i64 {
        let (x, y) = (i64::from(x), i64::from(y));
        (self.a2 * y * y + self.b2 * x * x - self.a2b2).abs()
    }

    fn next(&mut self) -> Option<(i32, i32)> {
        if self.finished {
            return None;
        }
        let point = (self.cx, self.cy);
        if self.cx == self.ex && self.cy == self.ey {
            self.finished = true;
        } else {
            let (mut nx, mut ny) = (self.cx, self.cy + 2);
            let mut ndelta = self.delta(nx, ny);
            if nx > 1 {
                let newdelta = self.delta(self.cx - 2, self.cy + 2);
                if ndelta > newdelta {
                    nx = self.cx - 2;
                    ny = self.cy + 2;
                    ndelta = newdelta;
                }
                let newdelta = self.delta(self.cx - 2, self.cy);
                if ndelta > newdelta {
                    nx = self.cx - 2;
                    ny = self.cy;
                }
            }
            self.cx = nx;
            self.cy = ny;
        }
        Some(point)
    }
}

/// Draw.c's `ellipse_state`: the horizontal segments of a ring of `w` pixels.
struct EllipseState {
    outer: Quarter,
    inner: Quarter,
    py: i32,
    pl: i32,
    pr: i32,
    buffer: Vec<(i32, i32, i32)>,
    finished: bool,
    leftmost: i32,
}

impl EllipseState {
    fn new(a: i32, b: i32, w: i32) -> EllipseState {
        let mut outer = Quarter::new(a, b);
        let leftmost = a % 2;
        let first = if w < 1 { None } else { outer.next() };
        match first {
            None => EllipseState {
                outer,
                inner: Quarter::new(-1, -1),
                py: 0,
                pl: 0,
                pr: 0,
                buffer: Vec::new(),
                finished: true,
                leftmost,
            },
            Some((pr, py)) => EllipseState {
                outer,
                inner: Quarter::new(a - 2 * (w - 1), b - 2 * (w - 1)),
                py,
                pl: leftmost,
                pr,
                buffer: Vec::new(),
                finished: false,
                leftmost,
            },
        }
    }

    fn next(&mut self) -> Option<(i32, i32, i32)> {
        if self.buffer.is_empty() {
            if self.finished {
                return None;
            }
            let (y, mut l, r) = (self.py, self.pl, self.pr);
            loop {
                match self.outer.next() {
                    None => {
                        self.finished = true;
                        break;
                    }
                    Some((cx, cy)) if cy > y => {
                        self.pr = cx;
                        self.py = cy;
                        break;
                    }
                    Some(_) => {}
                }
            }
            self.pl = loop {
                match self.inner.next() {
                    None => break self.leftmost,
                    Some((cx, cy)) if cy > y => break cx,
                    Some((cx, _)) => l = cx,
                }
            };
            if (l > 0 || l < r) && y > 0 {
                self.buffer.push((if l == 0 { 2 } else { l }, y, r));
            }
            if y > 0 {
                self.buffer.push((-r, y, -l));
            }
            if l > 0 || l < r {
                self.buffer.push((if l == 0 { 2 } else { l }, -y, r));
            }
            self.buffer.push((-r, -y, -l));
        }
        self.buffer.pop()
    }
}

/// Draw.c's clipping tree: half planes `a x + b y + c > 0` combined.
enum Clip {
    And(Box<Clip>, Box<Clip>),
    Or(Box<Clip>, Box<Clip>),
    Half(f64, f64, f64),
}

impl Clip {
    fn combine(and: bool, l: Clip, r: Clip) -> Clip {
        if and {
            Clip::And(Box::new(l), Box::new(r))
        } else {
            Clip::Or(Box::new(l), Box::new(r))
        }
    }

    /// `clip_tree_transpose`.
    fn transpose(self) -> Clip {
        match self {
            Clip::And(l, r) => Clip::And(Box::new(l.transpose()), Box::new(r.transpose())),
            Clip::Or(l, r) => Clip::Or(Box::new(l.transpose()), Box::new(r.transpose())),
            Clip::Half(a, b, c) => Clip::Half(b, a, c),
        }
    }
}

/// `clip_tree_do_clip`: the segment `x0..=x1` of row `y` cut by `root`, as (x, +1 start / -1 end)
/// events.
fn do_clip(root: Option<&Clip>, x0: i32, y: i32, x1: i32) -> Vec<(i32, i8)> {
    let Some(root) = root else {
        return vec![(x0, 1), (x1, -1)];
    };
    match root {
        Clip::Half(a, b, c) => {
            let (mut x0, mut x1) = (x0, x1);
            let eps = 1e-9;
            let y = f64::from(y);
            if a.abs() < eps {
                if b * y + c < -eps {
                    x0 = 1;
                    x1 = 0;
                }
            } else {
                let ix = -(b * y + c) / a;
                if a * f64::from(x0) + b * y + c < eps {
                    x0 = f64::from(x0).max(ix).round() as i32;
                }
                if a * f64::from(x1) + b * y + c < eps {
                    x1 = f64::from(x1).min(ix).round() as i32;
                }
            }
            if x0 <= x1 {
                vec![(x0, 1), (x1, -1)]
            } else {
                Vec::new()
            }
        }
        Clip::And(l, r) | Clip::Or(l, r) => {
            let and = matches!(root, Clip::And(..));
            let (l1, l2) = (do_clip(Some(l), x0, y, x1), do_clip(Some(r), x0, y, x1));
            let (mut i, mut j) = (0, 0);
            let (mut k1, mut k2) = (0i32, 0i32);
            let mut out: Vec<(i32, i8)> = Vec::new();
            while i < l1.len() || j < l2.len() {
                let take_first = j == l2.len()
                    || (i < l1.len()
                        && (l1[i].0 < l2[j].0 || (l1[i].0 == l2[j].0 && l1[i].1 > l2[j].1)));
                let t = if take_first {
                    i += 1;
                    k1 += i32::from(l1[i - 1].1);
                    l1[i - 1]
                } else {
                    j += 1;
                    k2 += i32::from(l2[j - 1].1);
                    l2[j - 1]
                };
                let tail = out.last().map(|e| e.1);
                let keep = if and {
                    (t.1 == 1 && tail.is_none_or(|t| t == -1) && k1 > 0 && k2 > 0)
                        || (t.1 == -1 && tail == Some(1) && (k1 == 0 || k2 == 0))
                } else {
                    (t.1 == 1 && tail.is_none_or(|t| t == -1)) || (t.1 == -1 && k1 == 0 && k2 == 0)
                };
                if keep {
                    out.push(t);
                }
            }
            out
        }
    }
}

/// `arc_init`'s clipping tree.
fn arc_clip(a: i32, b: i32, al: f32, ar: f32) -> Option<Clip> {
    if a < b {
        return arc_clip(b, a, 90.0 - ar, 90.0 - al).map(Clip::transpose);
    }
    let (al, ar) = normalize_angles(al, ar);
    if ar == al + 360.0 {
        return None;
    }
    let (af, bf) = (f64::from(a), f64::from(b));
    let (alf, arf) = (f64::from(al), f64::from(ar));
    let lc = Clip::Half(
        f64::from(-a) * (alf * PI / 180.0).sin(),
        bf * (alf * PI / 180.0).cos(),
        f64::from(a * a - b * b) * (alf * PI / 90.0).sin() / 2.0,
    );
    let rc = Clip::Half(
        af * (arf * PI / 180.0).sin(),
        f64::from(-b) * (arf * PI / 180.0).cos(),
        f64::from(b * b - a * a) * (arf * PI / 90.0).sin() / 2.0,
    );
    let narrow = ar - al < 180.0;
    if alf % 180.0 == 0.0 || arf % 180.0 == 0.0 {
        Some(Clip::combine(narrow, lc, rc))
    } else if ((al / 180.0) as i32 + (ar / 180.0) as i32) % 2 == 1 {
        let side = |angle: f32| {
            Clip::Half(
                0.0,
                if (angle / 180.0) as i32 % 2 == 0 {
                    1.0
                } else {
                    -1.0
                },
                0.0,
            )
        };
        Some(Clip::Or(
            Box::new(Clip::And(Box::new(side(al)), Box::new(lc))),
            Box::new(Clip::And(Box::new(side(ar)), Box::new(rc))),
        ))
    } else {
        let side = Clip::Half(0.0, if ar < 180.0 || ar > 540.0 { 1.0 } else { -1.0 }, 0.0);
        Some(Clip::combine(narrow, Clip::combine(narrow, lc, rc), side))
    }
}

/// `pie_init`'s clipping tree.
fn pie_clip(a: i32, b: i32, al: f32, ar: f32) -> Clip {
    let (af, bf) = (f64::from(a), f64::from(b));
    let (alr, arr) = (f64::from(al) * PI / 180.0, f64::from(ar) * PI / 180.0);
    let (xl, xr) = (af * alr.cos(), af * arr.cos());
    let (yl, yr) = (bf * alr.sin(), bf * arr.sin());
    let root = Clip::combine(
        ar - al < 180.0,
        Clip::Half(-yl, xl, 0.0),
        Clip::Half(yr, -xr, 0.0),
    );
    if ar - al < 90.0 {
        Clip::And(
            Box::new(root),
            Box::new(Clip::Half((xl + xr) / 2.0, (yl + yr) / 2.0, 0.0)),
        )
    } else {
        root
    }
}

// -- Text (_imagingft.c with skrifa's auto-hinter) ------------------------------------------------

/// A TrueType font, read from its file.
pub struct Font {
    bytes: Vec<u8>,
    styles: GlyphStyles,
    units_per_em: i64,
    ascender: i64,
}

impl Font {
    pub fn open(path: &Path) -> io::Result<Font> {
        let bytes = std::fs::read(path)?;
        let invalid = |error: String| io::Error::new(io::ErrorKind::InvalidData, error);
        let face = FontRef::new(&bytes).map_err(|e| invalid(e.to_string()))?;
        let units_per_em = face
            .head()
            .map(|head| i64::from(head.units_per_em()))
            .map_err(|e| invalid(e.to_string()))?;
        // FreeType's `face->ascender`: the `hhea` table's.
        let ascender = face
            .hhea()
            .map(|hhea| i64::from(hhea.ascender().to_i16()))
            .map_err(|e| invalid(e.to_string()))?;
        let styles = GlyphStyles::new(&face.outline_glyphs());
        Ok(Font {
            bytes,
            styles,
            units_per_em,
            ascender,
        })
    }

    /// `ImageFont.truetype(path, size)`: the font at `size` pixels per em, hinted as FreeType
    /// loads these fonts at Pillow's `FT_LOAD_DEFAULT`: no `fpgm` and a `prep` of at most 7 bytes
    /// send them to the auto-hinter (`FT_Load_Glyph`), in the normal target (both directions,
    /// hinted advances).
    pub fn at(&self, size: i64) -> FontSize<'_> {
        // FT_DivFix(size << 6, units_per_EM): the 16.16 scale from font units to 26.6 pixels.
        let size = size.max(1);
        let scale = ((size << 6 << 16) + self.units_per_em / 2) / self.units_per_em.max(1);
        let options = HintingOptions {
            engine: Engine::Auto(Some(self.styles.clone())),
            target: SmoothMode::Normal.into(),
        };
        // `open` parsed these bytes and the auto-hinter accepts any outlines, so this is `None`
        // only in theory (and the text then draws nothing).
        let hinted = FontRef::new(&self.bytes).ok().and_then(|face| {
            let ppem = Size::new(size as f32);
            let outlines = face.outline_glyphs();
            HintingInstance::new(&outlines, ppem, LocationRef::default(), options)
                .ok()
                .map(|hinting| (face, hinting))
        });
        FontSize {
            font: self,
            hinted,
            scale,
        }
    }
}

/// A font at one size.
pub struct FontSize<'a> {
    font: &'a Font,
    hinted: Option<(FontRef<'a>, HintingInstance)>,
    scale: i64,
}

/// A segment of a glyph outline, in pixels, y up.
#[derive(Clone, Copy)]
enum Curve {
    Line(Point, Point),
    Quad(Point, Point, Point),
    Cubic(Point, Point, Point, Point),
}

/// A glyph of a laid-out line: its hinted outline (pixels, y up) and its pen position (26.6).
struct Placed {
    curves: Vec<Curve>,
    /// Control box in 26.6: x min, y min, x max, y max.
    cbox: [i64; 4],
    pen: i64,
}

/// Collects the outline skrifa draws, and its control box.
#[derive(Default)]
struct Outline {
    curves: Vec<Curve>,
    start: Option<Point>,
    current: Option<Point>,
    /// x min, y min, x max, y max of every point, in 26.6.
    cbox: Option<[i64; 4]>,
}

impl Outline {
    fn include(&mut self, x: f32, y: f32) -> Point {
        let (fx, fy) = ((x * 64.0).round() as i64, (y * 64.0).round() as i64);
        let b = self.cbox.get_or_insert([fx, fy, fx, fy]);
        *b = [b[0].min(fx), b[1].min(fy), b[2].max(fx), b[3].max(fy)];
        point(x, y)
    }
}

impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.include(x, y);
        self.start = Some(p);
        self.current = Some(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.include(x, y);
        if let Some(from) = self.current {
            self.curves.push(Curve::Line(from, p));
        }
        self.current = Some(p);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let c = self.include(cx0, cy0);
        let p = self.include(x, y);
        if let Some(from) = self.current {
            self.curves.push(Curve::Quad(from, c, p));
        }
        self.current = Some(p);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1) = (self.include(cx0, cy0), self.include(cx1, cy1));
        let p = self.include(x, y);
        if let Some(from) = self.current {
            self.curves.push(Curve::Cubic(from, c0, c1, p));
        }
        self.current = Some(p);
    }

    fn close(&mut self) {
        if let (Some(from), Some(start)) = (self.current, self.start) {
            if from != start {
                self.curves.push(Curve::Line(from, start));
            }
        }
        self.current = self.start;
    }
}

/// The bounds of a line (`bounding_box_and_anchors`): pixels, y up, from the pen line and the
/// glyphs' control boxes.
struct Extent {
    x_min: i64,
    x_max: i64,
    y_min: i64,
    y_max: i64,
}

/// What `font.render` hands `ImageDraw.text`: the coverage and where its corner goes, from the
/// text position's integer part.
struct Mask {
    picture: Picture,
    x: i64,
    y: i64,
}

/// FreeType's `PIXEL`: 26.6 to the nearest pixel.
fn pixel(v: i64) -> i64 {
    ((v + 32) & -64) >> 6
}

impl FontSize<'_> {
    /// `ImageDraw.textlength(text, font)`: the total advance of the layout (26.6) in pixels.
    pub fn length(&self, text: &str) -> f64 {
        self.layout(text).1 as f64 / 64.0
    }

    /// `FT_MulFix(units, scale)`: font units to 26.6.
    fn scaled(&self, units: i64) -> i64 {
        let ab = units * self.scale;
        (ab + 0x8000 - i64::from(ab < 0)) >> 16
    }

    /// The basic layout of `text`: one glyph per character, each with its hinted outline and the
    /// advance the hinter gives it (`glyph->metrics.horiAdvance`).
    fn layout(&self, text: &str) -> (Vec<Placed>, i64) {
        let Some((face, hinting)) = &self.hinted else {
            return (Vec::new(), 0);
        };
        let outlines = face.outline_glyphs();
        let charmap = face.charmap();
        let mut pen = 0i64;
        let mut placed = Vec::new();
        for c in text.chars() {
            let id = charmap.map(c).unwrap_or(GlyphId::NOTDEF);
            let mut outline = Outline::default();
            let drawn = outlines.get(id).and_then(|glyph| {
                glyph
                    .draw(DrawSettings::hinted(hinting, false), &mut outline)
                    .ok()
            });
            let advance = drawn
                .and_then(|metrics| metrics.advance_width)
                .map_or(0, |width| (f64::from(width) * 64.0).round() as i64);
            placed.push(Placed {
                curves: outline.curves,
                cbox: outline.cbox.unwrap_or([0; 4]),
                pen,
            });
            pen += advance;
        }
        (placed, pen)
    }

    fn extent(&self, placed: &[Placed]) -> Extent {
        let mut extent = Extent {
            x_min: 0,
            x_max: 0,
            y_min: 0,
            y_max: 0,
        };
        for glyph in placed {
            let px = pixel(glyph.pen);
            let [x0, y0, x1, y1] = glyph.cbox;
            extent.x_min = extent.x_min.min((x0 >> 6) + px);
            extent.x_max = extent.x_max.max(((x1 + 63) >> 6) + px);
            extent.y_min = extent.y_min.min(y0 >> 6);
            extent.y_max = extent.y_max.max((y1 + 63) >> 6);
        }
        extent
    }

    fn ascender(&self) -> i64 {
        // FT_PIX_CEIL(FT_MulFix(ascender, y_scale)), in pixels.
        (self.scaled(self.font.ascender) + 63) >> 6
    }

    /// `ImageDraw.textbbox((0, 0), text, font, stroke_width=stroke)`.
    pub fn bbox(&self, text: &str, stroke: i64) -> (i64, i64, i64, i64) {
        let (placed, advance) = self.layout(text);
        let mut extent = self.extent(&placed);
        if !placed.is_empty() {
            extent.x_max = extent.x_max.max(pixel(advance));
        }
        let left = extent.x_min - stroke;
        let top = self.ascender() - extent.y_max - stroke;
        (
            left,
            top,
            left + extent.x_max - extent.x_min + 2 * stroke,
            top + extent.y_max - extent.y_min + 2 * stroke,
        )
    }

    /// `font.render` for `ImageDraw.text` at `xy`: the mask and its offset.
    fn render(&self, text: &str, stroke: i64, xy: (f64, f64)) -> Option<Mask> {
        let (placed, advance) = self.layout(text);
        if placed.is_empty() {
            return None;
        }
        let mut extent = self.extent(&placed);
        extent.x_max = extent.x_max.max(pixel(advance));
        let (x_start, y_start) = (xy.0.fract() as f32, xy.1.fract() as f32);
        let sw = stroke as f32;
        let width = extent.x_max - extent.x_min + f64::from(sw * 2.0 + x_start).ceil() as i64;
        let height = extent.y_max - extent.y_min + f64::from(sw * 2.0 + y_start).ceil() as i64;
        if width <= 0 || height <= 0 {
            return None;
        }
        let x_offset = extent.x_min - stroke;
        let y_offset = self.ascender() - extent.y_max - stroke;
        let pen_x = f64::from((-(extent.x_min as f32) + sw + x_start) * 64.0).round() as i64;
        let pen_y = f64::from((-(extent.y_max as f32) - sw - y_start) * 64.0).round() as i64;
        let mut mask = Picture::new(Mode::L, width as usize, height as usize, &[]);
        let py = pixel(pen_y);
        for glyph in &placed {
            let px = pixel(pen_x + glyph.pen);
            draw_glyph(&mut mask, glyph, px, -py, stroke);
        }
        Some(Mask {
            picture: mask,
            x: x_offset,
            y: y_offset,
        })
    }
}

/// Blends the coverage of `glyph` (widened by `stroke` pixels) with its origin at `x`, `y` of the
/// L `mask`, as `font_render` blends overlapping glyphs.
fn draw_glyph(mask: &mut Picture, glyph: &Placed, x: i64, y: i64, stroke: i64) {
    if glyph.curves.is_empty() {
        return;
    }
    // The pixels the control box covers with the origin at `x`, `y` (FreeType's bitmap box).
    let [x0, y0, x1, y1] = glyph.cbox;
    let (left, top) = ((x0 >> 6) + x, y - ((y1 + 63) >> 6));
    let width = (((x1 + 63) >> 6) - (x0 >> 6)).max(0) as usize;
    let height = (((y1 + 63) >> 6) - (y0 >> 6)).max(0) as usize;
    let (dx, dy) = ((x - left) as f32, (y - top) as f32);
    let place = |p: Point| point(p.x + dx, -p.y + dy);
    let mut raster = Rasterizer::new(width, height);
    for curve in &glyph.curves {
        match *curve {
            Curve::Line(a, b) => raster.draw_line(place(a), place(b)),
            Curve::Quad(a, b, c) => raster.draw_quad(place(a), place(b), place(c)),
            Curve::Cubic(a, b, c, d) => raster.draw_cubic(place(a), place(b), place(c), place(d)),
        }
    }
    // The coverage with a margin of `stroke` pixels on every side.
    let margin = stroke as usize;
    let (w, h) = (width + 2 * margin, height + 2 * margin);
    let mut coverage = vec![0u8; w * h];
    raster.for_each_pixel_2d(|cx, cy, c| {
        let (cx, cy) = (cx as usize + margin, cy as usize + margin);
        if cx < w && cy < h {
            coverage[cy * w + cx] = gray(c);
        }
    });
    if stroke > 0 {
        coverage = widen(&coverage, w, h, margin);
    }
    let (left, top) = (left - stroke, top - stroke);
    for cy in 0..h {
        for cx in 0..w {
            let source = coverage[cy * w + cx];
            let (mx, my) = (left + cx as i64, top + cy as i64);
            if source == 0
                || mx < 0
                || my < 0
                || mx >= mask.width as i64
                || my >= mask.height as i64
            {
                continue;
            }
            let at = my as usize * mask.width + mx as usize;
            let target = mask.data[at];
            mask.data[at] = if target > 0 {
                (u32::from(source) + u32::from(muldiv255(target, 255 - source))).min(255) as u8
            } else {
                source
            };
        }
    }
}

/// A coverage as FreeType's gray rasterizer stores it for TrueType's clockwise outlines.
fn gray(c: f32) -> u8 {
    if c <= 0.0 {
        0
    } else {
        ((c * 256.0).ceil() as i32 - 1).clamp(0, 255) as u8
    }
}

/// The coverage of the outline grown by `radius` pixels (`FT_Glyph_StrokeBorder` with round
/// joins, filled): the brightest coverage within a disc around each pixel.
fn widen(coverage: &[u8], w: usize, h: usize, radius: usize) -> Vec<u8> {
    let r = radius as i64;
    let mut out = vec![0u8; w * h];
    // Row maxima over each half width of the disc (the lattice points within r + 1/2), then the
    // rows of the disc combined.
    let reach: Vec<usize> = (0..=r)
        .map(|dy| ((r * r + r - dy * dy).max(0) as f64).sqrt() as usize)
        .collect();
    let mut row_max: Vec<Vec<u8>> = Vec::with_capacity(reach.len());
    for half in &reach {
        let mut rows = vec![0u8; w * h];
        for y in 0..h {
            sliding_extreme(
                &coverage[y * w..(y + 1) * w],
                *half,
                true,
                &mut rows[y * w..(y + 1) * w],
            );
        }
        row_max.push(rows);
    }
    for y in 0..h {
        for x in 0..w {
            let mut value = 0u8;
            for (dy, rows) in row_max.iter().enumerate() {
                for sy in [y as i64 - dy as i64, y as i64 + dy as i64] {
                    if sy >= 0 && (sy as usize) < h {
                        value = value.max(rows[sy as usize * w + x]);
                    }
                }
            }
            out[y * w + x] = value;
        }
    }
    out
}

/// The maximum (or minimum) of `line` over `x - half ..= x + half` clipped to the line, for every
/// `x` (van Herk / Gil-Werman: the line padded with values that never win).
fn sliding_extreme(line: &[u8], half: usize, max: bool, out: &mut [u8]) {
    if half == 0 {
        out.copy_from_slice(line);
        return;
    }
    let pick = |a: u8, b: u8| if max { a.max(b) } else { a.min(b) };
    let pad = if max { 0 } else { 255 };
    let k = 2 * half + 1;
    let mut padded = vec![pad; line.len() + 2 * half];
    padded[half..half + line.len()].copy_from_slice(line);
    let n = padded.len();
    let mut ahead = vec![pad; n];
    let mut behind = vec![pad; n];
    for start in (0..n).step_by(k) {
        let end = (start + k).min(n);
        let mut running = pad;
        for i in start..end {
            running = pick(running, padded[i]);
            ahead[i] = running;
        }
        running = pad;
        for i in (start..end).rev() {
            running = pick(running, padded[i]);
            behind[i] = running;
        }
    }
    for (x, value) in out.iter_mut().enumerate() {
        *value = pick(behind[x], ahead[x + k - 1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture of `mode` whose every band of every pixel is pseudo-random.
    fn noise(mode: Mode, width: usize, height: usize, seed: u32) -> Picture {
        let mut picture = Picture::new(mode, width, height, &[]);
        let mut state = seed;
        for value in &mut picture.data {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *value = (state >> 16) as u8;
        }
        picture
    }

    /// Asserts `part` is `whole` inside `window` and zero outside it.
    fn assert_window(part: &Picture, whole: &Picture, window: Window, case: &str) {
        assert_eq!(
            (part.width, part.height),
            (whole.width, whole.height),
            "{case}"
        );
        for y in 0..whole.height {
            for x in 0..whole.width {
                let inside =
                    (window.x0..window.x1).contains(&x) && (window.y0..window.y1).contains(&y);
                if inside {
                    assert_eq!(part.pixel(x, y), whole.pixel(x, y), "{case}: {x}, {y}");
                } else {
                    assert!(part.pixel(x, y).iter().all(|v| *v == 0), "{case}: {x}, {y}");
                }
            }
        }
    }

    #[test]
    fn resize_window_is_the_whole_resize_inside_the_window() {
        for mode in [Mode::L, Mode::Rgb, Mode::Rgba] {
            let picture = noise(mode, 96, 80, 7);
            // The pair shares every row but ten with the first picture.
            let mut other = picture.clone();
            other.paste(&noise(mode, 96, 10, 9), 0, 33, None);
            for filter in [Filter::Lanczos, Filter::Bicubic] {
                for (width, height) in [(48, 40), (24, 20), (37, 13), (130, 7)] {
                    let whole = picture.resize(width, height, filter);
                    let other_whole = other.resize(width, height, filter);
                    for window in [
                        Window {
                            x0: width / 3,
                            y0: height / 4,
                            x1: width - 2,
                            y1: height - 1,
                        },
                        Window {
                            x0: 0,
                            y0: 0,
                            x1: width,
                            y1: height,
                        },
                        Window {
                            x0: width - 1,
                            y0: 0,
                            x1: width,
                            y1: 1,
                        },
                    ] {
                        let case = format!("{mode:?} {filter:?} {width}x{height} {window:?}");
                        let part = picture.resize_window(width, height, filter, window);
                        assert_window(&part, &whole, window, &case);
                        let (first, second) =
                            picture.resize_window_pair(&other, width, height, filter, window);
                        assert_window(&first, &whole, window, &format!("{case} pair first"));
                        assert_window(
                            &second,
                            &other_whole,
                            window,
                            &format!("{case} pair second"),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn resize_placed_is_the_resize_of_the_whole_zero_canvas() {
        let (full_width, full_height) = (160, 120);
        for mode in [Mode::L, Mode::Rgba] {
            // A letter with a zero border, so its box is smaller than the picture.
            let mut letter = Picture::new(mode, 50, 30, &[]);
            letter.paste(&noise(mode, 44, 21, 3), 3, 5, None);
            for (left, top) in [(37, 41), (110, 90), (0, 0)] {
                let mut canvas = Picture::new(mode, full_width, full_height, &[]);
                canvas.paste(&letter, left as i64, top as i64, None);
                for filter in [Filter::Lanczos, Filter::Bicubic] {
                    for (width, height) in [(40, 30), (53, 41), (80, 60), (107, 77)] {
                        let case =
                            format!("{mode:?} {filter:?} at {left}, {top} to {width}x{height}");
                        let placed = letter.resize_placed(
                            left,
                            top,
                            full_width,
                            full_height,
                            width,
                            height,
                            filter,
                        );
                        assert_eq!(placed, canvas.resize(width, height, filter), "{case}");
                    }
                }
            }
            let blank = Picture::new(mode, 50, 30, &[]);
            let placed =
                blank.resize_placed(5, 5, full_width, full_height, 40, 30, Filter::Lanczos);
            assert_eq!(placed, Picture::new(mode, 40, 30, &[]));
        }
    }

    #[test]
    fn length_is_the_sum_of_the_advances() {
        let font = Font::open(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../launcher/Cinzel-Black.ttf"
        )))
        .unwrap();
        let size = font.at(40);
        assert_eq!(size.length(""), 0.0);
        assert!(size.length("S") > 0.0);
        assert_eq!(size.length("SP"), size.length("S") + size.length("P"));
    }

    #[test]
    fn alpha_composite_at_cuts_what_falls_outside() {
        let red_at = |x: i64, y: i64| {
            let mut base = Picture::new(Mode::Rgba, 4, 4, &[0, 0, 0, 255]);
            let over = Picture::new(Mode::Rgba, 2, 2, &[255, 0, 0, 255]);
            base.alpha_composite_at(&over, x, y);
            let mut red = Vec::new();
            for py in 0..4 {
                for px in 0..4 {
                    match base.pixel(px, py) {
                        [255, 0, 0, 255] => red.push((px, py)),
                        [0, 0, 0, 255] => {}
                        other => panic!("unexpected pixel {other:?} at {px}, {py}"),
                    }
                }
            }
            red
        };
        assert_eq!(red_at(1, 1), [(1, 1), (2, 1), (1, 2), (2, 2)]);
        assert_eq!(red_at(3, 3), [(3, 3)]);
        assert_eq!(red_at(-1, -1), [(0, 0)]);
    }
}
