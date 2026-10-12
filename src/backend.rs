//! Network side: a small tokio runtime talking to YouTube Music (`ytmapi`/`ytm`).
//!
//! The UI sends `Req`s and drains `Resp`s once per frame; every response wakes the UI.
//! Playback resolution also lives here: resolve stream → start progressive download →
//! build the decoder on a blocking thread → hand it to the audio engine.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crossbeam_channel::Sender;
use tokio::runtime::Runtime;

use crate::art::Art;
use crate::audio::{AudioCmd, AudioHandle};
use crate::lyrics::{self, Lyrics};
use crate::model::{self, Card, Target, Thumb, Track, sized};
use crate::stream::{self, Shared};
use crate::{innertube, jsc, webmusic, ytdlp, ytm, ytmapi};

pub enum Req {
    /// Home feed, optionally filtered by a mood chip's params.
    Home(Option<String>),
    HomeMore(String),
    MoreQuickPicks(String),
    Explore(Vec<String>),
    Search(String),
    Suggest(String),
    Album(String),
    Artist(String),
    Playlist(String),
    Library,
    History,
    /// Songs the account uploaded to YouTube Music.
    Uploads,
    /// Upload audio files to the account's library.
    Upload(Vec<PathBuf>),
    Lyrics(Track),
    /// Process cover art for the background; `key` is echoed back.
    Art { key: String, url: String },
    /// Radio seeded from a track; appended to the queue.
    Radio(String),
    /// Fetch an album/playlist and play it from the start.
    PlayCollection { target: Target, shuffle: bool },
    /// `at`: start position (non-zero when resuming last session's song).
    Play { video_id: String, generation: u64, at: std::time::Duration },
    /// Resolve and download a track ahead of time so skipping to it is instant.
    Prefetch(String),
    Like { video_id: String, like: bool },
    Dislike(String),
    CreatePlaylist { title: String, video_id: Option<String> },
    EditPlaylist { id: String, title: String, description: String, privacy: Option<String> },
    AddToPlaylist { playlist_id: String, video_id: String },
    DeletePlaylist(String),
    SaveCollection { id: String, save: bool },
    SetCookie(String),
    Logout,
}

impl Req {
    fn name(&self) -> &'static str {
        match self {
            Req::Home(_) | Req::HomeMore(_) => "Loading home",
            Req::MoreQuickPicks(_) => "Loading more picks",
            Req::Explore(_) => "Loading explore",
            Req::Search(_) => "Search",
            Req::Suggest(_) => "Suggest",
            Req::Album(_) => "Loading album",
            Req::Artist(_) => "Loading artist",
            Req::Playlist(_) => "Loading playlist",
            Req::Library => "Loading library",
            Req::History => "Loading history",
            Req::Uploads => "Loading uploads",
            Req::Upload(_) => "Upload",
            Req::Lyrics(_) => "Lyrics",
            Req::Art { .. } => "Art",
            Req::Radio(_) => "Loading radio",
            Req::PlayCollection { .. } => "Playback",
            Req::Play { .. } => "Playback",
            Req::Prefetch(_) => "Prefetch",
            Req::Like { .. } => "Like",
            Req::Dislike(_) => "Dislike",
            Req::CreatePlaylist { .. } => "Create playlist",
            Req::EditPlaylist { .. } => "Edit playlist",
            Req::AddToPlaylist { .. } => "Add to playlist",
            Req::DeletePlaylist(_) => "Delete playlist",
            Req::SaveCollection { .. } => "Save to library",
            Req::SetCookie(_) => "Sign in",
            Req::Logout => "Sign out",
        }
    }

    /// Background requests fail quietly (logged, no toast).
    fn quiet(&self) -> bool {
        matches!(self, Req::Suggest(_) | Req::Art { .. } | Req::Prefetch(_) | Req::HomeMore(_) | Req::Lyrics(_))
    }
}

pub struct Explore {
    pub new_albums: Vec<Card>,
    pub charts: Vec<Card>,
    pub top: Vec<Track>,
}

pub struct SearchResults {
    pub query: String,
    pub tracks: Vec<Track>,
    pub albums: Vec<Card>,
    pub artists: Vec<Card>,
    pub playlists: Vec<Card>,
}

