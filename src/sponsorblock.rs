//! SponsorBlock (sponsor.ajay.app): parts of music videos the community marked as not
//! music (talking, skits, a scene before the song starts) or as sponsor and
//! self-promotion breaks, to skip during playback. Album songs have none; music videos
//! often do.

use sha2::{Digest, Sha256};

const API: &str = "https://sponsor.ajay.app/api/skipSegments";
const CATEGORIES: &str = r#"["music_offtopic","sponsor","selfpromo","interaction"]"#;

/// Seconds into the video.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
}

#[derive(serde::Deserialize)]
struct Video {
    #[serde(rename = "videoID")]
    video_id: String,
    segments: Vec<Raw>,
}

#[derive(serde::Deserialize)]
struct Raw {
    segment: [f64; 2],
    #[serde(rename = "actionType")]
    action: String,
    /// Length of the video the segment was marked on; 0 when unknown.
    #[serde(rename = "videoDuration", default)]
    duration: f64,
}

/// The parts of a video to skip, in order. Asked for by the first four hex digits of
/// the ID's SHA-256, as the extension does, so the server never sees which song plays.
/// Segments marked on a cut of a different length than `duration` are left out.
pub async fn segments(http: &reqwest::Client, video_id: &str, duration: Option<f64>) -> anyhow::Result<Vec<Segment>> {
    let hash = Sha256::digest(video_id.as_bytes());
    let url = url::Url::parse_with_params(&format!("{API}/{:02x}{:02x}", hash[0], hash[1]), [("categories", CATEGORIES), ("actionType", "skip")])?;
    let resp = http.get(url).send().await?;
    // No video with that prefix has any.
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let videos: Vec<Video> = resp.error_for_status()?.json().await?;
    let mut out: Vec<Segment> = videos
        .into_iter()
        .filter(|v| v.video_id == video_id)
        .flat_map(|v| v.segments)
        .filter(|s| s.action == "skip" && s.segment[1] > s.segment[0] + 0.5)
        .filter(|s| match duration {
            Some(d) if s.duration > 0.0 => (s.duration - d).abs() < 3.0,
            _ => true,
        })
        .map(|s| Segment { start: s.segment[0], end: s.segment[1] })
        .collect();
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    Ok(out)
}
