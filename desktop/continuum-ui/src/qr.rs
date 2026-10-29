//! Renders a QR code into a GDK texture (black on white, with the standard quiet zone).

use gtk::gdk;
use qrcode::{Color, QrCode};

const QUIET: usize = 4;

pub fn texture(text: &str) -> Option<gdk::MemoryTexture> {
    let code = QrCode::new(text.as_bytes()).ok()?;
    let n = code.width();
    let colors = code.to_colors();
    let size = n + 2 * QUIET;
    let scale = (480 / size).max(1);
    let px = size * scale;
    let mut rgba = vec![255u8; px * px * 4];
    for (i, c) in colors.iter().enumerate() {
        if *c != Color::Dark {
            continue;
        }
        let (mx, my) = (i % n + QUIET, i / n + QUIET);
        for y in my * scale..(my + 1) * scale {
            for x in mx * scale..(mx + 1) * scale {
                let o = (y * px + x) * 4;
                rgba[o..o + 3].fill(0);
            }
        }
    }
    let bytes = glib::Bytes::from_owned(rgba);
    Some(gdk::MemoryTexture::new(
        px as i32,
        px as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        px * 4,
    ))
}