/// An album or playlist page.
pub struct Collection {
    pub id: String,
    pub is_album: bool,
    /// Album's playlist ID for saving it to the account library.
    pub save_id: Option<String>,
    pub title: String,
    pub subtitle: String,
    /// Album artists / playlist owner named in the subtitle, clickable.
    pub artists: Vec<model::Link>,
    pub description: Option<String>,
    pub thumbs: Vec<Thumb>,
    pub tracks: Vec<Track>,
}

pub struct ArtistPage {
    pub id: String,
    pub name: String,
    pub banner: Vec<Thumb>,
    pub subscribers: Option<u64>,
    pub description: Option<String>,
    pub top: Vec<Track>,
    /// "Top songs" playlist behind the shelf's Show all button.
    pub top_playlist: Option<String>,
    pub albums: Vec<Card>,
    pub playlists: Vec<Card>,
    pub similar: Vec<Card>,
}

pub struct Library {
    pub liked: Vec<Track>,
    pub playlists: Vec<Card>,
    pub albums: Vec<Card>,
}

pub enum Resp {
    Home(ytm::HomePage),
    HomeMore(ytm::HomePage),
    QuickPicksMore { seed: String, tracks: Vec<Track> },
    Explore(Explore),
    Search(SearchResults),
    Suggest { query: String, terms: Vec<String> },
    Collection(Collection),
    Artist(ArtistPage),
    Library(Library),
    History(Vec<Track>),
    Uploads(Vec<Track>),
    /// Upload finished: how many files went up, and which failed.
    Uploaded { done: usize, failed: Vec<String> },
    Lyrics { video_id: String, lyrics: Option<Lyrics> },
    /// Romanization/translation for the lyrics, arriving after them.
    LyricsExtras { video_id: String, extras: crate::translate::Extras },
    Art { key: String, art: Art },
    Radio { seed: String, tracks: Vec<Track> },
    PlayQueue { tracks: Vec<Track> },
    Liked { video_id: String, like: bool },
    PlaylistCreated { id: String, had_song: bool },
    PlaylistChanged { id: Option<String>, message: &'static str },
    /// Playback for `generation` failed before reaching the audio engine.
    PlayError { generation: u64, msg: String },
    LoggedIn(bool),
    Error(String),
}

/// A video id and its stream buffer.
type StreamSlot = Arc<std::sync::Mutex<Option<(String, Arc<Shared>)>>>;

#[derive(Clone)]
pub struct Backend {
    rt: Arc<Runtime>,
    api: Arc<ytmapi::Api>,
    http: reqwest::Client,
    tx: Sender<Resp>,
    audio: AudioHandle,
    wake: Arc<dyn Fn() + Send + Sync>,
    data_dir: PathBuf,
    lyrics: Arc<lyrics::Client>,
    translate: Arc<crate::translate::Client>,
    /// The upcoming track, resolved and downloading ahead of time.
    prefetched: StreamSlot,
    /// The stream most recently started, kept so a replay of a fully downloaded song
    /// (repeat one, restart) doesn't download it again.
    current: StreamSlot,
    jsc: Arc<jsc::Solver>,
    /// Whether the signed-in account has Premium; checked once per sign-in. A failed
    /// check is retried no sooner than `PREMIUM_RETRY` later.
    premium: Arc<tokio::sync::Mutex<Result<bool, Option<std::time::Instant>>>>,
    /// High quality: with Premium, every song goes through the signed-in web player
    /// for its 256 kbps stream instead of the anonymous 128 kbps one.
    hq: Arc<std::sync::atomic::AtomicBool>,
}

impl Backend {
    pub fn new(
        storage: PathBuf,
        tx: Sender<Resp>,
        audio: AudioHandle,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("net")
            .enable_all()
            .build()?;
        std::fs::create_dir_all(&storage)?;
        let data_dir = storage;
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36")
            .build()?;
        let lyrics = Arc::new(lyrics::Client::new(http.clone(), &data_dir));
        let translate = Arc::new(crate::translate::Client::new(http.clone(), &data_dir));
        let jsc = Arc::new(jsc::Solver::new(&data_dir));
        let api = Arc::new(ytmapi::Api::new(http.clone(), &data_dir));
        Ok(Self { rt: Arc::new(rt), api, http, tx, audio, wake, prefetched: Default::default(), current: Default::default(), data_dir, lyrics, translate, jsc, premium: Arc::new(tokio::sync::Mutex::new(Err(None))), hq: Default::default() })
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn spawn<F: Future<Output = ()> + Send + 'static>(&self, f: F) {
        self.rt.spawn(f);
    }

