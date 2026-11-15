//! "Send to <phone>" in file manager right-click menus that can't ask the daemon themselves,
//! kept in step with the connected phones (one entry per phone, none while none is connected):
//!
//! - GNOME's desktop icons (DING) and Files' Scripts menu: executable scripts in
//!   `~/.local/share/nautilus/scripts`. (Files itself also gets the entries from our Nautilus
//!   extension.)
//! - KDE Dolphin and the Plasma desktop: a service menu in `~/.local/share/kio/servicemenus`
//!   (Plasma 6; it must be executable) and `kservices5/ServiceMenus` (Plasma 5).
//!
//! Only files carrying [`MARKER`] are ever changed or removed.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::broadcast::error::RecvError;

use crate::state::{Daemon, Event};

const MARKER: &str = "Continuum send menu";
const KDE_FILE: &str = "continuum-send.desktop";

/// (device id, name) of the phones files can be sent to right now.
pub type Targets = Vec<(String, String)>;

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        })
}

fn targets(daemon: &Daemon) -> Targets {
    let mut t: Targets = daemon
        .list()
        .into_iter()
        .filter(|s| s.connected)
        .map(|s| (s.device.id, s.device.name))
        .collect();
    t.sort();
    t
}

/// Keeps the menus in step with the connected phones until the daemon stops.
pub async fn run(daemon: Arc<Daemon>) {
    let exe = std::env::current_exe().unwrap_or_else(|_| "continuumd".into());
    let mut events = daemon.events.subscribe();
    let mut last = None;
    loop {
        let now = targets(&daemon);
        if last.as_ref() != Some(&now) {
            if let Err(e) = sync(&data_home(), &exe, &now) {
                tracing::debug!("could not update the Send to menus: {e}");
            }
            last = Some(now);
        }
        match events.recv().await {
            Ok(Event::DevicesChanged) | Err(RecvError::Lagged(_)) => {}
            Ok(_) => continue,
            Err(RecvError::Closed) => return,
        }
    }
}

/// Removes every entry (the daemon is stopping: nothing could be sent anyway).
pub fn clear() {
    let _ = sync(&data_home(), Path::new("continuumd"), &[]);
}

/// A phone name as a file name / menu label: no slashes or control characters.
fn label(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .filter(|c| !c.is_control())
        .take(60)
        .collect();
    let clean = clean.trim().trim_start_matches('.').to_owned();
    if clean.is_empty() {
        "phone".into()
    } else {
        clean
    }
}

/// `'…'` for sh.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A quoted argument for a desktop entry's Exec key (spec: escape `"` `` ` `` `$` `\` inside
/// double quotes, then `\` again for the string value).
fn exec_quote(s: &str) -> String {
    let mut q = String::from("\"");
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            q.push('\\');
        }
        q.push(c);
    }
    q.push('"');
    q.replace('\\', "\\\\")
}

fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::read_to_string(path).ok().as_deref() != Some(content) {
        // Executable: Plasma 6 shows a non-executable service menu but refuses to run it.
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, content)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&tmp, path)?;
    }
    Ok(())
}

fn ours(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|c| c.contains(MARKER))
}

