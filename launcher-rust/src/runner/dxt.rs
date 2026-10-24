//! Pillow's DXT1 (BC1) and DXT5 (BC3) encoder: a line-by-line port of
//! `src/libImaging/BcnEncode.c` of Pillow 12.3.0 (license in `assets/LICENSE-Pillow.txt`), so the
//! blocks are the ones `Image.save(..., 'DDS', pixel_format=...)` writes, to the byte. A crate's
//! encoder would choose other colours.
//!
//! The C reads the picture through Pillow's `Imaging` and codec state, with `x` a byte offset;
//! here `State` holds the pixels and the block's top-left pixel, the same positions (Pillow keeps
//! RGB and RGBA at 4 bytes a pixel). The float arithmetic stays `f32`, as in the C.

/// Pillow's `rgb`: red, green, blue.
type Rgb = [u8; 3];

/// Pillow's `rgba`: red, green, blue, alpha.
type Rgba = [u8; 4];

/// What the C functions read from the `Imaging` and the `ImagingCodecState`: the pixels (4 bytes
/// each, row by row), the size, and the 4 x 4 block being encoded (its top-left pixel).
struct State<'a> {
    rgba: &'a [u8],
    xsize: usize,
    ysize: usize,
    x: usize,
    y: usize,
}

impl State<'_> {
    /// Pixel `i` across and `j` down in the block, or `None` where the block extends past the
    /// edge of the image (or past the end of a short buffer).
    fn pixel(&self, i: usize, j: usize) -> Option<&[u8]> {
        let (x, y) = (self.x + i, self.y + j);
        if x >= self.xsize || y >= self.ysize {
            return None;
        }
        let at = (y * self.xsize + x) * 4;
        self.rgba.get(at..at + 4)
    }
}

fn decode_565(x: u16) -> Rgb {
    let mut r = (x & 0xf800) >> 8;
    r |= r >> 5;
    let mut g = (x & 0x7e0) >> 3;
    g |= g >> 6;
    let mut b = (x & 0x1f) << 3;
    b |= b >> 5;
    // Each fits a byte: at most 0xf8 | 0x07, 0xfc | 0x03, 0xf8 | 0x07.
    [r as u8, g as u8, b as u8]
}

fn encode_565(item: Rgba) -> u16 {
    let r = u16::from(item[0] >> (8 - 5));
    let g = u16::from(item[1] >> (8 - 6));
    let b = u16::from(item[2] >> (8 - 5));
    (r << (5 + 6)) | (g << 5) | b
}

/// `encode_bc1_color` with `separate_alpha` 0, the only value DXT1 from RGB and DXT5 pass: the
/// transparency branch (DXT1 from RGBA or LA, whose padding pixels read an uninitialised alpha in
/// the C) is not ported.
fn encode_bc1_color(state: &State, dst: &mut Vec<u8>) {
    let mut color_min: u16 = 0;
    let mut color_max: u16 = 0;
    let mut block: [Rgba; 16] = [[0; 4]; 16];

    // Determine the min and max colors in this 4x4 block
    let mut first = true;
    for i in 0..4 {
        for j in 0..4 {
            let current_rgba = &mut block[i + j * 4];

            let Some(pixel) = state.pixel(i, j) else {
                // The 4x4 block extends past the edge of the image
                current_rgba[..3].fill(0);
                continue;
            };

            current_rgba[..3].copy_from_slice(&pixel[..3]);

            let color = encode_565(*current_rgba);
            if first || color < color_min {
                color_min = color;
            }
            if first || color > color_max {
                color_max = color;
            }
            first = false;
        }
    }

    dst.extend_from_slice(&color_max.to_le_bytes());
    dst.extend_from_slice(&color_min.to_le_bytes());

    let color_min_rgb = decode_565(color_min);
    let color_max_rgb = decode_565(color_max);
    for i in 0..4 {
        let mut l: u8 = 0;
        for j in (0..4).rev() {
            let current_rgba = &block[i * 4 + j];

            let mut distance: f32 = 0.0;
            let mut total: u8 = 0;
            for k in 0..3 {
                let denom = color_max_rgb[k].abs_diff(color_min_rgb[k]);
                if denom != 0 {
                    distance +=
                        f32::from(current_rgba[k].abs_diff(color_min_rgb[k])) / f32::from(denom);
                    total += 1;
                }
            }
            if total == 0 {
                continue;
            }
            // `6 / total` divides integers, as in the C.
            distance *= f32::from(6 / total);
            if distance < 1.0 {
                l |= 1 << (j * 2); // color_min
            } else if distance < 3.0 {
                l |= 3 << (j * 2); // 1/3 * color_min + 2/3 * color_max
            } else if distance < 5.0 {
                l |= 2 << (j * 2); // 2/3 * color_min + 1/3 * color_max
            } else {
                // color_max
            }
        }
        dst.push(l);
    }
}

