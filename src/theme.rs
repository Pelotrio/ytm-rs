//! Visual design tokens: fonts, icons, colours, motion.

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId};

// ------------------------------------------------------------------ colours

/// Base surface under the animated background (seen while it fades in).
pub const BG: Color32 = Color32::from_rgb(12, 10, 14);
pub const TEXT: Color32 = Color32::from_rgb(242, 240, 245);
pub const TEXT_DIM: Color32 = Color32::from_rgb(178, 172, 186);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(130, 124, 140);
/// Default accent until a cover provides one (YouTube Music red, slightly warm).
pub const RED: Color32 = Color32::from_rgb(255, 48, 72);

/// Theme preset multiplier for every glass surface (f32 bits; 1.0 by default).
static GLASS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// White at `a` opacity, for glassy surfaces over the background.
pub fn glass(a: f32) -> Color32 {
    let k = f32::from_bits(GLASS.load(std::sync::atomic::Ordering::Relaxed));
    Color32::from_white_alpha(((a * k).min(1.0) * 255.0) as u8)
}

/// The active theme's secondary text colour, set with its chrome (read very often).
static DIM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::from_le_bytes(TEXT_DIM.to_array()));

/// Secondary text colour (themes may tint it).
pub fn text_dim() -> Color32 {
    let [r, g, b, a] = DIM.load(std::sync::atomic::Ordering::Relaxed).to_le_bytes();
    Color32::from_rgba_premultiplied(r, g, b, a)
}

/// Corner radius of album/playlist art in cards.
pub fn card_radius() -> f32 {
    crate::preset::active().app.card_radius.clamp(0.0, 40.0)
}

/// Corner radius of small song-row thumbnails.
pub fn thumb_radius() -> f32 {
    crate::preset::active().app.thumb_radius.clamp(0.0, 20.0)
}

/// Corner radius of panels (settings sections, menus, dialogs).
pub fn panel_radius() -> u8 {
    crate::preset::active().app.panel_radius.clamp(0.0, 32.0) as u8
}

/// Strength of card hover zoom/lift.
pub fn hover_k() -> f32 {
    crate::preset::active().app.hover.clamp(0.0, 2.5)
}

pub fn shade(a: f32) -> Color32 {
    Color32::from_black_alpha((a * 255.0) as u8)
}

/// Scale a colour's opacity by `a` (works for opaque and already-translucent colours).
pub fn with_alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

/// Interpolate in premultiplied space (Color32's native form), so translucent glass
/// colours blend correctly with opaque ones.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

// ------------------------------------------------------------------ motion

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

/// Every UI transition runs at this fraction of the time it's written with: one knob
/// for how snappy the app feels. (Lyrics keep their own timing.)
pub const SPEED: f32 = 0.65;

/// Animate toward `target` (eased), keyed by `id`. Requests repaints while moving.
pub fn anim(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug, target: f32, secs: f32) -> f32 {
    ctx.animate_value_with_time(egui::Id::new(id), target, secs * SPEED)
}

pub fn anim_bool(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug, on: bool, secs: f32) -> f32 {
    ease_out(ctx.animate_bool_with_time(egui::Id::new(id), on, secs * SPEED))
}

// ------------------------------------------------------------------ type

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
// Named families, made once: these are called hundreds of times a frame, and a
// clone is a reference count instead of a new string.
static SEMIBOLD: std::sync::LazyLock<FontFamily> = std::sync::LazyLock::new(|| FontFamily::Name("semibold".into()));
static BOLD: std::sync::LazyLock<FontFamily> = std::sync::LazyLock::new(|| FontFamily::Name("bold".into()));
static ICONS: std::sync::LazyLock<FontFamily> = std::sync::LazyLock::new(|| FontFamily::Name("icons".into()));

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, SEMIBOLD.clone())
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, BOLD.clone())
}
pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, ICONS.clone())
}