pub fn sync(data: &Path, exe: &Path, targets: &[(String, String)]) -> std::io::Result<()> {
    let exe = exe.to_string_lossy();
    // Nautilus scripts: the file name is the menu label.
    let scripts = data.join("nautilus/scripts");
    let mut wanted = Vec::new();
    for (id, name) in targets {
        let mut file = format!("Send to {}", label(name));
        if wanted.iter().any(|(f, _): &(String, String)| *f == file) {
            file.push_str(&format!(" ({})", &id[..id.len().min(4)]));
        }
        let script = format!(
            "#!/bin/sh\n# {MARKER} (made by continuumd; follows the connected phones)\nexec {} send --to {} -- \"$@\"\n",
            sh_quote(&exe),
            sh_quote(id)
        );
        wanted.push((file, script));
    }
    if !wanted.is_empty() {
        std::fs::create_dir_all(&scripts)?;
    }
    if let Ok(dir) = std::fs::read_dir(&scripts) {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !wanted.iter().any(|(f, _)| *f == name) && ours(&e.path()) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    for (file, script) in &wanted {
        let path = scripts.join(file);
        // Never overwrite a script of the user's own with the same name.
        if path.exists() && !ours(&path) {
            continue;
        }
        write_if_changed(&path, script)?;
    }

    // KDE service menus.
    for dir in ["kio/servicemenus", "kservices5/ServiceMenus"] {
        let dir = data.join(dir);
        let path = dir.join(KDE_FILE);
        if targets.is_empty() {
            if ours(&path) {
                let _ = std::fs::remove_file(&path);
            }
            continue;
        }
        let mut actions = String::new();
        let mut body = String::new();
        for (i, (id, name)) in targets.iter().enumerate() {
            actions.push_str(&format!("send{i};"));
            body.push_str(&format!(
                "\n[Desktop Action send{i}]\nName=Send to {}\nIcon=phone\nExec={} send --to {} -- %F\n",
                label(name),
                exec_quote(&exe),
                exec_quote(id)
            ));
        }
        let content = format!(
            "# {MARKER} (made by continuumd; follows the connected phones)\n[Desktop Entry]\nType=Service\nMimeType=application/octet-stream;inode/directory;\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nX-KDE-Priority=TopLevel\nActions={actions}\n{body}"
        );
        if path.exists() && !ours(&path) {
            continue;
        }
        std::fs::create_dir_all(&dir)?;
        write_if_changed(&path, &content)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, name: &str) -> (String, String) {
        (id.into(), name.into())
    }

    #[test]
    fn entries_follow_the_connected_phones() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        let exe = Path::new("/usr/bin/continuumd");
        let scripts = data.join("nautilus/scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(scripts.join("mine.sh"), "#!/bin/sh\necho hi\n").unwrap();

        sync(data, exe, &[t("aa11", "Pixel 8"), t("bb22", "Galaxy/S24")]).unwrap();
        let pixel = std::fs::read_to_string(scripts.join("Send to Pixel 8")).unwrap();
        assert!(pixel.contains("exec '/usr/bin/continuumd' send --to 'aa11' -- \"$@\""));
        assert!(scripts.join("Send to Galaxy-S24").exists());
        let kde = std::fs::read_to_string(data.join("kio/servicemenus").join(KDE_FILE)).unwrap();
        assert!(kde.contains("Actions=send0;send1;"));
        assert!(kde.contains("Name=Send to Pixel 8\n"));
        assert!(kde.contains("Exec=\"/usr/bin/continuumd\" send --to \"aa11\" -- %F"));
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(data.join("kio/servicemenus").join(KDE_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "Plasma 6 needs it executable");

        sync(data, exe, &[t("aa11", "Pixel 8")]).unwrap();
        assert!(!scripts.join("Send to Galaxy-S24").exists());

        sync(data, exe, &[]).unwrap();
        assert!(!scripts.join("Send to Pixel 8").exists());
        assert!(!data.join("kio/servicemenus").join(KDE_FILE).exists());
        assert!(scripts.join("mine.sh").exists(), "user scripts stay");
    }

    #[test]
    fn same_names_and_user_files_are_safe() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        let scripts = data.join("nautilus/scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(scripts.join("Send to Pixel 8"), "my own\n").unwrap();
        sync(
            data,
            Path::new("/x"),
            &[t("aa11", "Pixel 8"), t("bb22", "Pixel 8")],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(scripts.join("Send to Pixel 8")).unwrap(),
            "my own\n"
        );
        assert!(scripts.join("Send to Pixel 8 (bb22)").exists());
    }

    #[test]
    fn quoting() {
        assert_eq!(sh_quote("a'b"), "'a'\\''b'");
        assert_eq!(exec_quote("/a b/$x"), "\"/a b/\\\\$x\"");
        assert_eq!(label("../evil\n"), "-evil");
        assert_eq!(label(""), "phone");
    }
}