    pub fn wake(&self) {
        (self.wake)()
    }

    pub fn request(&self, req: Req) {
        let (what, quiet) = (req.name(), req.quiet());
        let play_gen = match &req {
            Req::Play { generation, .. } => Some(*generation),
            _ => None,
        };
        let this = self.clone();
        let task = self.rt.spawn(async move { this.handle(req).await });
        let this = self.clone();
        self.rt.spawn(async move {
            // A panic in a handler (or a library it calls) fails just this request,
            // reported like any other error.
            let resp = match task.await {
                Ok(resp) => resp,
                Err(e) if e.is_panic() => {
                    log::error!("{what}: request panicked");
                    match play_gen {
                        Some(generation) => Some(Resp::PlayError { generation, msg: "YouTube didn't respond properly".into() }),
                        None => (!quiet).then(|| Resp::Error(format!("{what} failed: YouTube didn't respond properly"))),
                    }
                }
                Err(_) => None,
            };
            if let Some(resp) = resp {
                let _ = this.tx.send(resp);
                this.wake();
            }
        });
    }

    async fn handle(&self, req: Req) -> Option<Resp> {
        let api = &*self.api;
        // Personalised pages ask as the account when signed in, like music.youtube.com.
        let me = self.is_logged_in();
        let (what, quiet) = (req.name(), req.quiet());
        let res: anyhow::Result<Option<Resp>> = async {
            Ok(Some(match req {
                Req::Home(params) => Resp::Home(ytm::home(api, params.as_deref(), me).await?),
                Req::HomeMore(token) => Resp::HomeMore(ytm::home_more(api, &token, me).await?),
                Req::MoreQuickPicks(seed) => {
                    let tracks = ytm::radio(api, &seed, me, 0).await?;
                    Resp::QuickPicksMore { seed, tracks }
                }
                Req::Explore(preferred_artists) => {
                    let (charts, new) = tokio::join!(ytm::charts(api), ytm::new_albums(api, me));
                    let charts = charts?;
                    Resp::Explore(Explore {
                        new_albums: popular_new_albums(&new.unwrap_or_default(), &charts.artists, &preferred_artists),
                        charts: charts.playlists,
                        top: charts.top,
                    })
                }
                Req::Search(query) => {
                    let (t, al, ar, pl) = tokio::join!(
                        ytm::search_songs(api, &query),
                        ytm::search_cards(api, &query, ytm::Filter::Albums),
                        ytm::search_cards(api, &query, ytm::Filter::Artists),
                        ytm::search_cards(api, &query, ytm::Filter::Playlists),
                    );
                    Resp::Search(SearchResults {
                        tracks: t?,
                        albums: al.unwrap_or_default(),
                        artists: ar.unwrap_or_default(),
                        playlists: pl.unwrap_or_default(),
                        query,
                    })
                }
                Req::Suggest(query) => {
                    let terms = ytm::suggestions(api, &query).await?.into_iter().take(8).collect();
                    Resp::Suggest { query, terms }
                }
                Req::Album(id) => Resp::Collection(ytm::album(api, &id).await?),
                Req::Playlist(id) => Resp::Collection(ytm::playlist(api, &id, me, 500).await?),
                Req::Artist(id) => Resp::Artist(ytm::artist(api, &id).await?),
                Req::Library => {
                    let (liked, pls, albums) =
                        tokio::join!(ytm::playlist(api, "LM", true, 1000), ytm::saved_playlists(api), ytm::saved_albums(api));
                    Resp::Library(Library {
                        liked: liked?.tracks,
                        playlists: pls.map(|p| p.into_iter().filter(saved_playlist_visible).collect()).unwrap_or_default(),
                        albums: albums.unwrap_or_default(),
                    })
                }
                Req::History => Resp::History(ytm::history(api).await?),
                Req::Uploads => Resp::Uploads(ytm::uploads(api).await?),
                Req::Upload(files) => {
                    let session = api.account().ok_or_else(|| anyhow::anyhow!("sign in to upload music"))?;
                    let (mut done, mut failed) = (0, Vec::new());
                    for f in &files {
                        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        match session.upload(&self.http, f).await {
                            Ok(()) => done += 1,
                            Err(e) => {
                                log::warn!("upload {name}: {e:#}");
                                failed.push(name);
                            }
                        }
                    }
                    if done > 0 {
                        // YouTube Music needs a little while to process new uploads.
                        let this = self.clone();
                        self.rt.spawn(async move {
                            for wait in [20, 40, 60] {
                                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                                if let Ok(t) = ytm::uploads(&this.api).await {
                                    let _ = this.tx.send(Resp::Uploads(t));
                                    this.wake();
                                }
                            }
                        });
                    }
                    Resp::Uploaded { done, failed }
                }
                Req::Lyrics(track) => {
                    let video_id = track.id.clone();
                    let lyrics = self.lyrics_for(&track).await;
                    let texts: Option<Vec<String>> = lyrics.as_ref().map(|l| l.lines.iter().map(|l| l.text.clone()).collect());
                    // Show the lyrics straight away; the extras follow.
                    let _ = self.tx.send(Resp::Lyrics { lyrics, video_id: video_id.clone() });
                    self.wake();
                    let Some(texts) = texts else { return Ok(None) };
                    let Some(extras) = self.translate.extras(&video_id, &texts).await else { return Ok(None) };
                    Resp::LyricsExtras { video_id, extras }
                }
                Req::Art { key, url } => {
                    // The backdrop is a 64 px blur; fetch near that, not the 544 px art.
                    let url = model::sized(&url, 2 * crate::art::SIZE);
                    let bytes = self.http.get(&url).send().await?.error_for_status()?.bytes().await?;
                    let art = tokio::task::spawn_blocking(move || crate::art::process(&bytes)).await??;
                    Resp::Art { key, art }
                }
                Req::Radio(seed) => {
                    let tracks = ytm::radio(api, &seed, false, 50).await?;
                    Resp::Radio { seed, tracks }
                }
                Req::PlayCollection { target, shuffle } => {
                    let mut tracks = match target {
                        Target::Album(id) => ytm::album(api, &id).await?.tracks,
                        Target::Playlist(id) => ytm::playlist(api, &id, me, 500).await?.tracks,
                        Target::Artist(id) => ytm::artist(api, &id).await?.top,
                        Target::Song(t) => vec![t],
                    };
                    if shuffle {
                        crate::ui::shuffle(&mut tracks);
                    }
                    Resp::PlayQueue { tracks }
                }
                Req::Play { video_id, generation, at } => match self.start_playback(&video_id, generation, at).await {
                    Ok(()) => return Ok(None),
                    Err(e) => Resp::PlayError { generation, msg: format!("{e:#}") },
                },
                Req::Prefetch(video_id) => {
                    // Let the playing song finish downloading first: it gets the whole
                    // bandwidth, and a skip in the meantime makes this prefetch moot.
                    let generation = self.audio.status.wanted_gen.load(Ordering::Relaxed);
                    let playing = self.current.lock().unwrap().as_ref().map(|(_, s)| s.clone());
                    if let Some(s) = playing {
                        s.finished().await;
                    }
                    if self.audio.status.wanted_gen.load(Ordering::Relaxed) != generation
                        || self.prefetched.lock().unwrap().as_ref().is_some_and(|(id, _)| *id == video_id)
                    {
                        return Ok(None);
                    }
                    log::debug!("prefetch {video_id}");
                    let shared = self.open_stream(&video_id, true).await?;
                    if let Some((_, old)) = self.prefetched.lock().unwrap().replace((video_id, shared)) {
                        old.cancel();
                    }
                    return Ok(None);
                }
                Req::Like { video_id, like } => {
                    ytm::set_like(api, &video_id, like).await?;
                    Resp::Liked { video_id, like }
                }
                Req::Dislike(video_id) => {
                    ytm::dislike(api, &video_id).await?;
                    Resp::Liked { video_id, like: false }
                }
                Req::CreatePlaylist { title, video_id } => {
                    let id = ytm::create_playlist(api, &title, video_id.as_deref()).await?;
                    Resp::PlaylistCreated { id, had_song: video_id.is_some() }
                }
                Req::EditPlaylist { id, title, description, privacy } => {
                    ytm::edit_playlist(api, &id, &title, &description, privacy.as_deref()).await?;
                    Resp::PlaylistChanged { id: Some(id), message: "Playlist updated" }
                }
                Req::AddToPlaylist { playlist_id, video_id } => {
                    ytm::add_to_playlist(api, &playlist_id, &video_id).await?;
                    Resp::PlaylistChanged { id: Some(playlist_id), message: "Added to playlist" }
                }
                Req::DeletePlaylist(id) => {
                    ytm::delete_playlist(api, &id).await?;
                    Resp::PlaylistChanged { id: None, message: "Playlist deleted" }
                }
                Req::SaveCollection { id, save } => {
                    ytm::save_collection(api, &id, save).await?;
                    Resp::PlaylistChanged { id: Some(id), message: if save { "Saved to library" } else { "Removed from library" } }
                }
                Req::SetCookie(input) => {
                    self.set_cookie(&input).await?;
                    *self.premium.lock().await = Err(None);
                    self.warm_signed_in();
                    log::info!("signed in");
                    Resp::LoggedIn(true)
                }
                Req::Logout => {
                    webmusic::Session::remove(&self.data_dir);
                    api.set_account(None);
                    *self.premium.lock().await = Err(None);
                    crate::login::forget(&self.login_profile());
                    Resp::LoggedIn(false)
                }
            }))
        }
        .await;
        res.unwrap_or_else(|e| {
            log::warn!("{what} failed: {e:#}");
            (!quiet).then(|| Resp::Error(format!("{what} failed: {e}")))
        })
    }

