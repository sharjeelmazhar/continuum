//! Turns any peer-supplied file name into a safe base name (see `docs/design/06-security.md`).

const MAX_BYTES: usize = 255;

/// Returns a name that is a single path component: no separators, no control characters, no
/// leading dots (no hidden files, no `.`/`..`), at most 255 bytes, never empty.
pub fn sanitize(name: &str) -> String {
    // Keep only the last component of either kind of path.
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    let mut out = truncate_keep_extension(trimmed, MAX_BYTES);
    if out.is_empty() {
        out = "file".to_owned();
    }
    out
}

/// Cuts `name` to at most `max` bytes on a character boundary, keeping a short extension.
fn truncate_keep_extension(name: &str, max: usize) -> String {
    if name.len() <= max {
        return name.to_owned();
    }
    let ext = name
        .rfind('.')
        .map(|i| &name[i..])
        .filter(|e| e.len() <= 16)
        .unwrap_or("");
    let mut stem_end = max - ext.len();
    while !name.is_char_boundary(stem_end) {
        stem_end -= 1;
    }
    format!("{}{ext}", &name[..stem_end])
}

/// `name`, or `name (2)`, `name (3)`… for the first one `exists` says is free.
pub fn unique(name: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(name) {
        return name.to_owned();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..)
        .map(|n| truncate_keep_extension(&format!("{stem} ({n}){ext}"), MAX_BYTES))
        .find(|candidate| !exists(candidate))
        .expect("unbounded search")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn hostile_names() {
        assert_eq!(sanitize("../../.ssh/authorized_keys"), "authorized_keys");
        assert_eq!(sanitize("/etc/passwd"), "passwd");
        assert_eq!(sanitize("..\\..\\boot.ini"), "boot.ini");
        assert_eq!(sanitize(".."), "file");
        assert_eq!(sanitize("."), "file");
        assert_eq!(sanitize(""), "file");
        assert_eq!(sanitize(".bashrc"), "bashrc");
        assert_eq!(sanitize("a\0b\nc.txt"), "a_b_c.txt");
        assert_eq!(sanitize("photo:1?.jpg"), "photo_1_.jpg");
        assert_eq!(sanitize("  IMG_001.jpg  "), "IMG_001.jpg");
        assert_eq!(sanitize("café ☕.png"), "café ☕.png");
    }

    #[test]
    fn long_names_keep_extension_and_utf8() {
        let s = sanitize(&format!("{}.mp4", "é".repeat(300)));
        assert!(s.len() <= 255);
        assert!(s.ends_with(".mp4"));
    }

    #[test]
    fn unique_adds_counter_before_extension() {
        let taken = ["a.jpg", "a (2).jpg", "noext"];
        let exists = |n: &str| taken.contains(&n);
        assert_eq!(unique("a.jpg", exists), "a (3).jpg");
        assert_eq!(unique("b.jpg", exists), "b.jpg");
        assert_eq!(unique("noext", exists), "noext (2)");
    }

    proptest! {
        #[test]
        fn always_a_single_safe_component(name in ".*") {
            let s = sanitize(&name);
            prop_assert!(!s.is_empty() && s.len() <= 255);
            prop_assert!(!s.contains('/') && !s.contains('\\') && !s.contains('\0'));
            prop_assert!(!s.starts_with('.'));
            prop_assert!(!s.chars().any(char::is_control));
        }
    }
}
