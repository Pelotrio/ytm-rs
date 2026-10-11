//! Automatic updates from GitHub releases.
//!
//! The installed app (Inno Setup, per-user) checks the latest release at startup and
//! every few hours, downloads a newer installer in the background, and runs it
//! silently when the app closes, or right away when the user picks "Restart to
//! update". Dev builds (not installed) never update themselves.
//!
//! Dev hooks: `YTM_UPDATE_FORCE=1` treats any build as installed; `YTM_UPDATE_AS=<ver>`
//! pretends to be that version; `YTM_UPDATE_DRYRUN=1` logs the install command
//! instead of running it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const LATEST: &str = "https://api.github.com/repos/grapedjuice/ytm-rs/releases/latest";

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Downloading { version: String, percent: u8 },
    Ready { version: String, installer: PathBuf },
    Failed(String),
}

#[derive(Clone)]
pub struct Updater {
    state: Arc<Mutex<State>>,
    dir: PathBuf,
    wake: Arc<dyn Fn() + Send + Sync>,
}

pub fn current_version() -> String {
    std::env::var("YTM_UPDATE_AS").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned())
}

/// Whether this copy was installed by the installer (which leaves its uninstaller
/// beside the exe), so it's safe to update in place.
pub fn installed() -> bool {
    std::env::var_os("YTM_UPDATE_FORCE").is_some()
        || std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("unins000.exe").exists())).unwrap_or(false)
}

/// `a` is newer than `b` (dotted numeric versions, a leading "v" ignored).
pub fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| v.trim_start_matches('v').split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (a, b) = (parse(a), parse(b));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

impl Updater {
    pub fn new(data_dir: &Path, wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self { state: Arc::new(Mutex::new(State::Idle)), dir: data_dir.join("updates"), wake: Arc::new(wake) }
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, s: State) {
        *self.state.lock().unwrap() = s;
        (self.wake)();
    }

    /// Check for a newer release and download it, in the background.
    pub fn check(&self) {
        if matches!(self.state(), State::Checking | State::Downloading { .. } | State::Ready { .. }) {
            return;
        }
        self.set(State::Checking);
        let this = self.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => return this.set(State::Failed(e.to_string())),
            };
            match rt.block_on(this.run()) {
                Ok(s) => this.set(s),
                Err(e) => {
                    log::warn!("update check failed: {e:#}");
                    this.set(State::Failed(format!("{e:#}")));
                }
            }
        });
    }

    async fn run(&self) -> anyhow::Result<State> {
        let http = reqwest::Client::builder().user_agent(concat!("ytm-rs/", env!("CARGO_PKG_VERSION"))).build()?;
        let release: serde_json::Value = http.get(LATEST).send().await?.error_for_status()?.json().await?;
        let tag = release["tag_name"].as_str().unwrap_or_default();
        let current = current_version();
        if !newer(tag, &current) {
            log::info!("update: {current} is current (latest {tag})");
            return Ok(State::UpToDate);
        }
        let version = tag.trim_start_matches('v').to_owned();
        let want = format!("ytm-rs-setup-{version}.exe");
        let asset = release["assets"]
            .as_array()
            .and_then(|a| a.iter().find(|x| x["name"] == want.as_str()))
            .ok_or_else(|| anyhow::anyhow!("release {tag} has no {want}"))?;
        let url = asset["browser_download_url"].as_str().ok_or_else(|| anyhow::anyhow!("asset has no URL"))?;
        let size = asset["size"].as_u64().unwrap_or(0);
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(&want);
        if std::fs::metadata(&path).map(|m| m.len()) .ok() != Some(size) || size == 0 {
            log::info!("update: downloading {version} ({size} bytes)");
            self.set(State::Downloading { version: version.clone(), percent: 0 });
            let mut resp = http.get(url).send().await?.error_for_status()?;
            let mut bytes = Vec::with_capacity(size as usize);
            while let Some(chunk) = resp.chunk().await? {
                bytes.extend_from_slice(&chunk);
                if size > 0 {
                    let percent = (bytes.len() as u64 * 100 / size).min(100) as u8;
                    *self.state.lock().unwrap() = State::Downloading { version: version.clone(), percent };
                    (self.wake)();
                }
            }
            if size > 0 && bytes.len() as u64 != size {
                anyhow::bail!("download incomplete ({} of {size} bytes)", bytes.len());
            }
            // Write beside, then rename, so a half-written file is never taken as ready.
            let part = path.with_extension("part");
            std::fs::write(&part, &bytes)?;
            std::fs::rename(&part, &path)?;
        }
        // Old installers are no longer needed.
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                if e.path() != path {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        log::info!("update: {version} ready at {}", path.display());
        Ok(State::Ready { version, installer: path })
    }

    /// Run the downloaded installer silently once this process has exited, then
    /// (if `relaunch`) start the updated app.
    pub fn install(&self, relaunch: bool) {
        let State::Ready { installer, .. } = self.state() else { return };
        let exe = std::env::current_exe().unwrap_or_default();
        // Update the copy that's running: an all-users install lives under Program Files
        // (and the installer will ask for admin rights); otherwise it's per-user.
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
        let scope = if program_files.is_some_and(|p| exe.starts_with(p)) { "/ALLUSERS" } else { "/CURRENTUSER" };
        // Give this process a moment to save and exit before the installer replaces it.
        let mut script = format!(
            "ping -n 3 127.0.0.1 >NUL & \"{}\" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART {scope}",
            installer.display()
        );
        if relaunch {
            script += &format!(" & start \"\" \"{}\"", exe.display());
        }
        if std::env::var_os("YTM_UPDATE_DRYRUN").is_some() {
            log::info!("update (dry run): cmd /C {script}");
            return;
        }
        log::info!("update: installing {}", installer.display());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            if let Err(e) = std::process::Command::new("cmd").arg("/C").raw_arg(&script).creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS).spawn() {
                log::warn!("update: couldn't start installer: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn version_order() {
        assert!(newer("v0.2.8", "0.2.7"));
        assert!(newer("v0.10.0", "0.9.9"));
        assert!(newer("1.0", "0.99.99"));
        assert!(!newer("v0.2.7", "0.2.7"));
        assert!(!newer("v0.2.6", "0.2.7"));
        assert!(!newer("", "0.2.7"));
    }
}
