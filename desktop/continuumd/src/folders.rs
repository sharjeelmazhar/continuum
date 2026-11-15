//! Folders sent to a phone go as one `.zip` (stored, not compressed: photos and videos are
//! compressed already, and storing is as fast as copying). The phone keeps files flat in
//! Downloads/Continuum, so a zip is what keeps a folder together.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use zip::write::SimpleFileOptions;

/// Zips older than this are removed at start-up (an unsent job gives up after a day).
const KEEP: Duration = Duration::from_secs(3 * 24 * 3600);

pub fn outgoing_dir() -> PathBuf {
    crate::sysinfo::cache_dir().join("outgoing")
}

/// Deletes old zips left from earlier sends.
pub fn clean_old(root: &Path) {
    let Ok(dir) = std::fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for e in dir.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > KEEP);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Zips `folder` into `root/<unique>/<folder name>.zip` and returns that path. Symbolic links
/// are skipped (never followed out of the folder). Written to a `.part` file first.
pub fn zip_folder(folder: &Path, root: &Path) -> Result<PathBuf, String> {
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "folder".into());
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = root.join(format!("{stamp:x}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(format!("{name}.zip"));
    let part = dir.join(format!(".{name}.zip.part"));
    let file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::with_capacity(1 << 20, file));
    let opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true);
    let mut stack = vec![(folder.to_path_buf(), format!("{name}/"))];
    zip.add_directory(format!("{name}/"), opts)
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 20];
    while let Some((dir_path, prefix)) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir_path)
            .map_err(|e| format!("{}: {e}", dir_path.display()))?
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let Ok(ty) = e.file_type() else { continue };
            let entry_name = format!("{prefix}{}", e.file_name().to_string_lossy());
            if ty.is_dir() {
                zip.add_directory(format!("{entry_name}/"), opts)
                    .map_err(|e| e.to_string())?;
                stack.push((e.path(), format!("{entry_name}/")));
            } else if ty.is_file() {
                let mut src = std::fs::File::open(e.path())
                    .map_err(|err| format!("{}: {err}", e.path().display()))?;
                let opts = match e.metadata().ok().and_then(|m| m.modified().ok()) {
                    Some(t) => opts.last_modified_time(zip_time(t)),
                    None => opts,
                };
                zip.start_file(entry_name, opts)
                    .map_err(|e| e.to_string())?;
                loop {
                    let n = std::io::Read::read(&mut src, &mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    zip.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    let mut out = zip.finish().map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// A file time as a zip time (UTC; zip has no time zone, and can't go before 1980).
fn zip_time(t: SystemTime) -> zip::DateTime {
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
    zip::DateTime::from_date_and_time(
        year,
        month,
        day,
        (rem / 3600) as u8,
        (rem / 60 % 60) as u8,
        (rem % 60) as u8,
    )
    .unwrap_or_default()
}

/// Replaces each folder in `paths` with a zip of it (files stay as they are).
pub fn expand(paths: Vec<PathBuf>, root: &Path) -> Result<Vec<PathBuf>, String> {
    paths
        .into_iter()
        .map(|p| {
            if p.is_dir() {
                zip_folder(&p, root)
            } else {
                Ok(p)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_becomes_a_zip_with_its_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("Holiday");
        std::fs::create_dir_all(src.join("day 2")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        std::fs::write(src.join("day 2/b.jpg"), vec![7u8; 3 << 20]).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", src.join("link")).unwrap();
        let out = expand(vec![src.clone()], &tmp.path().join("out")).unwrap();
        assert!(out[0].ends_with("Holiday.zip"));
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out[0]).unwrap()).unwrap();
        let mut names: Vec<String> = z.file_names().map(|n| n.unwrap().into_owned()).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "Holiday/",
                "Holiday/a.txt",
                "Holiday/day 2/",
                "Holiday/day 2/b.jpg"
            ]
        );
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name("Holiday/a.txt").unwrap(), &mut s).unwrap();
        assert_eq!(s, "hello");
        assert_eq!(z.by_name("Holiday/day 2/b.jpg").unwrap().size(), 3 << 20);
        // No leftover temporary file.
        let dir = out[0].parent().unwrap();
        assert_eq!(std::fs::read_dir(dir).unwrap().count(), 1);
    }

    #[test]
    fn zip_times() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_705_894); // 2026-11-15 01:24:54
        let z = zip_time(t);
        assert_eq!(
            (
                z.year(),
                z.month(),
                z.day(),
                z.hour(),
                z.minute(),
                z.second()
            ),
            (2026, 11, 15, 1, 24, 54)
        );
        let leap = zip_time(SystemTime::UNIX_EPOCH + Duration::from_secs(951_782_400)); // 2000-02-29
        assert_eq!((leap.year(), leap.month(), leap.day()), (2000, 2, 29));
    }

    #[test]
    fn files_pass_through() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("x.mkv");
        std::fs::write(&f, b"v").unwrap();
        assert_eq!(expand(vec![f.clone()], tmp.path()).unwrap(), vec![f]);
    }
}
