//! Text size for the whole app (Small, Default, Large, Larger), chosen from the main menu and
//! remembered in ~/.config/continuum/text-size. It scales this app's fonts only, on top of the
//! desktop's own text scaling.

use std::cell::Cell;

/// Menu id and font scale for each size.
pub const SIZES: &[(&str, &str, f64)] = &[
    ("small", "Small", 0.9),
    ("default", "Default", 1.0),
    ("large", "Large", 1.15),
    ("larger", "Larger", 1.3),
];

thread_local! {
    /// The desktop's own font DPI, read once, so sizes scale from it and not from each other.
    static BASE_DPI: Cell<i32> = const { Cell::new(0) };
}

fn file() -> std::path::PathBuf {
    glib::user_config_dir().join("continuum").join("text-size")
}

/// The saved size id, or "default".
pub fn saved() -> String {
    std::fs::read_to_string(file())
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| SIZES.iter().any(|(id, ..)| id == s))
        .unwrap_or_else(|| "default".to_owned())
}

fn scale(id: &str) -> f64 {
    SIZES
        .iter()
        .find(|(i, ..)| *i == id)
        .map_or(1.0, |(.., s)| *s)
}

/// Applies a size to this app's text.
pub fn apply(id: &str) {
    let Some(settings) = gtk::Settings::default() else {
        return;
    };
    let base = BASE_DPI.with(|b| {
        if b.get() <= 0 {
            b.set(settings.gtk_xft_dpi());
        }
        b.get()
    });
    // -1 means "not set": GTK then uses 96 DPI.
    let base = if base > 0 { base } else { 96 * 1024 };
    settings.set_gtk_xft_dpi((f64::from(base) * scale(id)).round() as i32);
}

/// Applies and remembers a size.
pub fn set(id: &str) {
    apply(id);
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_sizes_are_default() {
        assert_eq!(scale("larger"), 1.3);
        assert_eq!(scale("huge"), 1.0);
    }
}