/// `encode_bc3_alpha`: the alpha block of BC3 from byte `o` of each pixel.
fn encode_bc3_alpha(state: &State, dst: &mut Vec<u8>, o: usize) {
    let mut alpha_min: u8 = 0;
    let mut alpha_max: u8 = 0;
    let mut block = [0u8; 16];

    // Determine the min and max colors in this 4x4 block
    let mut first = true;
    for i in 0..4 {
        for j in 0..4 {
            let Some(pixel) = state.pixel(i, j) else {
                // The 4x4 block extends past the edge of the image
                block[i + j * 4] = 0;
                continue;
            };

            let current_alpha = pixel[o];
            block[i + j * 4] = current_alpha;

            if first || current_alpha < alpha_min {
                alpha_min = current_alpha;
            }
            if first || current_alpha > alpha_max {
                alpha_max = current_alpha;
            }
            first = false;
        }
    }

    dst.push(alpha_min);
    dst.push(alpha_max);

    let denom = alpha_max.abs_diff(alpha_min);
    for i in 0..2 {
        let mut l: u32 = 0;
        for j in (0..8).rev() {
            let current_alpha = block[i * 8 + j];
            if current_alpha == 0 {
                l |= 6 << (j * 3);
                continue;
            } else if current_alpha == 255 {
                l |= 7 << (j * 3);
                continue;
            }

            let distance: f32 = if denom == 0 {
                0.0
            } else {
                f32::from(current_alpha.abs_diff(alpha_min)) / f32::from(denom) * 10.0
            };
            if distance < 3.0 {
                l |= 2 << (j * 3); // 4/5 * alpha_min + 1/5 * alpha_max
            } else if distance < 5.0 {
                l |= 3 << (j * 3); // 3/5 * alpha_min + 2/5 * alpha_max
            } else if distance < 7.0 {
                l |= 4 << (j * 3); // 2/5 * alpha_min + 3/5 * alpha_max
            } else {
                l |= 5 << (j * 3); // 1/5 * alpha_min + 4/5 * alpha_max
            }
        }
        dst.extend_from_slice(&l.to_le_bytes()[..3]);
    }
}

/// `ImagingBcnEncode` for `n` 1 (DXT1 of an RGB picture) and 3 (DXT5 of an RGBA one): the blocks
/// left to right, then top to bottom, each block's pixels past the edge as Pillow pads them.
fn bcn_encode(rgba: &[u8], width: u32, height: u32, n: u8) -> Vec<u8> {
    let (xsize, ysize) = (width as usize, height as usize);
    let will_write = if n == 3 { 16 } else { 8 };
    let mut dst = Vec::with_capacity(xsize.div_ceil(4) * ysize.div_ceil(4) * will_write);
    for y in (0..ysize).step_by(4) {
        for x in (0..xsize).step_by(4) {
            let state = State {
                rgba,
                xsize,
                ysize,
                x,
                y,
            };
            if n == 3 {
                encode_bc3_alpha(&state, &mut dst, 3);
            }
            encode_bc1_color(&state, &mut dst);
        }
    }
    dst
}

/// The bytes Pillow writes after the DDS header for `pixel_format='DXT1'`: 8 bytes per 4 x 4 block,
/// for any width and height. `rgba` holds `width` x `height` pixels, 4 bytes each, row by row.
///
/// The alpha is not read: the result is Pillow's for the picture converted to RGB first (as
/// `glyphs.encode_region` and `Image.convert('RGB')` do), so BC1's 3-colour mode with transparent
/// pixels never comes from the input's alpha. Pixels missing from a short `rgba` are encoded as
/// past the edge of the picture.
pub fn encode_dxt1(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    bcn_encode(rgba, width, height, 1)
}

