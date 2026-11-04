//! Texture mip chains made on the CPU, the one definition the compiler encodes and the renderer uploads (ADR 0035).

/// The mip levels below an RGBA8 image of `width` by `height` texels: each half the one above (rounded down, at
/// least 1), down to 1x1. A level texel is the bilinear sample of the level above at its centre with edges clamped
/// (a 2x2 box for even sides), filtered in linear light when `srgb` (alpha always linear) and rounded to 8 bits.
///
/// # Panics
///
/// If `rgba` is not `width * height * 4` bytes.
pub fn mips(rgba: &[u8], width: u32, height: u32, srgb: bool) -> Vec<Vec<u8>> {
    assert_eq!(
        rgba.len() as u64,
        u64::from(width) * u64::from(height) * 4,
        "RGBA8 texel count"
    );
    let to_linear: Vec<f32> = (0..=255u8)
        .map(|v| {
            let c = f32::from(v) / 255.0;
            if !srgb {
                c
            } else if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
        .collect();
    let encode = |c: f32, colour: bool| -> u8 {
        let c = c.clamp(0.0, 1.0);
        let c = if !(srgb && colour) {
            c
        } else if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        };
        (c * 255.0).round() as u8
    };
    // The two source texels around a level texel's centre and the weight of the second.
    let taps = |d: u32, dn: u32, sn: u32| {
        let u = (d as f32 + 0.5) * sn as f32 / dn as f32 - 0.5;
        let lo = u.floor();
        let clamp = |v: f32| (v.max(0.0) as u32).min(sn - 1);
        (clamp(lo), clamp(lo + 1.0), u - lo)
    };
    let mut levels: Vec<Vec<u8>> = Vec::new();
    let (mut sw, mut sh) = (width, height);
    while sw > 1 || sh > 1 {
        let above = levels.last().map_or(rgba, Vec::as_slice);
        let (w, h) = ((sw / 2).max(1), (sh / 2).max(1));
        let mut level = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let (y0, y1, fy) = taps(y, h, sh);
            for x in 0..w {
                let (x0, x1, fx) = taps(x, w, sw);
                for c in 0..4 {
                    let get = |sx: u32, sy: u32| {
                        let v = above[((sy * sw + sx) * 4) as usize + c];
                        if c == 3 {
                            f32::from(v) / 255.0
                        } else {
                            to_linear[v as usize]
                        }
                    };
                    let top = get(x0, y0) * (1.0 - fx) + get(x1, y0) * fx;
                    let bottom = get(x0, y1) * (1.0 - fx) + get(x1, y1) * fx;
                    level.push(encode(top * (1.0 - fy) + bottom * fy, c != 3));
                }
            }
        }
        levels.push(level);
        (sw, sh) = (w, h);
    }
    levels
}
