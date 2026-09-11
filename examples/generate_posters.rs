//! Generates the shared synthetic benchmark posters:
//! `benchmark/assets/posters/poster-001.jpg` … `poster-100.jpg`.
//!
//! Run with `cargo run --example generate_posters`. Pure integer arithmetic, no
//! RNG, so every run writes byte-identical files (see `SHA256SUMS` next to the
//! posters). Each poster has its own hue and shapes, and its number is drawn in
//! large digits, so a wrong poster is obvious on screen.

use std::path::Path;

use image::ExtendedColorType;
use image::codecs::jpeg::JpegEncoder;

const COUNT: u32 = 100;
const WIDTH: u32 = 240;
const HEIGHT: u32 = 360;
const QUALITY: u8 = 85;

/// 5×7 bitmap digits, one byte per row, low five bits used.
#[rustfmt::skip]
const DIGITS: [[u8; 7]; 10] = [
    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
    [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
    [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
    [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
    [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmark/assets/posters");
    std::fs::create_dir_all(&dir)?;
    for n in 1..=COUNT {
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, QUALITY).encode(
            &poster(n),
            WIDTH,
            HEIGHT,
            ExtendedColorType::Rgb8,
        )?;
        std::fs::write(dir.join(format!("poster-{n:03}.jpg")), jpeg)?;
    }
    println!("wrote {COUNT} posters to {}", dir.display());
    Ok(())
}

/// RGB8 pixels of poster `n`.
fn poster(n: u32) -> Vec<u8> {
    // Hues 137.5° apart (golden angle): neighbouring numbers look unrelated.
    let hue = n * 1375 % 3600;
    let sky = hsl(hue, 55, 58);
    let ground = hsl(hue, 60, 18);
    let sun = hsl((hue + 1800) % 3600, 70, 62);
    let (cx, cy, r) = (
        (50 + n * 53 % 140) as i32,
        (70 + n * 29 % 110) as i32,
        (30 + n * 17 % 35) as i32,
    );
    let horizon = 190 + n * 37 % 50;
    let stripe = 9 + n % 7;

    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let (dx, dy) = (x as i32 - cx, y as i32 - cy);
            let mut rgb = if dx * dx + dy * dy <= r * r {
                sun
            } else if y < horizon {
                mix(sky, ground, y * 128 / horizon)
            } else if ((x + y) / stripe).is_multiple_of(2) {
                ground
            } else {
                mix(ground, sky, 40)
            };
            // Band behind the number, then the number itself.
            if (262..=342).contains(&y) {
                rgb = mix(rgb, [0, 0, 0], 150);
                if digit_pixel(n, x, y) {
                    rgb = [245, 245, 240];
                }
            }
            // Deterministic grain, so the JPEG has photo-like entropy rather
            // than flat colour runs that decode unrealistically fast.
            let grain = (hash(n, x, y) % 17) as i32 - 8;
            pixels.extend(rgb.map(|c| (i32::from(c) + grain).clamp(0, 255) as u8));
        }
    }
    pixels
}

/// Whether (x, y) is inside the three-digit label, drawn at 10× scale.
fn digit_pixel(n: u32, x: u32, y: u32) -> bool {
    const SCALE: u32 = 10;
    const LEFT: u32 = (WIDTH - (3 * 5 + 2) * SCALE) / 2;
    const TOP: u32 = 267;
    if x < LEFT || y < TOP {
        return false;
    }
    let (col, row) = ((x - LEFT) / SCALE, (y - TOP) / SCALE);
    let (glyph, gx) = (col / 6, col % 6);
    if glyph >= 3 || gx == 5 || row >= 7 {
        return false;
    }
    let digit = [n / 100, n / 10 % 10, n % 10][glyph as usize];
    DIGITS[digit as usize][row as usize] >> (4 - gx) & 1 == 1
}

/// `a` blended toward `b` by `t`/255.
fn mix(a: [u8; 3], b: [u8; 3], t: u32) -> [u8; 3] {
    [0, 1, 2].map(|i| ((u32::from(a[i]) * (255 - t) + u32::from(b[i]) * t) / 255) as u8)
}

/// HSL to RGB with hue in tenths of a degree and saturation/lightness in
/// percent, in integer arithmetic so the output is identical everywhere.
fn hsl(hue: u32, s: u32, l: u32) -> [u8; 3] {
    let c = (100 - (2 * l as i32 - 100).abs()) as u32 * s * 255 / 10_000;
    let h = hue % 3600;
    let x = c * (600 - (h % 1200).abs_diff(600)) / 600;
    let m = l * 255 / 100 - c / 2;
    let (r, g, b) = match h / 600 {
        0 => (c, x, 0),
        1 => (x, c, 0),
        2 => (0, c, x),
        3 => (0, x, c),
        4 => (x, 0, c),
        _ => (c, 0, x),
    };
    [r + m, g + m, b + m].map(|v| v.min(255) as u8)
}

fn hash(n: u32, x: u32, y: u32) -> u32 {
    let mut h = n
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(x.wrapping_mul(0x85EB_CA6B))
        .wrapping_add(y.wrapping_mul(0xC2B2_AE35));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}