    async fn lyrics_for(&self, t: &Track) -> Option<Lyrics> {
        let artist = t.artist_line();
        let query = lyrics::Query {
            video_id: &t.id,
            song: &t.title,
            artist: &artist,
            album: t.album.as_ref().map(|a| a.name.as_str()),
            duration: t.duration.or_else(|| self.audio.status.duration().map(|d| d.as_secs() as u32)),
        };
        self.lyrics.fetch(&query).await.unwrap_or_else(|e| {
            log::warn!("better lyrics failed: {e:#}");
            None
        })
    }

    /// Sign in with youtube.com cookies: a `Cookie` header value (what the sign-in
    /// window hands over) or a Netscape cookies.txt export.
    async fn set_cookie(&self, input: &str) -> anyhow::Result<()> {
        let input = input.trim();
        let cookie = if input.lines().any(|l| l.split('\t').count() >= 7) {
            netscape_cookies(input)
        } else {
            // Tolerate a copied `cookie: ...` header line.
            input.strip_prefix("cookie:").or_else(|| input.strip_prefix("Cookie:")).unwrap_or(input).trim().to_owned()
        };
        anyhow::ensure!(!cookie.is_empty(), "no youtube.com cookies found");
        let session = self.api.session_from_cookie(&cookie).await.inspect_err(|e| {
            // Names only, never values: enough to see whether SAPISID etc. arrived.
            let names: Vec<&str> = cookie.split(';').filter_map(|kv| kv.trim().split('=').next()).take(40).collect();
            log::warn!("cookie sign-in rejected: {e:#}; cookie names: {names:?}");
        })?;
        session.save(&self.data_dir);
        self.api.set_account(Some(session));
        Ok(())
    }

