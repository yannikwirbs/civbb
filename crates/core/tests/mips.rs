//! CPU mip chains (ADR 0035): 2x2 boxes for even sides, linear-light averages for sRGB colour, linear alpha,
//! bilinear taps for odd sides, down to 1x1.

use civbb_core::mips::mips;

fn rgba(texels: &[[u8; 4]]) -> Vec<u8> {
    texels.concat()
}

#[test]
fn even_sides_average_2x2_boxes() {
    // 4x2: two boxes, each of four distinct linear values.
    let image = rgba(&[
        [0, 10, 20, 0],
        [4, 14, 24, 4],
        [100, 0, 0, 255],
        [100, 0, 0, 255],
        [2, 12, 22, 2],
        [6, 16, 26, 6],
        [100, 0, 0, 255],
        [100, 0, 0, 255],
    ]);
    let levels = mips(&image, 4, 2, false);
    assert_eq!(levels.len(), 2, "2x1 then 1x1");
    assert_eq!(levels[0], rgba(&[[3, 13, 23, 3], [100, 0, 0, 255]]));
    assert_eq!(levels[1], rgba(&[[52, 7, 12, 129]]));
}

#[test]
fn srgb_colour_averages_in_linear_light_and_alpha_stays_linear() {
    let image = rgba(&[[0, 0, 0, 0], [255, 255, 255, 255]]);
    // Half of linear white is sRGB 188; alpha 0 and 255 average to 128 either way.
    assert_eq!(mips(&image, 2, 1, true), vec![rgba(&[[188, 188, 188, 128]])]);
    assert_eq!(mips(&image, 2, 1, false), vec![rgba(&[[128, 128, 128, 128]])]);
}

#[test]
fn odd_sides_and_non_square_chains_end_at_1x1() {
    // 3 wide halves to 1: the centre texel's centre, exactly the middle source texel.
    let image = rgba(&[[0, 0, 0, 0], [90, 90, 90, 90], [255, 255, 255, 255]]);
    assert_eq!(mips(&image, 3, 1, false), vec![rgba(&[[90, 90, 90, 90]])]);
    let sizes: Vec<usize> = mips(&vec![0; 16 * 4 * 4], 16, 4, false).iter().map(Vec::len).collect();
    assert_eq!(sizes, [8 * 2 * 4, 4 * 4, 2 * 4, 4], "8x2, 4x1, 2x1, 1x1");
    assert!(mips(&[1, 2, 3, 4], 1, 1, true).is_empty());
}