/// Lucide icons (ISC), subset to the glyphs used here.
pub mod icon {
    pub const HOUSE: char = '\u{e0f5}';
    pub const COMPASS: char = '\u{e09b}';
    pub const LIBRARY: char = '\u{e100}';
    pub const SEARCH: char = '\u{e151}';
    pub const CHEVRON_LEFT: char = '\u{e06e}';
    pub const CHEVRON_RIGHT: char = '\u{e06f}';
    pub const CHEVRON_UP: char = '\u{e070}';
    pub const CHEVRON_DOWN: char = '\u{e06d}';
    pub const PLAY: char = '\u{e13c}';
    pub const PAUSE: char = '\u{e12e}';
    pub const SKIP_BACK: char = '\u{e15f}';
    pub const SKIP_FORWARD: char = '\u{e160}';
    pub const SHUFFLE: char = '\u{e15e}';
    pub const REPEAT: char = '\u{e146}';
    pub const REPEAT_1: char = '\u{e1fd}';
    pub const VOLUME_2: char = '\u{e1ab}';
    pub const VOLUME_1: char = '\u{e1aa}';
    pub const VOLUME_X: char = '\u{e1ac}';
    pub const HEART: char = '\u{e0f2}';
    pub const THUMBS_DOWN: char = '\u{e189}';
    pub const LIST_MUSIC: char = '\u{e2e0}';
    pub const MIC_VOCAL: char = '\u{e349}';
    pub const X: char = '\u{e1b2}';
    pub const ELLIPSIS_VERTICAL: char = '\u{e0b7}';
    pub const LIST_PLUS: char = '\u{e23f}';
    pub const RADIO: char = '\u{e142}';
    pub const DISC_3: char = '\u{e494}';
    pub const USER: char = '\u{e19f}';
    pub const SETTINGS: char = '\u{e154}';
    pub const LOG_OUT: char = '\u{e10e}';
    pub const MUSIC: char = '\u{e122}';
    pub const SPARKLES: char = '\u{e412}';
    pub const CHECK: char = '\u{e06c}';
    pub const CIRCLE_USER: char = '\u{e461}';
    pub const EXPAND: char = '\u{e21a}';
    pub const SHRINK: char = '\u{e220}';
    pub const UPLOAD: char = '\u{e19e}';
    pub const USERS: char = '\u{e1a4}';
    pub const COPY: char = '\u{e09e}';
    pub const LINK: char = '\u{e102}';
    pub const CROWN: char = '\u{e1d6}';
    pub const HEADPHONES: char = '\u{e0f1}';
}

const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-ExtraBold.ttf");
const LUCIDE: &[u8] = include_bytes!("../assets/fonts/lucide.ttf");

/// Lyric typefaces used by theme presets (all SIL OFL 1.1), as static
/// regular / semibold / bold instances subset to Latin, Greek and Cyrillic.
/// Instrument Sans is only ever used at semibold (one preset), so its bold slot reuses
/// that instead of shipping a weight nothing draws.
const LYRIC_FONTS: &[(&str, [&[u8]; 3])] = &[
    ("dmsans", [
        include_bytes!("../assets/fonts/DMSans-Regular.ttf"),
        include_bytes!("../assets/fonts/DMSans-SemiBold.ttf"),
        include_bytes!("../assets/fonts/DMSans-Bold.ttf"),
    ]),
    ("googlesansflex", [
        include_bytes!("../assets/fonts/GoogleSansFlex-Regular.ttf"),
        include_bytes!("../assets/fonts/GoogleSansFlex-SemiBold.ttf"),
        include_bytes!("../assets/fonts/GoogleSansFlex-Bold.ttf"),
    ]),
    ("figtree", [
        include_bytes!("../assets/fonts/Figtree-Regular.ttf"),
        include_bytes!("../assets/fonts/Figtree-SemiBold.ttf"),
        include_bytes!("../assets/fonts/Figtree-ExtraBold.ttf"),
    ]),
    ("instrumentsans", [
        include_bytes!("../assets/fonts/InstrumentSans-Regular.ttf"),
        include_bytes!("../assets/fonts/InstrumentSans-SemiBold.ttf"),
        include_bytes!("../assets/fonts/InstrumentSans-SemiBold.ttf"),
    ]),
];

