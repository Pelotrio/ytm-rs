<div align="center">
  <img src="assets/icon/icon.png" alt="ytm-rs app icon" width="88">
  <h1>ytm-rs</h1>
  <p><strong>YouTube Music in a fast, native desktop app.</strong></p>
  <p>Built with Rust, egui, and rodio. No Electron or browser-based player.</p>
  <p>
    <a href="https://grapedjuice.github.io/ytm-rs/">Website</a>
    · <a href="https://github.com/grapedjuice/ytm-rs/releases/latest">Download for Windows</a>
    · <a href="#features">Features</a>
    · <a href="#build-from-source">Build from source</a>
  </p>
</div>

<p align="center">
  <img src="docs/screenshots/now-playing.jpg" alt="Now playing: album art with synced lyrics over an animated album-art background" width="900">
</p>

## Features

| Feature                 | What you can do                                                                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Your music feed**     | Browse your personalized YouTube Music home feed, including Listen again, Quick picks, your mixes (My Supermix, Discover Mix and more), and mood filters. Keep scrolling for more. Pages refresh each time you open them.                |
| **Explore and search**  | Find new releases from US charting artists and artists in your music feed, browse charts, and search for songs, albums, and playlists.              |
| **Your library**        | Access liked songs, saved albums and playlists, listening history and your uploads after signing in. Upload your own songs from the Uploads tab or by dropping audio files on the window. |
| **Playback controls**   | Manage the queue, start radio from a song, and use shuffle, repeat, and autoplay. The next track is prefetched for quicker skips. Volume normalization turns loud songs down to an even level, the way YouTube Music does. |
| **Lyrics and visuals**  | Follow synced lyrics when available, click a line to seek, show romanization and English translations under Japanese, Korean, Chinese and other non-Latin lyrics, switch to full-screen lyrics, hide lyrics to see just the artwork, and turn on the animated, music-reactive album-art background. |
| **Themes**              | Pick from 26 presets rebuilt from the [Better Lyrics community themes](https://github.com/better-lyrics/themes), such as Apple Music, Spicy, Spinning Record and reCover. Each one restyles the lyrics, background, cover, player bar, fonts and the rest of the app. |
| **Desktop integration** | Use system media controls, Discord Rich Presence, and screen-reader-accessible controls.                                                                |
| **Automatic updates**   | The installed app downloads new versions in the background and installs them when you close it, or right away with **Restart to update**. It shows each release's notes when an update is ready and after it installs.              |

## Screenshots

<table>
  <tr>
    <td colspan="2"><img src="docs/screenshots/home.jpg" alt="Home with mood filters and today's biggest hits, a song playing"></td>
  </tr>
  <tr>
    <td colspan="2" align="center">Home</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/artist.jpg" alt="The Weeknd's artist page with a large header image and his top songs, Blinding Lights playing"></td>
    <td><img src="docs/screenshots/settings-themes.jpg" alt="Settings with appearance switches and the theme gallery"></td>
  </tr>
  <tr>
    <td align="center">Artist page</td>
    <td align="center">Settings and themes</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/theme-spinning-record.jpg" alt="Spinning Record theme: the cover as a slowly spinning vinyl disc beside the lyrics"></td>
    <td><img src="docs/screenshots/theme-recover.jpg" alt="reCover theme: the cover fills the left of the screen and fades into the lyrics"></td>
  </tr>
  <tr>
    <td align="center">Spinning Record theme</td>
    <td align="center">reCover theme</td>
  </tr>
</table>

## Get started

### Install on Windows

Download the latest `ytm-rs-setup-<version>.exe` from [GitHub Releases](https://github.com/grapedjuice/ytm-rs/releases/latest) and run it. The installer works for the current user without administrator access and adds a Start menu shortcut and an entry in Windows Settings for uninstalling.

Once installed, ytm-rs keeps itself up to date. You can turn this off in **Settings → Updates**.

### Sign in

Open **Settings → Sign in with Google** in the app. Signing in unlocks your music feed, library, likes, and history. You can also browse and search without signing in.

### Upload your music

Open **Library → Uploads** and click **Upload music**, or drop MP3, M4A, FLAC, OGG or WMA files anywhere on the window. They go to your YouTube Music library, the same as uploading on music.youtube.com. The title, artist, album and cover come from the file's tags, so tag your files before uploading.

### Pick a theme

Open **Settings → Theme** and click a card. Each card shows the original theme's cover and credits its creators. Turn on **Classic theme** under Appearance for the plain dark look with red accents.

## Build from source

On Windows, install the [Rust toolchain](https://rustup.rs/) and the MSVC C++ build tools, then build the app itself:

```powershell
git clone https://github.com/grapedjuice/ytm-rs.git
cd ytm-rs
cargo build --release
```

The built application is at `target\release\ytm-rs.exe`.

## Notes

- The app interface and audio playback are native. Google sign-in and lyrics verification use temporary system webview windows; on Windows, these use WebView2.
- Synced lyrics come from the Better Lyrics API. If they are unavailable, the app can fall back to YouTube Music's plain lyrics. Lyrics availability depends on the track and the providers. Romanization and translations come from Google Translate and are machine-made, so expect rough spots.
- Premium-only and age-restricted songs play through your signed-in account. With YouTube Premium they play natively; without it, age-restricted songs fall back to `yt-dlp`, which needs Node.js.
- Themes are native translations of CSS themes, not the CSS itself, so some details differ from the originals. Fonts that can't be redistributed (SF Pro, Spotify's) are replaced with the closest open fonts.
- ytm-rs is an unofficial client. Changes to YouTube Music or third-party services can affect playback, sign-in, or lyrics.

## License

[GPL-3.0](LICENSE). ytm-rs uses rustypipe, which is GPL-3.0.

## Built with

[egui](https://github.com/emilk/egui) for the interface, [rodio](https://github.com/RustAudio/rodio) and Symphonia for audio, and [rustypipe](https://crates.io/crates/rustypipe) for YouTube Music browsing. Signed-in stream URLs are decoded with yt-dlp's [EJS](https://github.com/yt-dlp/ejs) solver running in QuickJS. The animated background is based on Kawarp from Better Lyrics, and the theme presets are adapted from the Better Lyrics theme store, with credit to each theme's creators. The app also uses Inter, DM Sans, Google Sans Flex, Figtree and Instrument Sans (SIL OFL 1.1) and Lucide icons.

Found a bug or have an idea? [Open an issue](https://github.com/grapedjuice/ytm-rs/issues).