/// The bytes Pillow writes after the DDS header for `pixel_format='DXT5'` from an RGBA picture:
/// 16 bytes per 4 x 4 block (BC3's alpha block, then the colour block), for any width and height.
/// `rgba` holds `width` x `height` pixels, 4 bytes each, row by row; pixels missing from a short
/// buffer are encoded as past the edge of the picture.
pub fn encode_dxt5(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    bcn_encode(rgba, width, height, 3)
}

// -- Decoding: Pillow 12.3.0 `src/libImaging/BcnDecode.c` (public domain), BC1 and BC3 ------------

/// `decode_bc1_color`: the 16 pixels of a colour block, row by row. `separate_alpha` (BC3) always
/// takes the four-colour mode.
fn decode_bc1_color(src: &[u8], separate_alpha: bool) -> [Rgba; 16] {
    let c0 = u16::from_le_bytes([src[0], src[1]]);
    let c1 = u16::from_le_bytes([src[2], src[3]]);
    let [r0, g0, b0] = decode_565(c0).map(u16::from);
    let [r1, g1, b1] = decode_565(c1).map(u16::from);
    let opaque = |r: u16, g: u16, b: u16| [r as u8, g as u8, b as u8, 0xff];
    let p = if c0 > c1 || separate_alpha {
        [
            opaque(r0, g0, b0),
            opaque(r1, g1, b1),
            opaque((2 * r0 + r1) / 3, (2 * g0 + g1) / 3, (2 * b0 + b1) / 3),
            opaque((r0 + 2 * r1) / 3, (g0 + 2 * g1) / 3, (b0 + 2 * b1) / 3),
        ]
    } else {
        [
            opaque(r0, g0, b0),
            opaque(r1, g1, b1),
            opaque((r0 + r1) / 2, (g0 + g1) / 2, (b0 + b1) / 2),
            [0, 0, 0, 0],
        ]
    };
    let mut out = [[0u8; 4]; 16];
    for n in 0..4 {
        for o in 0..4 {
            out[n * 4 + o] = p[usize::from(3 & (src[4 + n] >> (2 * o)))];
        }
    }
    out
}

/// `decode_bc3_alpha`: the 16 alphas of an alpha block.
fn decode_bc3_alpha(src: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u16::from(src[0]), u16::from(src[1]));
    let lut1 = u32::from(src[2]) | (u32::from(src[3]) << 8) | (u32::from(src[4]) << 16);
    let lut2 = u32::from(src[5]) | (u32::from(src[6]) << 8) | (u32::from(src[7]) << 16);
    let a: [u16; 8] = if a0 > a1 {
        [
            a0,
            a1,
            (6 * a0 + a1) / 7,
            (5 * a0 + 2 * a1) / 7,
            (4 * a0 + 3 * a1) / 7,
            (3 * a0 + 4 * a1) / 7,
            (2 * a0 + 5 * a1) / 7,
            (a0 + 6 * a1) / 7,
        ]
    } else {
        [
            a0,
            a1,
            (4 * a0 + a1) / 5,
            (3 * a0 + 2 * a1) / 5,
            (2 * a0 + 3 * a1) / 5,
            (a0 + 4 * a1) / 5,
            0,
            0xff,
        ]
    };
    let mut out = [0u8; 16];
    for n in 0..8 {
        out[n] = a[(7 & (lut1 >> (3 * n))) as usize] as u8;
        out[8 + n] = a[(7 & (lut2 >> (3 * n))) as usize] as u8;
    }
    out
}