/// The bundled font a preset font name refers to. Fonts that can't be bundled (SF Pro,
/// Spotify's) map to the closest open one; unknown names to Inter.
fn lyric_key(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().replace(' ', "").as_str() {
        "dmsans" => "dmsans",
        "googlesansflex" | "googlesans" => "googlesansflex",
        "figtree" | "spotifymixuititle" | "spotifymix" | "circular" => "figtree",
        "switzer" | "instrumentsans" => "instrumentsans",
        _ => return None,
    })
}

/// Lyric font for a preset's font name and weight.
pub fn lyric_font(name: &str, weight: crate::preset::Weight, size: f32) -> FontId {
    use crate::preset::Weight;
    let key = lyric_key(name).unwrap_or("inter");
    let w = match weight {
        Weight::Regular => 0,
        Weight::Semibold => 1,
        Weight::Bold => 2,
    };
    FontId::new(size, FontFamily::Name(format!("{key}-{w}").into()))
}

// System fallbacks: egui's fonts cover Latin only, so CJK, Arabic, Indic, Thai,
// symbols and emoji in titles would render as boxes. Memory-mapped, not read, so a
// 20 MB CJK collection costs only the glyph pages actually drawn.
#[cfg(windows)]
const CANDIDATES: &[(&str, u32)] = &[
    ("C:\\Windows\\Fonts\\segoeui.ttf", 0),   // Latin ext., Greek, Cyrillic, Arabic, Hebrew
    ("C:\\Windows\\Fonts\\seguisym.ttf", 0),  // symbols, arrows, dingbats
    ("C:\\Windows\\Fonts\\YuGothM.ttc", 0),   // Japanese
    ("C:\\Windows\\Fonts\\msyh.ttc", 0),      // Simplified Chinese
    ("C:\\Windows\\Fonts\\msjh.ttc", 0),      // Traditional Chinese
    ("C:\\Windows\\Fonts\\malgun.ttf", 0),    // Korean
    ("C:\\Windows\\Fonts\\Nirmala.ttc", 0),   // Indic scripts
    ("C:\\Windows\\Fonts\\Nirmala.ttf", 0),
    ("C:\\Windows\\Fonts\\LeelawUI.ttf", 0),  // Thai, Lao, Khmer
    ("C:\\Windows\\Fonts\\ebrima.ttf", 0),    // Ethiopic, N'Ko, etc.
    ("C:\\Windows\\Fonts\\seguiemj.ttf", 0),  // emoji (drawn monochrome)
];

/// Heavier faces for the semibold and bold families, so CJK lyrics come out at the
/// weight the theme asks for (Better Lyrics uses Noto Sans CJK at the lyric weight).
#[cfg(windows)]
const BOLD_VARIANTS: &[(&str, &str)] = &[
    ("C:\\Windows\\Fonts\\YuGothM.ttc", "C:\\Windows\\Fonts\\YuGothB.ttc"),
    ("C:\\Windows\\Fonts\\msyh.ttc", "C:\\Windows\\Fonts\\msyhbd.ttc"),
    ("C:\\Windows\\Fonts\\msjh.ttc", "C:\\Windows\\Fonts\\msjhbd.ttc"),
    ("C:\\Windows\\Fonts\\malgun.ttf", "C:\\Windows\\Fonts\\malgunbd.ttf"),
];
#[cfg(not(windows))]
const BOLD_VARIANTS: &[(&str, &str)] = &[];

#[cfg(target_os = "macos")]
const CANDIDATES: &[(&str, u32)] = &[
    ("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", 0),
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
    ("/System/Library/Fonts/AppleSDGothicNeo.ttc", 0),
    ("/System/Library/Fonts/Apple Symbols.ttf", 0),
];

#[cfg(all(unix, not(target_os = "macos")))]
const CANDIDATES: &[(&str, u32)] = &[
    ("/usr/share/fonts/noto/NotoSans-Regular.ttf", 0),
    ("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", 0),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf", 0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0),
];

