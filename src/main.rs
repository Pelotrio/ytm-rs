#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod audio;
mod backend;
mod discord;
mod images;
mod innertube;
mod jsc;
mod login;
mod lyrics;
mod media;
mod model;
mod preset;
mod shader;
mod stream;
mod theme;
mod transfer;
mod translate;
mod tray;
mod turnstile;
mod ui;
mod update;
mod webmusic;
mod ytdlp;
mod ytm;
mod vault;
mod ytmapi;

use std::sync::{Arc, OnceLock};

use crossbeam_channel::unbounded;

/// Window size on first launch, and the smallest it may be (points).
pub const DEFAULT_SIZE: [f32; 2] = [1200.0, 780.0];
pub const MIN_SIZE: [f32; 2] = [760.0, 480.0];

fn main() -> eframe::Result {
    // Child-process modes for the webview windows (see login.rs / turnstile.rs).
    let mut args = std::env::args_os().skip(1);
    if let (Some(flag), Some(dir)) = (args.next(), args.next()) {
        if flag == login::FLAG {
            login::child_main(login::Site::Google, dir.into());
            return Ok(());
        }
        if flag == login::SPOTIFY_FLAG {
            login::child_main(login::Site::Spotify, dir.into());
            return Ok(());
        }
        if flag == turnstile::FLAG {
            turnstile::child_main(dir.into());
            return Ok(());
        }
    }
    init_logging();

    // Dev hook: YTM_BACKGROUND opens the window off-screen, unfocused and off the
    // taskbar, so automated test runs don't pop up over whatever the user is doing.
    let background = std::env::var_os("YTM_BACKGROUND").is_some();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("YT Music")
        .with_app_id("ytm-rs")
        .with_inner_size(DEFAULT_SIZE)
        .with_min_inner_size(MIN_SIZE)
        .with_icon(
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/icon.png"))
                .expect("bundled icon is a valid PNG"),
        );
    if background {
        viewport = viewport.with_position([-4000.0, 0.0]).with_inner_size([1920.0, 1057.0]).with_active(false).with_taskbar(false);
    }
    let options = eframe::NativeOptions {
        viewport,
        persist_window: !background,
        // Dev hook: YTM_DATA_DIR runs on a separate profile (settings, sign-in, caches),
        // e.g. a clean signed-out one for screenshots.
        persistence_path: std::env::var_os("YTM_DATA_DIR").map(|d| std::path::PathBuf::from(d).join("app.ron")),
        // No MSAA/depth/stencil: egui doesn't need them and they cost VRAM.
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        // Dev hook: YTM_FPS paces frames itself (see `dev_pace`), so vsync is off.
        glow_options: eframe::egui_glow::GlowConfiguration { vsync: std::env::var_os("YTM_FPS").is_none(), ..Default::default() },
        ..Default::default()
    };

    eframe::run_native(
        "ytm-rs",
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            theme::install_style(&ctx);
            theme::install_fonts(&ctx, "inter");

            // Background threads wake the UI through this; set once the context exists.
            static CTX: OnceLock<egui::Context> = OnceLock::new();
            let _ = CTX.set(ctx.clone());
            let wake = || {
                if let Some(c) = CTX.get() {
                    c.request_repaint();
                }
            };

            let volume = cc
                .storage
                .and_then(|s| s.get_string("volume"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.8f32);
            // Dev hook: YTM_MUTE starts silent and never persists the volume.
            let volume = if std::env::var_os("YTM_MUTE").is_some() { 0.0 } else { volume };

            let (audio_tx, audio_rx) = unbounded();
            let audio = audio::spawn(volume, audio_tx, wake);

            let data_dir = match std::env::var_os("YTM_DATA_DIR") {
                Some(d) => std::path::PathBuf::from(d),
                None => directories::ProjectDirs::from("", "", "ytm-rs").expect("no home directory").data_dir().to_path_buf(),
            };
            let (resp_tx, resp_rx) = unbounded();
            let backend = backend::Backend::new(data_dir, resp_tx, audio.clone(), Arc::new(wake))?;

            egui_extras::install_image_loaders(&ctx);
            ctx.add_bytes_loader(Arc::new(images::HttpImages::new(backend.clone())));

            let (media_tx, media_rx) = unbounded();
            let media = media::Media::new(hwnd(cc), media_tx, wake);

            // The animated background needs the GL context; without it the UI falls back
            // to a flat gradient.
            let bg = cc.gl.as_ref().filter(|_| std::env::var_os("YTM_NO_SHADER").is_none()).and_then(|gl| {
                shader::Background::new(gl).map_err(|e| log::warn!("background shader unavailable: {e:#}")).ok()
            });

            Ok(Box::new(ui::App::new(
                backend,
                audio,
                ui::Channels { resp_rx, audio_rx, media_rx },
                media,
                bg,
                cc.storage,
            )))
        }),
    )
}

#[cfg(windows)]
fn hwnd(cc: &eframe::CreationContext) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}

#[cfg(not(windows))]
fn hwnd(_: &eframe::CreationContext) -> Option<*mut std::ffi::c_void> {
    None
}

/// Log to stderr in debug builds; release builds have no console, so log to a file
/// next to the settings (overwritten each launch).
fn init_logging() {
    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,ytm_rs=info"),
    );
    // Dev hook: YTM_LOG=<path> logs there (test runs beside a running copy).
    if let Some(path) = std::env::var_os("YTM_LOG") {
        if let Ok(file) = std::fs::File::create(path) {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    } else if !cfg!(debug_assertions) {
        if let Some(dirs) = directories::ProjectDirs::from("", "", "ytm-rs") {
            let _ = std::fs::create_dir_all(dirs.data_dir());
            if let Ok(file) = std::fs::File::create(dirs.data_dir().join("ytm-rs.log")) {
                builder.target(env_logger::Target::Pipe(Box::new(file)));
            }
        }
    }
    builder.init();
}