    async fn start_playback(&self, video_id: &str, generation: u64, at: std::time::Duration) -> anyhow::Result<()> {
        let t0 = std::time::Instant::now();
        let prefetched = self.prefetched.lock().unwrap().take();
        let replay = self.current.lock().unwrap().take().filter(|(id, s)| id == video_id && s.is_complete()).map(|(_, s)| s);
        let ready = match prefetched {
            Some((id, s)) if id == video_id => Some(s),
            // Replaying the current song keeps the next one's prefetch.
            other if replay.is_some() => {
                *self.prefetched.lock().unwrap() = other;
                replay
            }
            // Playback went elsewhere: that prefetch is moot.
            other => {
                if let Some((_, s)) = other {
                    s.cancel();
                }
                None
            }
        };
        let shared = match ready {
            Some(s) => {
                log::debug!("play {video_id} (gen {generation}): using buffered stream");
                s
            }
            None => self.open_stream(video_id, false).await?,
        };
        *self.current.lock().unwrap() = Some((video_id.to_owned(), shared.clone()));
        log::debug!("play {video_id} (gen {generation}): stream open in {:?}", t0.elapsed());
        if self.audio.status.wanted_gen.load(Ordering::Relaxed) != generation {
            shared.cancel();
            return Ok(());
        }

        let audio = self.audio.clone();
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        // Opening the decoder reads the container header, which blocks on the download.
        // Starting mid-song needs the seekable decoder (it waits for every fragment
        // header); from the top, the non-seekable one starts sooner.
        let seek = !at.is_zero();
        let shared2 = shared.clone();
        let open = move || -> anyhow::Result<crate::audio::TrackDecoder> {
            let mut d = stream::decoder(shared.clone(), seek)?;
            if seek {
                rodio::Source::try_seek(&mut d, at).map_err(|e| anyhow::anyhow!("seek: {e}"))?;
            }
            Ok(d)
        };
        tokio::task::spawn_blocking(move || match open() {
            Ok(decoder) => {
                log::debug!("play gen {generation}: decoder ready at {at:?} in {:?}", t0.elapsed());
                audio.send(AudioCmd::Start { generation, decoder, shared: shared2, at })
            }
            Err(e) => {
                let _ = tx.send(Resp::PlayError { generation, msg: format!("decode: {e}") });
                wake();
            }
        });
        Ok(())
    }