/// Ascent and line height in em, as egui lays text out (skrifa's metrics: OS/2 typo
/// values when the font asks for them, else hhea).
fn em_metrics(bytes: &[u8], index: u32) -> Option<(f32, f32)> {
    use skrifa::MetadataProvider;
    let font = skrifa::FontRef::from_index(bytes, index).ok()?;
    let m = font.metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
    let upem = m.units_per_em.max(1) as f32;
    Some((m.ascent / upem, (m.ascent - m.descent + m.leading) / upem))
}

/// Install fonts. `ui_font` is a preset font name for all interface text ("inter" or
/// one of the bundled lyric fonts).
///
/// egui places a fallback font's glyphs by centring its line height on the primary
/// font's, not by sharing a baseline, so CJK glyphs (Yu Gothic, Microsoft YaHei, …)
/// sit visibly higher than Latin ones in the same line. Each fallback therefore gets a
/// y offset computed from both fonts' metrics, once per primary font it backs.
pub fn install_fonts(ctx: &egui::Context, ui_font: &str) {
    // Installing rebuilds the font atlas; most themes share a UI font, so skip repeats.
    static INSTALLED: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);
    let ui_key = lyric_key(ui_font).unwrap_or("inter");
    if INSTALLED.lock().unwrap().replace(ui_key) == Some(ui_key) {
        return;
    }
    let mut defs = FontDefinitions::default();
    let egui_default = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    defs.font_data.insert("lucide".into(), Arc::new(FontData::from_static(LUCIDE).tweak(egui::FontTweak { y_offset_factor: 0.0, ..Default::default() })));
    defs.families.insert(FontFamily::Name("icons".into()), vec!["lucide".to_owned()]);

    // Mapped once for the life of the process, however often fonts are installed.
    static SYSTEM: std::sync::OnceLock<Vec<(&str, &'static [u8], u32)>> = std::sync::OnceLock::new();
    let system = SYSTEM.get_or_init(|| CANDIDATES.iter().filter_map(|&(path, index)| map(path).map(|b| (path, b, index))).collect());
    static SYSTEM_BOLD: std::sync::OnceLock<Vec<(&str, &'static [u8], u32)>> = std::sync::OnceLock::new();
    // The same list with the heavier CJK faces swapped in.
    let system_bold = SYSTEM_BOLD.get_or_init(|| {
        CANDIDATES
            .iter()
            .filter_map(|&(path, index)| {
                let heavy = BOLD_VARIANTS.iter().find(|(base, _)| *base == path).map(|(_, b)| *b);
                heavy.and_then(|h| map(h).map(|b| (h, b, index))).or_else(|| map(path).map(|b| (path, b, index)))
            })
            .collect()
    });
    // Primary font groups that share metrics: Inter's weights, then each lyric font.
    let mut groups: Vec<(&str, [&'static [u8]; 3])> = vec![("inter", [INTER_REGULAR, INTER_SEMIBOLD, INTER_BOLD])];
    groups.extend(LYRIC_FONTS.iter().map(|(k, w)| (*k, *w)));
    for (key, weights) in &groups {
        let (asc_p, h_p) = em_metrics(weights[0], 0).unwrap_or((0.97, 1.21));
        let mut chains: [Vec<String>; 2] = [Vec::new(), Vec::new()];
        for (heavy, list) in [system, system_bold].into_iter().enumerate() {
            for &(path, bytes, index) in list {
                let Some((asc_f, h_f)) = em_metrics(bytes, index) else { continue };
                let shift = asc_p - (asc_f + 0.5 * (h_p - h_f));
                let name = format!("sys:{path}@{key}");
                if !defs.font_data.contains_key(&name) {
                    let tweak = egui::FontTweak { y_offset_factor: shift, ..Default::default() };
                    defs.font_data.insert(name.clone(), Arc::new(FontData { index, ..FontData::from_static(bytes) }.tweak(tweak)));
                }
                chains[heavy].push(name);
            }
        }
        let fallbacks = chains[0].clone();
        for (w, bytes) in weights.iter().enumerate() {
            let fallbacks = if w == 0 { &chains[0] } else { &chains[1] };
            let name = format!("{key}-{w}");
            defs.font_data.insert(name.clone(), Arc::new(FontData::from_static(bytes)));
            let mut chain = vec![name.clone()];
            chain.extend(fallbacks.iter().cloned());
            // egui's bundled emoji/icon font stays as a last resort.
            chain.extend(egui_default.iter().filter(|n| n.as_str() != "Ubuntu-Light").cloned());
            defs.families.insert(FontFamily::Name(name.clone().into()), chain.clone());
            if *key == ui_key {
                let family = match w {
                    0 => FontFamily::Proportional,
                    1 => FontFamily::Name("semibold".into()),
                    _ => FontFamily::Name("bold".into()),
                };
                defs.families.insert(family, chain);
            }
        }
        if *key == "inter" {
            if let Some(mono) = defs.families.get_mut(&FontFamily::Monospace) {
                mono.extend(fallbacks.iter().cloned());
            }
        }
    }
    ctx.set_fonts(defs);
}

fn map(path: &str) -> Option<&'static [u8]> {
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: system font files aren't modified while in use; the mapping is
    // read-only and intentionally lives for the rest of the process.
    let mmap = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    Some(Box::leak(Box::new(mmap)))
}

/// Apply a theme preset's chrome: how strong glass surfaces are and how round
/// buttons, pills and fields are.
pub fn apply_chrome(ctx: &egui::Context, p: &crate::preset::Preset) {
    GLASS.store(p.chrome.glass.clamp(0.0, 2.5).to_bits(), std::sync::atomic::Ordering::Relaxed);
    let dim = crate::preset::hex(&p.app.text_dim).unwrap_or(TEXT_DIM);
    DIM.store(u32::from_le_bytes(dim.to_array()), std::sync::atomic::Ordering::Relaxed);
    install_style(ctx);
    install_fonts(ctx, &p.app.font);
    let r = p.chrome.radius.clamp(2.0, 28.0) as u8;
    let pr = egui::CornerRadius::same(p.app.panel_radius.clamp(0.0, 32.0) as u8);
    ctx.global_style_mut(|s| {
        s.visuals.window_corner_radius = pr;
        s.visuals.menu_corner_radius = pr;
        for w in [&mut s.visuals.widgets.inactive, &mut s.visuals.widgets.hovered, &mut s.visuals.widgets.active, &mut s.visuals.widgets.open] {
            w.corner_radius = egui::CornerRadius::same(r);
        }
    });
}

pub fn install_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.text_styles.insert(egui::TextStyle::Body, regular(14.0));
        s.text_styles.insert(egui::TextStyle::Button, semibold(14.0));
        s.text_styles.insert(egui::TextStyle::Heading, bold(22.0));
        s.text_styles.insert(egui::TextStyle::Small, regular(12.0));
        let v = &mut s.visuals;
        v.panel_fill = BG;
        v.window_fill = Color32::from_rgb(28, 24, 32);
        v.extreme_bg_color = glass(0.08);
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = with_alpha(RED, 0.5);
        v.hyperlink_color = TEXT;
        v.window_corner_radius = egui::CornerRadius::same(14);
        v.menu_corner_radius = egui::CornerRadius::same(12);
        v.window_stroke = egui::Stroke::new(1.0, glass(0.08));
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: shade(0.5) };
        for (w, fill) in [
            (&mut v.widgets.inactive, glass(0.08)),
            (&mut v.widgets.hovered, glass(0.14)),
            (&mut v.widgets.active, glass(0.2)),
            (&mut v.widgets.open, glass(0.14)),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = egui::Stroke::NONE;
            w.corner_radius = egui::CornerRadius::same(18);
            w.fg_stroke.color = TEXT;
        }
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, glass(0.08));
        s.interaction.selectable_labels = false;
        s.animation_time = 0.18 * SPEED;
    });
}
