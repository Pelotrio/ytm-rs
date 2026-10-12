//! The listener's lyrics settings, the same ones the better-lyrics extension keeps:
//! source order (some switched off) with a per-song pick, translation language, and
//! timing offsets. Also the smoothed clock the lyrics follow.

use std::collections::HashMap;

use crate::lyrics::{Line, Provider};

/// Offsets are seconds taken off the lyrics clock: positive shows lyrics later.
pub struct Prefs {
    pub order: Vec<(Provider, bool)>,
    pub target: String,
    pub global_offset: f32,
    pub word_trim: f32,
    pub line_trim: f32,
    /// "videoId|source" → offset.
    pub song_offsets: HashMap<String, f32>,
    /// videoId → source picked for that song.
    pub pins: HashMap<String, Provider>,
}

/// Languages offered for translation.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Spanish"),
    ("pt", "Portuguese"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("pl", "Polish"),
    ("tr", "Turkish"),
    ("ar", "Arabic"),
    ("hi", "Hindi"),
    ("id", "Indonesian"),
    ("vi", "Vietnamese"),
    ("th", "Thai"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh-CN", "Chinese (Simplified)"),
    ("zh-TW", "Chinese (Traditional)"),
];

impl Prefs {
    pub fn load(get: &dyn Fn(&str) -> Option<String>) -> Self {
        // Stored like the extension's `preferredProviderList`: keys in order, "d_" = off;
        // sources it doesn't list are merged in next to their default neighbours.
        let mut order: Vec<(Provider, bool)> = get("lyric_providers")
            .unwrap_or_default()
            .split(',')
            .filter_map(|k| {
                let (k, on) = match k.strip_prefix("d_") {
                    Some(k) => (k, false),
                    None => (k, true),
                };
                Provider::from_key(k).map(|p| (p, on))
            })
            .collect();
        for (di, p) in Provider::ALL.iter().enumerate() {
            if order.iter().any(|(q, _)| q == p) {
                continue;
            }
            let at = Provider::ALL[..di]
                .iter()
                .rev()
                .find_map(|prev| order.iter().position(|(q, _)| q == prev).map(|i| i + 1))
                .or_else(|| Provider::ALL[di + 1..].iter().find_map(|next| order.iter().position(|(q, _)| q == next)))
                .unwrap_or(order.len());
            order.insert(at, (*p, true));
        }
        let num = |k: &str| get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        Self {
            order,
            target: get("lyric_lang").filter(|l| !l.is_empty()).unwrap_or_else(|| "en".into()),
            global_offset: num("lyric_offset"),
            word_trim: num("lyric_trim_word"),
            line_trim: num("lyric_trim_line"),
            song_offsets: get("lyric_song_offsets").and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
            pins: get("lyric_pins")
                .and_then(|s| serde_json::from_str::<HashMap<String, String>>(&s).ok())
                .map(|m| m.into_iter().filter_map(|(k, v)| Provider::from_key(&v).map(|p| (k, p))).collect())
                .unwrap_or_default(),
        }
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        let order: Vec<String> = self.order.iter().map(|(p, on)| if *on { p.key().to_owned() } else { format!("d_{}", p.key()) }).collect();
        storage.set_string("lyric_providers", order.join(","));
        storage.set_string("lyric_lang", self.target.clone());
        storage.set_string("lyric_offset", format!("{:.1}", self.global_offset));
        storage.set_string("lyric_trim_word", format!("{:.1}", self.word_trim));
        storage.set_string("lyric_trim_line", format!("{:.1}", self.line_trim));
        storage.set_string("lyric_song_offsets", serde_json::to_string(&self.song_offsets).unwrap_or_default());
        let pins: HashMap<&String, &str> = self.pins.iter().map(|(k, p)| (k, p.key())).collect();
        storage.set_string("lyric_pins", serde_json::to_string(&pins).unwrap_or_default());
    }

    /// Enabled sources in priority order, the song's pick first (`orderByPin`).
    pub fn order_for(&self, video_id: &str) -> Vec<Provider> {
        let mut out: Vec<Provider> = self.order.iter().filter(|(_, on)| *on).map(|(p, _)| *p).collect();
        if let Some(pin) = self.pins.get(video_id) {
            out.retain(|p| p != pin);
            out.insert(0, *pin);
        }
        out
    }

    pub fn song_offset(&self, video_id: &str, source: Provider) -> f32 {
        self.song_offsets.get(&format!("{video_id}|{}", source.key())).copied().unwrap_or(0.0)
    }

    pub fn set_song_offset(&mut self, video_id: &str, source: Provider, v: f32) {
        let k = format!("{video_id}|{}", source.key());
        let v = (v * 10.0).round() / 10.0;
        if v == 0.0 {
            self.song_offsets.remove(&k);
        } else {
            self.song_offsets.insert(k, v);
        }
    }

    /// Seconds to take off the clock for this song and source, as the extension does
    /// (`globalLyricOffset + lyricOffset` and the per-sync-type trim).
    pub fn offset(&self, video_id: &str, source: Provider, richsync: bool) -> f64 {
        (self.global_offset + self.song_offset(video_id, source) + if richsync { self.word_trim } else { self.line_trim }) as f64
    }
}

/// `findBestLanguageMatch`: a translation shipped with the lyrics in the listener's language.
pub fn shipped_translation<'a>(l: &'a Line, target: &str) -> Option<&'a str> {
    let base = |s: &str| s.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase();
    l.translations
        .iter()
        .find(|(lang, _)| lang.eq_ignore_ascii_case(target))
        .or_else(|| l.translations.iter().find(|(lang, _)| base(lang) == base(target)))
        .map(|(_, t)| t.as_str())
}

/// The lyrics clock: the audio position smoothed so it never steps backwards while
/// playing (the output device takes audio in ~40 ms blocks, so the raw position jumps).
#[derive(Default)]
pub struct Clock {
    shown: f64,
    wall: f64,
    playing: bool,
}

impl Clock {
    pub fn tick(&mut self, pos: f64, playing: bool, now: f64) -> f64 {
        if !playing || !self.playing || self.wall == 0.0 {
            self.shown = pos;
        } else {
            let dt = (now - self.wall).max(0.0);
            let predicted = self.shown + dt;
            let err = pos - predicted;
            if err.abs() > 0.25 {
                self.shown = pos;
            } else {
                // Ease toward the audio, never backwards.
                self.shown = (predicted + err * (dt * 6.0).min(1.0)).max(self.shown);
            }
        }
        self.wall = now;
        self.playing = playing;
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_order_merges_new_sources_and_keeps_disabled() {
        let get = |k: &str| (k == "lyric_providers").then(|| "lrclib-synced,d_bLyrics-richsynced".to_owned());
        let p = Prefs::load(&get);
        assert_eq!(p.order.len(), Provider::ALL.len());
        let i = |q: Provider| p.order.iter().position(|(x, _)| *x == q).unwrap();
        assert!(i(Provider::LrclibSynced) < i(Provider::BLyricsRich));
        assert!(!p.order[i(Provider::BLyricsRich)].1);
        assert!(!p.order_for("x").contains(&Provider::BLyricsRich));
    }

    #[test]
    fn clock_never_steps_back_while_playing() {
        let mut c = Clock::default();
        let a = c.tick(10.0, true, 100.0);
        let b = c.tick(10.0, true, 100.016);
        let d = c.tick(9.99, true, 100.032);
        assert!(b >= a && d >= b);
        // A seek snaps.
        assert_eq!(c.tick(50.0, true, 100.05), 50.0);
    }
}