    /// Resolve a stream URL and start downloading it into a new buffer (just its start,
    /// for a prefetch, until playback opens it).
    async fn open_stream(&self, video_id: &str, prefetch: bool) -> anyhow::Result<Arc<Shared>> {
        let (source, s) = self.resolve(video_id).await?;
        let shared = Shared::new(s.size, s.duration_ms, s.loudness_db, prefetch);
        let refresh: stream::Refresh = {
            let (this, id) = (self.clone(), video_id.to_owned());
            Arc::new(move || {
                let (this, id) = (this.clone(), id.clone());
                Box::pin(async move { Ok(this.resolve_from(source, &id).await?.url) })
            })
        };
        self.rt.spawn(stream::download(self.http.clone(), s.url, s.user_agent, shared.clone(), Some(refresh)));
        Ok(shared)
    }

    /// Find a stream URL, trying the fastest source that can play the song.
    async fn resolve(&self, video_id: &str) -> anyhow::Result<(Source, ResolvedStream)> {
        if self.hq.load(Ordering::Relaxed)
            && let Some(session) = self.api.account()
            && self.is_premium(&session).await
        {
            match self.resolve_from(Source::WebMusic, video_id).await {
                Ok(s) => return Ok((Source::WebMusic, s)),
                // Not fatal: the usual 128 kbps stream still plays.
                Err(e) => log::warn!("high quality stream failed for {video_id} ({e:#}); using standard quality"),
            }
        }
        match self.resolve_from(Source::VisionOs, video_id).await {
            Ok(s) => Ok((Source::VisionOs, s)),
            // Age-restricted or Premium-only: only signed-in web players get it.
            Err(e) if needs_account(&e) => {
                log::info!("{video_id} needs a signed-in player ({e:#})");
                let session = self.api.account().ok_or_else(|| anyhow::anyhow!("sign in to play this song"))?;
                if self.is_premium(&session).await {
                    match self.resolve_from(Source::WebMusic, video_id).await {
                        Ok(s) => return Ok((Source::WebMusic, s)),
                        Err(e) => log::warn!("web music player failed ({e:#}); using yt-dlp"),
                    }
                }
                Ok((Source::YtDlp, self.resolve_from(Source::YtDlp, video_id).await?))
            }
            Err(e) => Err(e),
        }
    }

