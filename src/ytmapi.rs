//! YouTube Music's own InnerTube API, as music.youtube.com calls it (the `WEB_REMIX`
//! client): request context, signed-in or anonymous headers, visitor data and the
//! current client version. Parsing lives in `ytm.rs`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::webmusic::{self, Session};

const ORIGIN: &str = "https://music.youtube.com";
/// Used until the live version is read from music.youtube.com.
const FALLBACK_VERSION: &str = "1.20261006.10.00";
/// How long a scraped client version (and visitor ID) is trusted before re-reading it.
const REFRESH: Duration = Duration::from_secs(24 * 3600);
/// Accepts YouTube's cookie consent, so anonymous requests get content, not a consent page.
const CONSENT: &str = "SOCS=CAISAiAD";

/// Client version and visitor ID read from music.youtube.com, cached on disk.
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
struct Web {
    version: String,
    visitor_data: String,
    /// Unix seconds when read.
    at: u64,
}

pub struct Api {
    http: reqwest::Client,
    file: PathBuf,
    web: tokio::sync::Mutex<Option<Web>>,
    account: RwLock<Option<Arc<Session>>>,
    /// IANA time zone and its current UTC offset in minutes.
    tz: (String, i32),
}

impl Api {
    pub fn new(http: reqwest::Client, data_dir: &Path) -> Self {
        let file = data_dir.join("ytmapi.json");
        let web = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Web>(&b).ok());
        Self {
            http,
            file,
            web: tokio::sync::Mutex::new(web),
            account: RwLock::new(Session::load(data_dir).map(Arc::new)),
            tz: local_tz(),
        }
    }

    pub fn account(&self) -> Option<Arc<Session>> {
        self.account.read().unwrap().clone()
    }

    pub fn set_account(&self, s: Option<Session>) {
        *self.account.write().unwrap() = s.map(Arc::new);
    }

    /// The cached client version and visitor ID, read from music.youtube.com when
    /// missing, a day old, or `fresh` (a request was refused with the current ones).
    async fn web(&self, fresh: bool) -> Web {
        let mut slot = self.web.lock().await;
        let now = now_secs();
        let stale = slot.as_ref().is_none_or(|w| fresh || now.saturating_sub(w.at) > REFRESH.as_secs() || w.visitor_data.is_empty());
        if stale {
            match self.read_web().await {
                Ok(w) => {
                    let _ = std::fs::write(&self.file, serde_json::to_vec(&w).unwrap_or_default());
                    *slot = Some(w);
                }
                Err(e) => log::warn!("couldn't read music.youtube.com: {e:#}"),
            }
        }
        slot.clone().unwrap_or_else(|| Web { version: FALLBACK_VERSION.into(), ..Default::default() })
    }

    async fn read_web(&self) -> anyhow::Result<Web> {
        let resp = self
            .http
            .get(ORIGIN)
            .header("User-Agent", webmusic::UA)
            .header("Cookie", CONSENT)
            .timeout(Duration::from_secs(10))
            .send()
            .await?
            .error_for_status()?;
        // The visitor ID also arrives as a cookie; the page's ytcfg has it too.
        let cookie = resp.headers().get_all("set-cookie").iter().find_map(|c| {
            let v = c.to_str().ok()?.strip_prefix("__Secure-YEC=")?.split(';').next()?;
            (!v.is_empty()).then(|| v.to_owned())
        });
        let html = resp.text().await?;
        let visitor_data = cookie
            .or_else(|| between(&html, "\"VISITOR_DATA\":\"", "\"").map(str::to_owned))
            .ok_or_else(|| anyhow::anyhow!("no visitor data on the page"))?;
        let version = between(&html, "\"INNERTUBE_CLIENT_VERSION\":\"", "\"")
            .or_else(|| between(&html, "\"INNERTUBE_CONTEXT_CLIENT_VERSION\":\"", "\""))
            .unwrap_or(FALLBACK_VERSION)
            .to_owned();
        log::info!("music.youtube.com client {version}");
        Ok(Web { version, visitor_data, at: now_secs() })
    }

    /// An anonymous visitor ID (the visionOS player needs one too).
    pub async fn visitor_data(&self, fresh: bool) -> anyhow::Result<String> {
        let w = self.web(fresh).await;
        anyhow::ensure!(!w.visitor_data.is_empty(), "couldn't get a YouTube visitor ID");
        Ok(w.visitor_data)
    }

    /// POST `body` to `/youtubei/v1/<endpoint>`. `auth`: as the signed-in account, when
    /// there is one; otherwise anonymously as the visitor.
    pub async fn call(&self, endpoint: &str, body: Value, auth: bool) -> anyhow::Result<Value> {
        let account = if auth { self.account() } else { None };
        match self.call_once(endpoint, &body, account.as_deref(), false).await {
            // A refused anonymous request is usually a stale visitor ID or client version.
            Err(e) if account.is_none() && is_retryable(&e) => {
                log::info!("{endpoint}: {e:#}; retrying with a fresh visitor ID");
                self.call_once(endpoint, &body, None, true).await
            }
            Err(e) if account.is_some() && is_refused(&e) => self.retry_signed_in(endpoint, &body, account.as_deref(), e).await,
            r => r,
        }
    }

    /// Like `call`, but a signed-in account is required.
    pub async fn call_signed_in(&self, endpoint: &str, body: Value) -> anyhow::Result<Value> {
        let account = self.account().ok_or_else(|| anyhow::anyhow!("sign in first"))?;
        match self.call_once(endpoint, &body, Some(&account), false).await {
            Err(e) if is_refused(&e) => self.retry_signed_in(endpoint, &body, Some(&account), e).await,
            r => r,
        }
    }

    /// YouTube now and then refuses a signed-in request (403) and takes the same one a
    /// moment later. A refused request changed nothing, so it's safe to send again.
    async fn retry_signed_in(&self, endpoint: &str, body: &Value, account: Option<&Session>, e: anyhow::Error) -> anyhow::Result<Value> {
        log::info!("{endpoint}: {e:#}; trying again");
        tokio::time::sleep(Duration::from_millis(700)).await;
        self.call_once(endpoint, body, account, false).await
    }

    async fn call_once(&self, endpoint: &str, body: &Value, account: Option<&Session>, fresh: bool) -> anyhow::Result<Value> {
        let web = self.web(fresh).await;
        let mut payload = body.clone();
        payload["context"] = json!({
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": web.version,
                "platform": "DESKTOP",
                "originalUrl": ORIGIN,
                "visitorData": web.visitor_data,
                "hl": "en",
                "gl": "US",
                "timeZone": self.tz.0,
                "utcOffsetMinutes": self.tz.1,
            },
            "request": {"useSsl": true},
            "user": {"lockedSafetyMode": false},
        });
        let mut req = self
            .http
            .post(format!("{ORIGIN}/youtubei/v1/{endpoint}?prettyPrint=false"))
            .header("User-Agent", webmusic::UA)
            .header("Origin", ORIGIN)
            .header("X-Origin", ORIGIN)
            .header("Referer", ORIGIN)
            .header("X-YouTube-Client-Name", "67")
            .header("X-YouTube-Client-Version", &web.version)
            .timeout(Duration::from_secs(15));
        if !web.visitor_data.is_empty() {
            req = req.header("X-Goog-Visitor-Id", &web.visitor_data);
        }
        req = match account {
            Some(a) => a.sign(req, ORIGIN)?,
            None => req.header("Cookie", CONSENT),
        };
        let resp = req.json(&payload).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"]["message"].as_str().map(str::to_owned));
            anyhow::bail!(ApiError { status: status.as_u16(), msg: msg.unwrap_or_default() });
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// Check a signed-in cookie with YouTube and read which account/channel it is for.
    pub async fn session_from_cookie(&self, cookie: &str) -> anyhow::Result<Session> {
        let html = self
            .http
            .get("https://www.youtube.com/results?search_query=")
            .header("User-Agent", webmusic::UA)
            .header("Cookie", cookie)
            .timeout(Duration::from_secs(15))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        anyhow::ensure!(html.contains("\"USER_SESSION_ID\":\""), "YouTube didn't recognise a signed-in session in those cookies");
        // `datasyncId` is "<page>||<user>" for a brand channel and "<user>||" for the
        // account's own channel; requests for a brand channel name it in X-Goog-PageId.
        let page_id = between(&html, "\"datasyncId\":\"", "\"").and_then(|d| {
            let (page, user) = d.split_once("||")?;
            (!user.is_empty()).then(|| page.to_owned())
        });
        let session_index = between(&html, "\"SESSION_INDEX\":\"", "\"").and_then(|s| s.parse().ok()).unwrap_or(0);
        Ok(Session::new(cookie.to_owned(), session_index, page_id))
    }
}

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub msg: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.status, self.msg.is_empty()) {
            (401, _) => f.write_str("YouTube Music signed this session out; sign in again"),
            (_, true) => write!(f, "YouTube Music returned HTTP {}", self.status),
            (_, false) => write!(f, "YouTube Music returned HTTP {}: {}", self.status, self.msg),
        }
    }
}

