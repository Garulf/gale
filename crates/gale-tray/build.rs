use std::env;
use std::fs;
use std::path::Path;

const SIZE: i32 = 32;
const RADIUS: f64 = 14.0;
const CENTER: f64 = 15.5;
const ACCENT: [u8; 3] = [0x1f, 0x8a, 0xff];

fn main() {
    let out_dir = env::var("OUT_DIR").expect("cargo always sets OUT_DIR");
    let dest = Path::new(&out_dir).join("icon_rgba.rs");
    fs::write(&dest, render_source()).expect("write icon_rgba.rs");
}

fn render_source() -> String {
    let pixels = render_pixels();
    let literal = pixels
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "pub const ICON_WIDTH: u32 = {SIZE};\npub const ICON_HEIGHT: u32 = {SIZE};\npub const ICON_RGBA: [u8; {len}] = [{literal}];\n",
        len = pixels.len(),
    )
}

fn render_pixels() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f64 + 0.5 - CENTER;
            let dy = y as f64 + 0.5 - CENTER;
            if (dx * dx + dy * dy).sqrt() <= RADIUS {
                pixels.extend_from_slice(&[ACCENT[0], ACCENT[1], ACCENT[2], 0xff]);
            } else {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    pixels
}
