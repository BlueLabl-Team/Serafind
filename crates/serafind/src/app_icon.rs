//! The icons of applications (PNG or SVG files of the icon theme), turned into images the bar
//! can draw.

use std::path::Path;

use eframe::egui::ColorImage;

/// The size SVG icons are drawn at, in pixels: sharp up to twice the usual row icon size.
pub const SIZE: u32 = 64;

/// Reads an icon file; `None` when it cannot be read or drawn.
#[must_use]
pub fn load(path: &Path) -> Option<ColorImage> {
    let data = std::fs::read(path).ok()?;
    let svg = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));
    if svg {
        from_svg(&data, SIZE)
    } else {
        from_png(&data)
    }
}

fn from_svg(data: &[u8], size: u32) -> Option<ColorImage> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let picture = tree.size();
    #[expect(clippy::cast_precision_loss, reason = "icon sizes are small")]
    let target = size as f32;
    let scale = (target / picture.width()).min(target / picture.height());
    // Centered when the picture is not square.
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(
        (target - picture.width() * scale) / 2.0,
        (target - picture.height() * scale) / 2.0,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let pixels = usize::try_from(size).ok()?;
    Some(ColorImage::from_rgba_premultiplied(
        [pixels, pixels],
        pixmap.data(),
    ))
}

fn from_png(data: &[u8]) -> Option<ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], u8::MAX])
            .collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        png::ColorType::Grayscale => buffer
            .iter()
            .flat_map(|&gray| [gray, gray, gray, u8::MAX])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    let size = [
        usize::try_from(info.width).ok()?,
        usize::try_from(info.height).ok()?,
    ];
    Some(ColorImage::from_rgba_unmultiplied(size, &rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_icons_are_drawn() {
        let folder = assert_fs::TempDir::new().unwrap();
        let file = folder.path().join("app.svg");
        std::fs::write(
            &file,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 8"><rect width="16" height="8" fill="#ff0000"/></svg>"##,
        )
        .unwrap();
        let image = load(&file).unwrap();
        assert_eq!(image.size, [64, 64]);
        // A wide picture is centered: red in the middle, transparent at the top.
        assert_eq!(image.pixels[32 * 64 + 32].r(), 255);
        assert_eq!(image.pixels[0].a(), 0);
    }

    #[test]
    fn png_icons_are_read() {
        let folder = assert_fs::TempDir::new().unwrap();
        let file = folder.path().join("app.png");
        std::fs::write(&file, crate::logo::PNG_32).unwrap();
        assert_eq!(load(&file).unwrap().size, [32, 32]);
    }

    #[test]
    fn broken_icons_are_none() {
        let folder = assert_fs::TempDir::new().unwrap();
        let file = folder.path().join("broken.svg");
        std::fs::write(&file, "not an svg").unwrap();
        assert!(load(&file).is_none());
        assert!(load(&folder.path().join("missing.png")).is_none());
    }
}