    async fn resolve_from(&self, source: Source, video_id: &str) -> anyhow::Result<ResolvedStream> {
        let plain = |s: innertube::Stream, ua: &str| ResolvedStream {
            url: s.url,
            size: s.size,
            duration_ms: s.duration_ms,
            user_agent: ua.to_owned(),
            loudness_db: s.loudness_db,
        };
        Ok(match source {
            Source::VisionOs => plain(self.visionos_stream(video_id).await?, innertube::UA),
            Source::WebMusic => {
                let session = self.api.account().ok_or_else(|| anyhow::anyhow!("signed out"))?;
                plain(session.audio_stream(&self.http, &self.jsc, video_id).await?, webmusic::UA)
            }
            Source::YtDlp => {
                let session = self.api.account().ok_or_else(|| anyhow::anyhow!("sign in to play this song"))?;
                let s = ytdlp::resolve(&self.http, &self.data_dir, &session.cookie, video_id).await?;
                // yt-dlp's JSON doesn't carry YouTube's loudness figure.
                ResolvedStream { url: s.url, size: s.size, duration_ms: s.duration_ms, user_agent: s.user_agent, loudness_db: None }
            }
        })
    }

    /// Get the signed-in playback path ready in the background, so the first Premium-only
    /// or age-restricted song doesn't wait for the Premium check or the solver setup that
    /// each new YouTube player version needs (~5 s, every few days).
    pub fn warm_signed_in(&self) {
        let this = self.clone();
        self.rt.spawn(async move {
            let Some(session) = this.api.account() else { return };
            if this.is_premium(&session).await
                && let Err(e) = this.jsc.warm(&this.http).await
            {
                log::warn!("solver warm-up failed: {e:#}");
            }
        });
    }

    /// Premium accounts can stream signed-in web player URLs without a PO token.
    async fn is_premium(&self, session: &webmusic::Session) -> bool {
        const PREMIUM_RETRY: std::time::Duration = std::time::Duration::from_secs(300);
        let mut premium = self.premium.lock().await;
        match *premium {
            Ok(p) => return p,
            Err(Some(at)) if at.elapsed() < PREMIUM_RETRY => return false,
            Err(_) => {}
        }
        match session.is_premium(&self.http).await {
            Ok(p) => {
                log::info!("account has Premium: {p}");
                *premium = Ok(p);
                p
            }
            Err(e) => {
                log::warn!("Premium check failed: {e:#}");
                *premium = Err(Some(std::time::Instant::now()));
                false
            }
        }
    }

    async fn visionos_stream(&self, video_id: &str) -> anyhow::Result<innertube::Stream> {
        let vd = self.api.visitor_data(false).await?;
        match innertube::audio_stream(&self.http, video_id, &vd).await {
            Ok(s) => Ok(s),
            Err(e) if needs_account(&e) => Err(e),
            Err(e) => {
                // A stale/flagged visitor ID is the usual cause; retry once with a fresh one.
                log::info!("visionOS player: {e:#}; retrying with new visitor data");
                let vd = self.api.visitor_data(true).await?;
                innertube::audio_stream(&self.http, video_id, &vd).await
            }
        }
    }

    /// Webview profile used by the sign-in window.
    pub fn login_profile(&self) -> PathBuf {
        self.data_dir.join("webview")
    }

