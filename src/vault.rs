//! Secrets (the Google and Spotify sign-ins) in the OS credential store: Windows
//! Credential Manager, encrypted to the signed-in Windows user. Never in a plain file.
//!
//! A credential holds at most 2,560 bytes and a Google cookie is close to that, so a
//! secret is split over numbered entries: `name`, `name #2`, `name #3`, …
//! Other systems have no store wired up and keep the secret in the data folder.

use std::path::Path;

const SERVICE: &str = "ytm-rs";

/// The credential names for `name`, scoped to the data folder when it isn't the
/// default one (`YTM_DATA_DIR` dev profiles, tests), so those never share or touch
/// the real sign-ins.
fn key(name: &str, data_dir: &Path) -> String {
    #[cfg(not(windows))]
    let _ = DIR.set(data_dir.to_owned());
    let default = directories::ProjectDirs::from("", "", "ytm-rs").map(|d| d.data_dir().to_path_buf());
    if default.as_deref() == Some(data_dir) {
        name.to_owned()
    } else {
        format!("{name} ({})", data_dir.display())
    }
}

#[cfg(windows)]
mod store {
    /// Credential Manager's limit on one secret.
    const CHUNK: usize = 2560;

    fn entry(key: &str, part: usize) -> keyring::Result<keyring::Entry> {
        let user = if part == 0 { key.to_owned() } else { format!("{key} #{}", part + 1) };
        keyring::Entry::new(super::SERVICE, &user)
    }

    pub fn get(key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let mut out = Vec::new();
        for part in 0.. {
            match entry(key, part)?.get_secret() {
                Ok(bytes) => {
                    let full = bytes.len() == CHUNK;
                    out.extend(bytes);
                    if !full {
                        break;
                    }
                }
                Err(keyring::Error::NoEntry) if part == 0 => return Ok(None),
                Err(keyring::Error::NoEntry) => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Some(out))
    }

    pub fn set(key: &str, secret: &[u8]) -> anyhow::Result<()> {
        // A secret of an exact multiple of CHUNK ends with an empty part, so reading knows where it stops.
        let mut parts: Vec<&[u8]> = secret.chunks(CHUNK).collect();
        if secret.len() % CHUNK == 0 {
            parts.push(&[]);
        }
        for (i, part) in parts.iter().enumerate() {
            entry(key, i)?.set_secret(part)?;
        }
        delete_from(key, parts.len())
    }

    pub fn delete(key: &str) -> anyhow::Result<()> {
        delete_from(key, 0)
    }

    /// Delete parts `first..` (left over from a longer secret).
    fn delete_from(key: &str, first: usize) -> anyhow::Result<()> {
        for part in first.. {
            match entry(key, part)?.delete_credential() {
                Ok(()) => {}
                Err(keyring::Error::NoEntry) => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod store {
    use std::path::PathBuf;

    /// No store here: the secret is a file named after its key, in the data folder.
    fn path(key: &str) -> PathBuf {
        let safe: String = key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        super::DIR.get().cloned().unwrap_or_default().join(format!("{safe}.secret"))
    }

    pub fn get(key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        match std::fs::read(path(key)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set(key: &str, secret: &[u8]) -> anyhow::Result<()> {
        Ok(std::fs::write(path(key), secret)?)
    }

    pub fn delete(key: &str) -> anyhow::Result<()> {
        match std::fs::remove_file(path(key)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(not(windows))]
static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// The secret saved under `name`, if any.
pub fn get(name: &str, data_dir: &Path) -> Option<Vec<u8>> {
    store::get(&key(name, data_dir)).unwrap_or_else(|e| {
        log::warn!("couldn't read {name} from the credential store: {e:#}");
        None
    })
}

pub fn set(name: &str, data_dir: &Path, secret: &[u8]) -> anyhow::Result<()> {
    store::set(&key(name, data_dir), secret)
}

pub fn delete(name: &str, data_dir: &Path) {
    if let Err(e) = store::delete(&key(name, data_dir)) {
        log::warn!("couldn't delete {name} from the credential store: {e:#}");
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn round_trip_across_chunks() {
        let key = "ytm-rs vault test";
        for len in [0, 10, 2560, 2561, 6000, 100] {
            let secret: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            super::store::set(key, &secret).unwrap();
            assert_eq!(super::store::get(key).unwrap().unwrap(), secret, "{len} bytes");
        }
        super::store::delete(key).unwrap();
        assert!(super::store::get(key).unwrap().is_none());
    }
}