/// Decodes `width` x `height` pixels of DXT blocks, `block` bytes each, as RGBA rows (Pillow's
/// `Image.open` of a DDS); `None` when `data` is short.
fn bcn_decode(data: &[u8], width: u32, height: u32, block: usize) -> Option<Vec<u8>> {
    let (width, height) = (width as usize, height as usize);
    let columns = width.div_ceil(4);
    let rows = height.div_ceil(4);
    let blocks = data.get(..columns * rows * block)?;
    let mut rgba = vec![0u8; width * height * 4];
    for (i, src) in blocks.chunks_exact(block).enumerate() {
        let (bx, by) = (i % columns * 4, i / columns * 4);
        let pixels = if block == 16 {
            let mut colour = decode_bc1_color(&src[8..], true);
            for (pixel, alpha) in colour.iter_mut().zip(decode_bc3_alpha(src)) {
                pixel[3] = alpha;
            }
            colour
        } else {
            decode_bc1_color(src, false)
        };
        for (n, pixel) in pixels.iter().enumerate() {
            let (x, y) = (bx + n % 4, by + n / 4);
            if x < width && y < height {
                let at = (y * width + x) * 4;
                rgba[at..at + 4].copy_from_slice(pixel);
            }
        }
    }
    Some(rgba)
}

/// The RGBA pixels Pillow decodes from `width` x `height` of DXT1 (BC1) blocks.
pub fn decode_dxt1(data: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    bcn_decode(data, width, height, 8)
}

/// The RGBA pixels Pillow decodes from `width` x `height` of DXT5 (BC3) blocks.
pub fn decode_dxt5(data: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    bcn_decode(data, width, height, 16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/runner/dxt")).join(name)
    }

    #[test]
    fn odd_sizes_pad_like_pillow() {
        // 5 x 3, one strip: both blocks reach past the bottom edge, the second past the right one.
        let image = image::open(fixture("input/odd.png")).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (5, 3));
        let dxt1 = std::fs::read(fixture("expected/odd.dxt1")).unwrap();
        let dxt5 = std::fs::read(fixture("expected/odd.dxt5")).unwrap();
        assert_eq!(encode_dxt1(image.as_raw(), 5, 3), dxt1);
        assert_eq!(encode_dxt5(image.as_raw(), 5, 3), dxt5);
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Pillow's `Image.open` of an 8 x 4 DDS holding the two blocks, as RGBA rows.
    fn rows(pixels: &[[u8; 4]]) -> Vec<u8> {
        pixels.iter().flatten().copied().collect()
    }

    #[test]
    fn decode_matches_pillow() {
        // c0 > c1 (four colours), then c0 <= c1 (three colours and transparent black).
        let colour = hex("1ff8e007e4e4e41be0071ff8e4e4e41b");
        let (m, g, mg, gm, grey, clear) = (
            [255, 0, 255, 255],
            [0, 255, 0, 255],
            [170, 85, 170, 255],
            [85, 170, 85, 255],
            [127, 127, 127, 255],
            [0, 0, 0, 0],
        );
        let row = [m, g, mg, gm, g, m, grey, clear];
        let last = [gm, mg, g, m, clear, grey, m, g];
        let expected = rows(&[row, row, row, last].concat());
        assert_eq!(decode_dxt1(&colour, 8, 4), Some(expected));
        // a0 > a1 (eight alphas), then a0 <= a1 (six, 0 and 255); the colour blocks act as c0 > c1.
        let mut blocks = hex("f0108899aabbccdd");
        blocks.extend(hex("1ff8e007e4e4e41b"));
        blocks.extend(hex("20e0123456789abc"));
        blocks.extend(hex("e0071ff8e4e4e41b"));
        let alphas = [
            240, 16, 80, 144, 70, 70, 32, 70, 16, 112, 208, 112, 108, 147, 185, 70, 176, 48, 208,
            80, 32, 255, 224, 185, 144, 176, 48, 80, 224, 224, 255, 185,
        ];
        let colours = [
            m, g, mg, gm, g, m, gm, mg, m, g, mg, gm, g, m, gm, mg, m, g, mg, gm, g, m, gm, mg, gm,
            mg, g, m, mg, gm, m, g,
        ];
        let expected: Vec<u8> = colours
            .iter()
            .zip(alphas)
            .flat_map(|(c, a)| [c[0], c[1], c[2], a])
            .collect();
        assert_eq!(decode_dxt5(&blocks, 8, 4), Some(expected));
        assert_eq!(decode_dxt1(&colour[..8], 8, 4), None);
    }
}