impl std::error::Error for ApiError {}

fn is_retryable(e: &anyhow::Error) -> bool {
    match e.downcast_ref::<ApiError>() {
        Some(a) => a.status == 400 || a.status == 403 || a.status == 429 || a.status >= 500,
        None => e.downcast_ref::<reqwest::Error>().is_some_and(|r| r.is_timeout() || r.is_connect()),
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The text between `start` and the next `end`.
fn is_refused(e: &anyhow::Error) -> bool {
    e.downcast_ref::<ApiError>().is_some_and(|a| a.status == 403)
}

pub fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)?;
    Some(&s[i..i + j])
}

/// The system's IANA time zone and current UTC offset, sent with every request the
/// way the web client does (YouTube Music uses them for "Today", history and charts).
fn local_tz() -> (String, i32) {
    let name = localzone::get_local_zone().unwrap_or_else(|| "UTC".into());
    (name, utc_offset_minutes())
}

#[cfg(windows)]
fn utc_offset_minutes() -> i32 {
    #[repr(C)]
    struct SystemTime([u16; 8]);
    #[repr(C)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
    }
    unsafe extern "system" {
        fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
    }
    // SAFETY: a plain out-parameter struct with the documented layout.
    unsafe {
        let mut info: TimeZoneInformation = std::mem::zeroed();
        match GetTimeZoneInformation(&mut info) {
            1 => -(info.bias + info.standard_bias),
            2 => -(info.bias + info.daylight_bias),
            0 => -info.bias,
            _ => 0,
        }
    }
}

#[cfg(not(windows))]
fn utc_offset_minutes() -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn between_finds_ytcfg_values() {
        let html = r#"ytcfg.set({"INNERTUBE_CLIENT_VERSION":"1.20261006.10.00","VISITOR_DATA":"Cgt4eXo%3D"});"#;
        assert_eq!(between(html, "\"INNERTUBE_CLIENT_VERSION\":\"", "\""), Some("1.20261006.10.00"));
        assert_eq!(between(html, "\"VISITOR_DATA\":\"", "\""), Some("Cgt4eXo%3D"));
        assert_eq!(between(html, "\"MISSING\":\"", "\""), None);
    }
}
