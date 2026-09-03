pub const ICON_WIDTH: u32 = 32;
pub const ICON_HEIGHT: u32 = 32;
pub const ICON_RGBA: &[u8] = include_bytes!("../../../assets/tray-32.rgba");

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * ICON_WIDTH + x) * 4) as usize;
        [
            ICON_RGBA[offset],
            ICON_RGBA[offset + 1],
            ICON_RGBA[offset + 2],
            ICON_RGBA[offset + 3],
        ]
    }

    #[test]
    fn icon_rgba_length_matches_dimensions() {
        assert_eq!(ICON_RGBA.len(), (ICON_WIDTH * ICON_HEIGHT * 4) as usize);
    }

    #[test]
    fn icon_has_opaque_white_center_and_transparent_corner() {
        assert_eq!(pixel(16, 16), [0xff, 0xff, 0xff, 0xff]);
        assert_eq!(pixel(0, 0)[3], 0);
    }
}
