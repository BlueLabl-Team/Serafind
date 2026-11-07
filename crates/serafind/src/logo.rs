//! Serafind's logo: a winged magnifying glass (Serafind = seraphim + find). The source is
//! `assets/logo/serafind.svg`; the PNG sizes are rendered from it.

/// The logo, 256 × 256: desktop icon and window icon.
pub const PNG_256: &[u8] = include_bytes!("../assets/logo/serafind-256.png");
/// The logo, 64 × 64.
pub const PNG_64: &[u8] = include_bytes!("../assets/logo/serafind-64.png");
/// The logo, 32 × 32: notification area.
pub const PNG_32: &[u8] = include_bytes!("../assets/logo/serafind-32.png");

/// An image decoded to 8-bit RGBA pixels, row by row.
#[derive(Debug, Clone)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Red, green, blue, alpha for each pixel.
    pub rgba: Vec<u8>,
}

/// Decodes one of the built-in PNG files.
///
/// # Panics
/// When the PNG is not an 8-bit RGBA image (the built-in files are, checked by a test).
#[must_use]
pub fn decode(png: &[u8]) -> Image {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().expect("built-in PNG");
    let mut rgba = vec![0; reader.output_buffer_size().expect("small image")];
    let info = reader.next_frame(&mut rgba).expect("built-in PNG");
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Rgba, png::BitDepth::Eight),
        "built-in PNG must be 8-bit RGBA"
    );
    rgba.truncate(info.buffer_size());
    Image {
        width: info.width,
        height: info.height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_logos_decode() {
        for (png, size) in [(PNG_256, 256), (PNG_64, 64), (PNG_32, 32)] {
            let image = decode(png);
            assert_eq!((image.width, image.height), (size, size));
            assert_eq!(image.rgba.len(), (size * size * 4) as usize);
            // Transparent corners: the logo is a rounded square.
            assert_eq!(image.rgba[3], 0);
        }
    }
}
