//! On-disk state in `$XDG_DATA_HOME/continuum/`: the identity and the paired devices.
//! Every write is atomic (temp file, fsync, rename, fsync dir) so a power cut can't corrupt it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use continuum_proto::identity::Identity;
use continuum_proto::message::DeviceType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: DeviceType,
    /// Pinned certificate fingerprint, lowercase hex.
    pub fp: String,
    pub paired_at: u64,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn open(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        Ok(Self { dir })
    }

    pub fn default_dir() -> PathBuf {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
            });
        base.join("continuum")
    }

    /// Loads the identity, creating one on first run.
    pub fn identity(&self) -> io::Result<Identity> {
        let (cert, key) = (self.dir.join("cert.der"), self.dir.join("key.der"));
        if let (Ok(cert_der), Ok(key_der)) = (fs::read(&cert), fs::read(&key)) {
            return Ok(Identity { cert_der, key_der });
        }
        let id = Identity::generate().map_err(io::Error::other)?;
        // Key first: a cert without its key would be useless, a key without a cert is regenerated.
        write_atomic(&key, &id.key_der)?;
        write_atomic(&cert, &id.cert_der)?;
        Ok(id)
    }

    pub fn load_devices(&self) -> Vec<PairedDevice> {
        match fs::read(self.dir.join("devices.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::error!(
                    "devices.json is unreadable ({e}); starting with no paired devices"
                );
                Vec::new()
            }),
            Err(_) => Vec::new(),
        }
    }

    pub fn save_devices(&self, devices: &[PairedDevice]) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(devices).map_err(io::Error::other)?;
        write_atomic(&self.dir.join("devices.json"), &json)
    }
}

/// Writes `data` to `path` with mode 0600 so that readers see either the old or the new file.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent() {
        File::open(dir)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_created_once_and_private() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().join("c")).unwrap();
        let a = store.identity().unwrap();
        let b = store.identity().unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
        let mode = fs::metadata(tmp.path().join("c/key.der"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn devices_round_trip_and_corrupt_file_is_survivable() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().to_path_buf()).unwrap();
        assert!(store.load_devices().is_empty());
        let d = PairedDevice {
            id: "ab".into(),
            name: "Pixel 8".into(),
            device_type: DeviceType::Phone,
            fp: "00".into(),
            paired_at: 1,
        };
        store.save_devices(std::slice::from_ref(&d)).unwrap();
        assert_eq!(store.load_devices(), vec![d]);
        fs::write(tmp.path().join("devices.json"), b"{half-writ").unwrap();
        assert!(store.load_devices().is_empty());
    }
}