    /// Report an error from outside the request flow (e.g. the sign-in window).
    pub fn report_error(&self, msg: String) {
        let _ = self.tx.send(Resp::Error(msg));
        self.wake();
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub fn set_high_quality(&self, on: bool) {
        self.hq.store(on, Ordering::Relaxed);
    }

    pub fn is_logged_in(&self) -> bool {
        self.api.account().is_some()
    }
}

#[derive(Clone, Copy)]
enum Source {
    VisionOs,
    WebMusic,
    YtDlp,
}

struct ResolvedStream {
    url: String,
    size: u64,
    duration_ms: u64,
    user_agent: String,
    loudness_db: Option<f32>,
}

/// The anonymous visionOS player refused because the song needs an account:
/// age-restricted (LOGIN_REQUIRED) or Music Premium only.
fn needs_account(e: &anyhow::Error) -> bool {
    let msg = e.to_string();
    msg.contains("LOGIN_REQUIRED") || msg.contains("Premium")
}

/// Keep the signed-in release feed's order, using US charts and the user's home artists.
fn popular_new_albums(albums: &[Card], ranked_artists: &[String], preferred_artists: &[String]) -> Vec<Card> {
    let chart_ids: std::collections::HashSet<&str> = ranked_artists.iter().chain(preferred_artists).map(String::as_str).collect();
    let mut seen = std::collections::HashSet::new();
    albums
        .iter()
        .filter(|album| album.links.iter().any(|a| a.id.as_deref().is_some_and(|id| chart_ids.contains(id))))
        .filter(|album| seen.insert(match &album.target {
            Target::Album(id) => id.as_str(),
            _ => album.title.as_str(),
        }))
        .take(16)
        .cloned()
        .collect()
}

/// youtube.com cookies from a Netscape cookies.txt export, as a `Cookie` header value.
fn netscape_cookies(txt: &str) -> String {
    txt.lines()
        .filter(|l| !l.starts_with('#') || l.starts_with("#HttpOnly_"))
        .filter_map(|l| {
            let f: Vec<&str> = l.trim_start_matches("#HttpOnly_").split('\t').collect();
            (f.len() >= 7 && f[0].trim_start_matches('.').ends_with("youtube.com")).then(|| format!("{}={}", f[5], f[6].trim()))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// "Episodes for later" is listed even when empty; hide it then.
fn saved_playlist_visible(card: &Card) -> bool {
    card.target != Target::Playlist("SE".into()) || card.subtitle.split_whitespace().any(|w| w.parse::<u64>().is_ok_and(|n| n > 0))
}

pub fn compact(n: u64) -> String {
    let (v, unit) = match n {
        0..1_000 => return n.to_string(),
        1_000..1_000_000 => (n as f64 / 1e3, "K"),
        1_000_000..1_000_000_000 => (n as f64 / 1e6, "M"),
        _ => (n as f64 / 1e9, "B"),
    };
    // "6M", not "6.0M"; three significant figures and up need no decimal.
    let s = if v >= 100.0 { format!("{v:.0}") } else { format!("{v:.1}") };
    format!("{}{unit}", s.strip_suffix(".0").unwrap_or(&s))
}

// Larger art URL for the background / now-playing view.
pub fn art_url(t: &Track) -> Option<String> {
    model::pick(&t.thumbs, 120).map(|u| sized(u, 544))
}

#[cfg(test)]
mod explore_tests {
    use super::*;

    fn card(title: &str, target: Target, artist_id: Option<&str>, subtitle: &str) -> Card {
        Card {
            title: title.into(),
            subtitle: subtitle.into(),
            thumbs: vec![],
            target,
            round: false,
            links: artist_id.map(|id| model::Link { name: "Artist".into(), id: Some(id.into()) }).into_iter().collect(),
        }
    }

    #[test]
    fn new_releases_only_include_charting_artists_and_deduplicate() {
        let album = |id: &str, artist_id: &str| card(id, Target::Album(id.into()), Some(artist_id), "Album");
        let albums = vec![album("obscure", "other"), album("favorite-release", "favorite"), album("first-release", "popular"), album("second-release", "popular"), album("first-release", "popular")];
        let found = popular_new_albums(&albums, &["popular".into(), "popular".into()], &["favorite".into()]);
        assert_eq!(found.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["favorite-release", "first-release", "second-release"]);
    }

    #[test]
    fn cookies_txt_keeps_only_youtube() {
        let txt = "# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tabc\n#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t0\tSID\txyz\n.google.com\tTRUE\t/\tTRUE\t0\tNID\tno\n";
        assert_eq!(netscape_cookies(txt), "SAPISID=abc; SID=xyz");
    }

    #[test]
    fn empty_episodes_for_later_is_hidden() {
        let item = |id: &str, subtitle: &str| card("Episodes for later", Target::Playlist(id.into()), None, subtitle);
        assert!(!saved_playlist_visible(&item("SE", "Auto playlist")));
        assert!(!saved_playlist_visible(&item("SE", "Auto playlist • 0 episodes")));
        assert!(saved_playlist_visible(&item("SE", "Auto playlist • 2 episodes")));
        assert!(saved_playlist_visible(&item("PLcustom", "Playlist")));
    }
}
