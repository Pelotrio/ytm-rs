//! Romanization and English translation of lyrics, the way better-lyrics does it:
//! Google Translate's keyless `gtx` endpoint, one request per song for each, cached
//! on disk by video ID. Songs already in Latin script cost no requests at all.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Same API under two client IDs. Google sometimes answers the first with a 429
/// "automated queries" page for a while; the Chrome dictionary extension's client
/// keeps working when it does.
const ENDPOINTS: [(&str, &str); 2] = [
    ("https://translate.googleapis.com/translate_a/single", "gtx"),
    ("https://clients5.google.com/translate_a/single", "dict-chrome-ex"),
];
const TARGET: &str = "en";

/// Per-line extras, index-aligned with `Lyrics::lines`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Extras {
    pub roman: Vec<Option<String>>,
    pub trans: Vec<Option<String>>,
}

pub struct Client {
    http: reqwest::Client,
    dir: PathBuf,
}

impl Client {
    pub fn new(http: reqwest::Client, data_dir: &Path) -> Self {
        Self { http, dir: data_dir.join("translations") }
    }

    pub async fn extras(&self, video_id: &str, lines: &[String]) -> Option<Extras> {
        let file = self.dir.join(format!("{video_id}.json"));
        if let Some(e) = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Extras>(&b).ok()) {
            if e.roman.len() == lines.len() {
                return Some(e);
            }
        }
        // Distinct non-empty lines only; choruses repeat.
        let mut uniq: Vec<&str> = Vec::new();
        for l in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty() && *l != "♪") {
            if !uniq.contains(&l) {
                uniq.push(l);
            }
        }
        if !uniq.iter().any(|l| non_latin(l)) {
            return None;
        }
        let (roman, trans) = tokio::join!(self.romanize(&uniq), self.translate(&uniq));
        let roman = roman.inspect_err(|e| log::warn!("romanization failed: {e:#}")).ok();
        let trans = trans.inspect_err(|e| log::warn!("translation failed: {e:#}")).ok().flatten();
        if roman.is_none() && trans.is_none() {
            return None;
        }
        let pick = |table: &Option<Vec<String>>, l: &str, only_non_latin: bool| {
            let t = table.as_ref()?;
            let i = uniq.iter().position(|u| *u == l.trim())?;
            let out = t.get(i)?.trim();
            (!out.is_empty() && !out.eq_ignore_ascii_case(l.trim()) && (!only_non_latin || non_latin(l))).then(|| out.to_owned())
        };
        let e = Extras {
            roman: lines.iter().map(|l| pick(&roman, l, true)).collect(),
            trans: lines.iter().map(|l| pick(&trans, l, false)).collect(),
        };
        // Only cache complete results, so a failed half is retried next time.
        if roman.is_some() && trans.is_some() {
            let _ = std::fs::create_dir_all(&self.dir);
            let _ = std::fs::write(&file, serde_json::to_vec(&e).unwrap_or_default());
        }
        Some(e)
    }

    async fn call(&self, sl: &str, dt: &[&str], text: &str) -> anyhow::Result<Value> {
        let mut last = None;
        for (url, client) in ENDPOINTS {
            let mut query = vec![("client", client), ("sl", sl), ("tl", TARGET)];
            query.extend(dt.iter().map(|d| ("dt", *d)));
            let resp = self.http.post(url).query(&query).form(&[("q", text)]).timeout(Duration::from_secs(15)).send().await?;
            match resp.error_for_status() {
                Ok(r) => return Ok(r.json().await?),
                Err(e) => last = Some(e),
            }
        }
        Err(last.expect("at least one endpoint").into())
    }

    /// Romanization comes back as one run-on string (newlines are dropped), so lines
    /// are joined with a numeric separator that survives, like better-lyrics does.
    async fn romanize(&self, lines: &[&str]) -> anyhow::Result<Vec<String>> {
        let mut sep = "0000".to_owned();
        while lines.iter().any(|l| l.contains(&sep)) {
            sep.push('0');
        }
        let v = self.call("auto", &["rm"], &lines.join(&format!(" {sep} "))).await?;
        let rm = v[0]
            .as_array()
            .and_then(|segs| segs.iter().find_map(|s| s[3].as_str()))
            .ok_or_else(|| anyhow::anyhow!("no romanization in response"))?;
        let out: Vec<String> = rm.split(sep.as_str()).map(|s| s.trim().to_owned()).collect();
        anyhow::ensure!(out.len() == lines.len(), "romanization split into {} parts, expected {}", out.len(), lines.len());
        Ok(out)
    }

    /// `None` when the song is already in English. Lines are joined with newlines,
    /// which translation keeps (a separator token gets merged into nearby numbers).
    async fn translate(&self, lines: &[&str]) -> anyhow::Result<Option<Vec<String>>> {
        let v = self.call("auto", &["t"], &lines.join("\n")).await?;
        if v[2].as_str().is_some_and(|lang| lang == TARGET) {
            return Ok(None);
        }
        let text: String = v[0].as_array().into_iter().flatten().filter_map(|s| s[0].as_str()).collect();
        let out: Vec<String> = text.split('\n').map(|s| s.trim().to_owned()).collect();
        anyhow::ensure!(out.len() == lines.len(), "translation split into {} parts, expected {}", out.len(), lines.len());
        Ok(Some(out))
    }
}

/// Whether a line has letters outside the Latin script (kana, hangul, Cyrillic, ...).
pub fn non_latin(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic() && !matches!(c as u32, 0..=0x024F | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F | 0xA720..=0xA7FF | 0xFF21..=0xFF5A))
}
