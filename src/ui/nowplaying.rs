//! Now-playing overlay: big art + synced lyrics / up next, over the full-strength
//! animated background. Lyrics follow better-lyrics' presentation: the active line
//! fills word by word, others dim with distance, the view glides to keep the active
//! line in place, and instrumental breaks show breathing dots.

use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, Vec2, pos2, vec2};

use super::widgets::{self, icon_button, named, text_at};
use super::{Action, App, NpTab};
use crate::lyrics::{Line, Lyrics, Sync};
use crate::shader::Params;
use crate::theme::{self, icon};

pub fn overlay(app: &mut App, ctx: &egui::Context, now: f64) {
    use crate::preset::BgMode;
    let t = theme::anim_bool(ctx, "np-open", app.np_open && app.current().is_some(), 0.42);
    if t <= 0.001 {
        return;
    }
    let screen = ctx.content_rect();
    let full = if app.fullscreen { screen } else { Rect::from_min_max(screen.min, pos2(screen.right(), screen.bottom() - app.bar_height())) };
    let rect = full.translate(vec2(0.0, full.height() * (1.0 - t)));
    let look = app.look();
    let b = &look.background;
    egui::Area::new("now-playing".into())
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            // Swallow input so the page underneath doesn't react.
            ui.allocate_rect(rect, Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let themed = !app.default_theme;
            let art = matches!(b.mode, BgMode::Kawarp | BgMode::CoverStatic);
            if themed && b.mode == BgMode::CoverFullbleed {
                // The cover itself fills the screen behind a scrim. With the lyrics-only
                // layout on a wide window it fills the height on the left instead and
                // fades out toward the lyrics (reCover).
                let solid = look.solid();
                painter.rect_filled(rect, 0.0, solid);
                if let Some(track) = app.current() {
                    let side_by_side = look.cover.layout == crate::preset::Layout::LyricsOnly && rect.width() >= 900.0;
                    let area = if side_by_side { Rect::from_min_size(rect.min, Vec2::splat(rect.height())) } else { rect };
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
                    child.set_clip_rect(area);
                    widgets::paint_cover_fill(&child, area, &track.thumbs, Color32::from_gray((b.np_dim.clamp(0.0, 1.0) * 255.0) as u8));
                    if side_by_side {
                        let fade = Rect::from_min_max(pos2(area.center().x - area.width() * 0.1, area.top()), area.max);
                        super::shell::hgradient(&painter, fade, Color32::TRANSPARENT, solid);
                    }
                }
            } else if let Some(bg) = app.bg.as_ref().filter(|_| themed && art) {
                let blend = theme::ease_in_out(((now - app.art_changed) / 1.4) as f32);
                let level = if app.reactive_bg && app.playing() { app.audio.status.level() * b.reactive } else { 0.0 };
                bg.paint(
                    &painter,
                    rect,
                    Params {
                        time: app.bg_time as f32,
                        blend,
                        intensity: if b.mode == BgMode::CoverStatic { 0.0 } else { b.intensity },
                        saturation: b.saturation,
                        dim: b.np_dim,
                        scale: b.scale + level.min(0.5) * 0.12,
                    },
                );
            } else if themed && b.mode == BgMode::Solid {
                painter.rect_filled(rect, 0.0, look.solid());
            } else {
                painter.rect_filled(rect, 0.0, theme::lerp_color(theme::BG, app.accent_color(), 0.25));
            }
            let k = if themed { b.scrim / 0.3 } else { 1.0 };
            super::shell::vgradient(ui.painter(), rect, theme::shade((0.18 * k).min(0.95)), theme::shade((0.42 * k).min(0.95)));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(32.0, 20.0))));
            body(app, &mut child, now);
        });
}

fn body(app: &mut App, ui: &mut Ui, now: f64) {
    use crate::preset::{Layout, Side};
    let Some(track) = app.current().cloned() else { return };
    let look = app.look();
    let cs = &look.cover;
    let area = ui.max_rect();
    let no_lyrics = matches!(&app.lyrics, Some((_, None)));
    // Header
    let header = Rect::from_min_size(area.min, vec2(area.width(), 44.0));
    let mut hui = ui.new_child(egui::UiBuilder::new().max_rect(header).layout(egui::Layout::left_to_right(egui::Align::Center)));
    if icon_button(&mut hui, icon::CHEVRON_DOWN, 22.0, "Close now playing", false).clicked() {
        app.np_open = false;
        if app.fullscreen {
            app.set_fullscreen(ui.ctx(), false);
        }
    }
    hui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let (g, name) = if app.fullscreen { (icon::SHRINK, "Exit full screen") } else { (icon::EXPAND, "Full screen") };
        if icon_button(ui, g, 18.0, name, false).clicked() {
            let fs = !app.fullscreen;
            app.set_fullscreen(ui.ctx(), fs);
        }
        ui.add_space(8.0);
        let more = icon_button(ui, icon::ELLIPSIS_VERTICAL, 20.0, "Song options", false);
        let liked = app.liked.contains(&track.id);
        egui::Popup::menu(&more).show(|ui| widgets::song_menu(ui, &track, liked, &mut app.actions));
        ui.add_space(8.0);
        // Clicking the active chip again turns it off, leaving just the artwork.
        if widgets::chip(ui, "Up next", app.np_tab == NpTab::UpNext).clicked() {
            app.np_tab = if app.np_tab == NpTab::UpNext { NpTab::Art } else { NpTab::UpNext };
        }
        // A song without lyrics: the chip is off and does nothing, but the choice stays
        // Lyrics so the next song that has them shows them.
        let lyr = ui.scope(|ui| {
            if no_lyrics {
                ui.disable();
                ui.multiply_opacity(0.45);
            }
            widgets::chip(ui, "Lyrics", app.np_tab == NpTab::Lyrics && !no_lyrics).on_disabled_hover_text("No lyrics for this song")
        });
        if lyr.inner.clicked() {
            app.np_tab = if app.np_tab == NpTab::Lyrics { NpTab::Art } else { NpTab::Lyrics };
        }
        if app.np_tab == NpTab::Lyrics && !no_lyrics {
            ui.add_space(4.0);
            let tr = widgets::chip(ui, "文A", app.show_roman || app.show_trans).on_hover_text("Romanization and translation");
            egui::Popup::menu(&tr).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                ui.set_width(300.0);
                super::pages::setting_row(ui, "Romanization", Some("Japanese, Korean, Chinese, Cyrillic and other scripts in Latin letters."), |ui| {
                    super::pages::toggle(ui, &mut app.show_roman, "Romanization");
                });
                super::pages::setting_row(ui, "English translation", Some("Machine translated by Google. Skipped for English songs."), |ui| {
                    super::pages::toggle(ui, &mut app.show_trans, "English translation");
                });
            });
        }
    });

    let content = Rect::from_min_max(pos2(area.left(), header.bottom() + 12.0), area.max);
    // The side panel (lyrics / up next) slides in and out when its chip is toggled.
    // A song without lyrics shows just the artwork, with no transition.
    let last_id = egui::Id::new("np-last-panel");
    let panel_tab = if app.np_tab == NpTab::Art {
        ui.ctx().data(|d| d.get_temp(last_id)).unwrap_or(NpTab::Lyrics)
    } else {
        ui.ctx().data_mut(|d| d.insert_temp(last_id, app.np_tab));
        app.np_tab
    };
    // Linear 0..1 over the transition; each part eases its own slice of it.
    let shown = ui.ctx().animate_bool_with_time(egui::Id::new("np-panel"), app.np_tab != NpTab::Art, 0.6);
    if shown > 0.0 && shown < 1.0 {
        // Every frame while sliding, not the now-playing view's usual 30 fps.
        ui.ctx().request_repaint();
    }
    let ppp = ui.ctx().pixels_per_point();
    let p = if panel_tab == NpTab::Lyrics && no_lyrics { 0.0 } else { shown };
    // Whichever is leaving slides and fades out over the first 45%, then the other
    // comes in over the last 70%, both easing out, in either direction.
    let (panel_vis, art_vis) = if app.np_tab != NpTab::Art {
        (theme::ease_out((p - 0.3) / 0.7), 1.0 - theme::ease_out(p / 0.45))
    } else {
        let q = 1.0 - p;
        (1.0 - theme::ease_out(q / 0.45), theme::ease_out((q - 0.3) / 0.7))
    };
    // The cover gliding between its two places (wide layouts): a gentle S-curve.
    let glide = p * p * (3.0 - 2.0 * p);
    let narrow = content.width() < 860.0;
    let lyrics_only = narrow || cs.layout == Layout::LyricsOnly;
    // reCover: the backdrop cover sits on the left, so the lyrics take the right side.
    let beside_cover = !narrow && cs.layout == Layout::LyricsOnly && look.background.mode == crate::preset::BgMode::CoverFullbleed;
    let centered = lyrics_only && !beside_cover;
    // Artwork alone: centered.
    let col_art = Rect::from_center_size(content.center(), vec2(content.width().min(content.height() * 0.9), content.height()));
    let slide = 72.0;
    let right = if lyrics_only {
        // No cover column next to the panel: the artwork fades in from the left as the panel leaves.
        let px = (art_side(cs, col_art) * ppp) as u32;
        if art_vis > 0.001 {
            let mut cui = ui.new_child(egui::UiBuilder::new().max_rect(content));
            cui.multiply_opacity(art_vis);
            cover_column(app, &mut cui, col_art.translate(vec2(-slide * (1.0 - art_vis), 0.0)), &track, no_lyrics, now, px);
        } else {
            // Hidden behind the lyrics: have it loaded for when they're turned off.
            widgets::preload_cover(ui.ctx(), &track.thumbs, px);
        }
        if beside_cover { Rect::from_min_max(pos2(content.left() + content.width() * 0.46, content.top()), content.max) } else { content }
    } else {
        // Cover column, on the preset's side; it glides to the center as the panel leaves.
        let frac = if cs.layout == Layout::CoverHero { 0.5 } else { 0.42 };
        let col_w = content.width() * frac;
        let col_panel = if cs.side == Side::Right {
            Rect::from_min_size(pos2(content.right() - col_w, content.top()), vec2(col_w, content.height()))
        } else {
            Rect::from_min_size(content.min, vec2(col_w, content.height()))
        };
        let col = Rect::from_min_max(col_art.min.lerp(col_panel.min, glide), col_art.max.lerp(col_panel.max, glide));
        // One image size for the whole glide, the bigger of its two ends.
        let px = (art_side(cs, col_art).max(art_side(cs, col_panel)) * ppp) as u32;
        cover_column(app, ui, col, &track, no_lyrics, now, px);
        if cs.side == Side::Right {
            Rect::from_min_max(content.min, pos2(col_panel.left() - 28.0, content.bottom()))
        } else {
            Rect::from_min_max(pos2(col_panel.right() + 28.0, content.top()), content.max)
        }
    };
    if panel_vis <= 0.001 {
        return;
    }
    let toward = if !lyrics_only && cs.side == Side::Right { -slide } else { slide };
    let rect = right.translate(vec2(toward * (1.0 - panel_vis), 0.0));
    let mut rui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    rui.set_clip_rect(content.intersect(ui.clip_rect()));
    rui.multiply_opacity(panel_vis);
    match panel_tab {
        NpTab::Lyrics => lyrics_panel(app, &mut rui, rect, now, centered),
        NpTab::UpNext => super::shell::queue_list(app, &mut rui, "np"),
        NpTab::Art => {}
    }
}

/// Size of the artwork laid out in `col`.
fn art_side(cs: &crate::preset::CoverStyle, col: Rect) -> f32 {
    (col.width() * 0.82 * cs.size.clamp(0.5, 1.35)).min(col.height() - 110.0).max(160.0)
}

/// Artwork with the title and artists under it, laid out in `col`. `px`: the size to
/// fetch the image at (fixed while the column animates).
#[allow(clippy::too_many_arguments)]
fn cover_column(app: &mut App, ui: &mut Ui, col: Rect, track: &crate::model::Track, no_lyrics: bool, now: f64, px: u32) {
    use crate::preset::Shape;
    let look = app.look();
    let cs = &look.cover;
    let side = art_side(cs, col);
    let art = Rect::from_center_size(pos2(col.center().x, col.top() + side / 2.0 + (col.height() - side - 110.0).max(0.0) * 0.35), Vec2::splat(side));
    let round = cs.shape != Shape::Rounded;
    let radius = if round { side / 2.0 } else { cs.radius.clamp(0.0, side / 2.0) };
    let cr = egui::CornerRadius::same(radius.min(255.0) as u8);
    if cs.glow > 0.0 {
        let s = egui::epaint::Shadow { offset: [0, 16], blur: 90, spread: 6, color: theme::with_alpha(app.accent_color(), cs.glow.min(1.0) * 0.7) };
        ui.painter().add(s.as_shape(art, cr));
    }
    if cs.shadow > 0.0 {
        widgets::soft_shadow(ui, art, radius.min(255.0), 60.0, cs.shadow.min(1.0));
    }
    let tint = Color32::from_gray((cs.brightness.clamp(0.1, 1.0) * 255.0) as u8);
    if round {
        if cs.shape == Shape::Record && app.playing() {
            let dt = ui.input(|i| i.stable_dt).min(0.1) as f64;
            app.record_angle = (app.record_angle + dt * std::f64::consts::TAU / cs.spin_secs.max(4.0) as f64) % std::f64::consts::TAU;
        }
        let angle = if cs.shape == Shape::Record { app.record_angle as f32 } else { 0.0 };
        widgets::paint_disc(ui, art.center(), side / 2.0, &track.thumbs, angle, tint, px);
        if cs.shape == Shape::Record {
            // Vinyl: faint grooves and a spindle hole.
            let p = ui.painter();
            for i in 1..6 {
                p.circle_stroke(art.center(), side / 2.0 * (0.42 + i as f32 * 0.1), egui::Stroke::new(1.0, theme::shade(0.18)));
            }
            p.circle_filled(art.center(), side * 0.09, theme::shade(0.35));
            p.circle_filled(art.center(), side * 0.022, Color32::from_rgb(10, 10, 12));
        }
    } else {
        widgets::paint_cover_px(ui, art, &track.thumbs, radius, 1.0, px);
        if cs.brightness < 0.99 {
            ui.painter().rect_filled(art, cr, theme::shade(1.0 - cs.brightness));
        }
    }
    let cover = named(ui.interact(art, ui.id().with(("cover-playback", &track.id)), Sense::click()), if app.playing() { "Pause" } else { "Play" })
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let hover = theme::anim_bool(ui.ctx(), ("cover-playback-hover", &track.id), cover.hovered(), 0.16);
    if hover > 0.0 {
        ui.painter().rect_filled(art, cr, theme::shade(0.28 * hover));
        ui.painter().circle_filled(art.center(), 38.0, theme::shade(0.58 * hover));
        widgets::icon_at(ui, art.center(), if app.playing() || app.buffering { icon::PAUSE } else { icon::PLAY }, 37.0, theme::with_alpha(Color32::WHITE, hover));
    }
    if cover.clicked() { app.actions.push(Action::TogglePlay); }
    let w = side;
    let title_y = art.bottom() + 22.0;
    text_at(ui, pos2(art.left(), title_y), &track.title, theme::bold(28.0), Color32::WHITE, w);
    // Each artist opens their own page (features included).
    let line = track.artist_line();
    widgets::linked_text(ui, pos2(art.left(), title_y + 40.0), &line, &track.artists, theme::regular(17.0), theme::text_dim(), w, &mut app.actions);
    if no_lyrics && app.np_tab == NpTab::Lyrics {
        // A brief note, faded out after a few seconds.
        let since = ui.ctx().memory_mut(|m| *m.data.get_temp_mut_or_insert_with(egui::Id::new(("no-lyrics", &track.id)), || now));
        let a = (1.0 - ((now - since - 3.0) / 1.0) as f32).clamp(0.0, 1.0);
        if a > 0.0 {
            let c = theme::with_alpha(theme::text_dim(), a * 0.8);
            text_at(ui, pos2(art.left(), title_y + 68.0), "No lyrics for this song", theme::regular(14.0), c, w);
            // Frames only for the fade-out.
            let wait = (since + 3.0 - now).max(0.0);
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(if wait > 0.0 { wait } else { 0.033 }));
        }
    }
}

// ------------------------------------------------------------------ lyrics

enum Item {
    Line(usize),
    /// Instrumental break between `start` and `end` ms.
    Gap { start: u32, end: u32 },
}

fn lyrics_panel(app: &mut App, ui: &mut Ui, rect: Rect, now: f64, centered: bool) {
    // Lent out for the frame rather than cloned (the drawing needs `app` mutably).
    let lyrics = app.lyrics.take();
    lyrics_body(app, ui, rect, now, centered, lyrics.as_ref().map(|(_, l)| l.as_ref()));
    app.lyrics = lyrics;
}

/// `lyrics`: None while loading, Some(None) when the song has none.
fn lyrics_body(app: &mut App, ui: &mut Ui, rect: Rect, now: f64, centered: bool, lyrics: Option<Option<&Lyrics>>) {
    let Some(lyrics) = lyrics else {
        ui.add_space(rect.height() * 0.35);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("Loading lyrics…").color(theme::text_dim()));
        });
        return;
    };
    let Some(lyrics) = lyrics else {
        ui.add_space(rect.height() * 0.4);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No lyrics found for this song").font(theme::semibold(20.0)).color(theme::text_dim()));
        });
        return;
    };
    if lyrics.sync == Sync::None {
        egui::ScrollArea::vertical().id_salt("plain-lyrics").auto_shrink(false).show(ui, |ui| {
            ui.label(egui::RichText::new("These lyrics aren't synced").font(theme::regular(13.0)).color(theme::TEXT_FAINT));
            ui.add_space(14.0);
            let extras = app.lyrics_extras.as_ref().map(|(_, e)| e);
            fn get(on: bool, v: Option<&Vec<Option<String>>>, i: usize) -> Option<&str> {
                v.filter(|_| on)?.get(i)?.as_deref()
            }
            for (i, l) in lyrics.lines.iter().enumerate() {
                ui.label(egui::RichText::new(&l.text).font(theme::semibold(22.0)).color(theme::TEXT));
                if let Some(r) = get(app.show_roman, extras.map(|e| &e.roman), i) {
                    ui.label(egui::RichText::new(r).font(theme::semibold(14.0)).color(theme::text_dim()));
                }
                if let Some(t) = get(app.show_trans, extras.map(|e| &e.trans), i) {
                    ui.label(egui::RichText::new(t).font(theme::regular(13.5)).color(theme::TEXT_FAINT));
                }
            }
            ui.add_space(20.0);
            let shown = extras.is_some() && (app.show_roman || app.show_trans);
            ui.label(egui::RichText::new(credit(lyrics, shown)).font(theme::regular(12.0)).color(theme::TEXT_FAINT));
        });
        return;
    }
    synced(app, ui, rect, lyrics, now, centered);
}

fn synced(app: &mut App, ui: &mut Ui, rect: Rect, lyrics: &Lyrics, now: f64, narrow: bool) {
    use crate::preset::{Align, BlurScope, Fill};
    let style = &app.look().lyrics;
    let accent = app.accent_color();
    let centered = narrow || style.align == Align::Center;
    let pos_ms = app.audio.status.position_smooth().as_millis() as u32 + 60;
    let max_scale = style.active_scale.clamp(1.0, 1.2);
    // Leave room for the active line to grow without being clipped.
    let width = ((rect.width() - 40.0) / max_scale).max(200.0);
    let size = (width / 20.0).clamp(26.0, if narrow { 46.0 } else { 36.0 }) * style.size.clamp(0.6, 1.6);
    let font = |sz: f32, bg: bool| {
        if bg { theme::lyric_font(&style.font, crate::preset::Weight::Semibold, sz * 0.62) } else { theme::lyric_font(&style.font, style.weight, sz) }
    };

    // Items: lines plus breathing gaps for long instrumental breaks.
    let mut items = Vec::new();
    let first = lyrics.lines.iter().find(|l| !l.background).map_or(0, |l| l.start);
    if first > 4500 {
        items.push(Item::Gap { start: 0, end: first });
    }
    let mut last_end = 0;
    for (i, l) in lyrics.lines.iter().enumerate() {
        if !l.background && i > 0 && l.start > last_end + 5000 {
            items.push(Item::Gap { start: last_end, end: l.start });
        }
        items.push(Item::Line(i));
        if !l.background {
            last_end = last_end.max(l.end);
        }
    }

    // Active item: latest one that has started.
    let started = |it: &Item| match it {
        Item::Line(i) => lyrics.lines[*i].start,
        Item::Gap { start, .. } => *start,
    };
    let active = items.iter().rposition(|it| started(it) <= pos_ms && !matches!(it, Item::Line(i) if lyrics.lines[*i].background));

    // Romanization and translation, laid out under their line.
    let extras = app.lyrics_extras.as_ref().map(|(_, e)| e);
    let (show_roman, show_trans) = (app.show_roman, app.show_trans);
    let sub_gap = size * 0.08;
    // Same font family as the lyrics: romanization a step lighter, translation regular.
    let roman_weight = if style.weight == crate::preset::Weight::Bold { crate::preset::Weight::Semibold } else { style.weight };
    let subs_for = |ui: &mut Ui, i: usize, bg: bool, scale: f32| -> [Option<std::sync::Arc<egui::Galley>>; 2] {
        let s = if bg { size * 0.62 } else { size } * scale;
        let get = |on: bool, v: Option<&Vec<Option<String>>>| v.filter(|_| on).and_then(|v| v.get(i).cloned().flatten());
        let roman = get(show_roman, extras.map(|e| &e.roman));
        let trans = get(show_trans, extras.map(|e| &e.trans));
        [
            roman.map(|t| ui.fonts_mut(|f| f.layout(t, theme::lyric_font(&style.font, roman_weight, s * 0.5), Color32::WHITE, width * scale))),
            trans.map(|t| ui.fonts_mut(|f| f.layout(t, theme::lyric_font(&style.font, crate::preset::Weight::Regular, s * 0.46), Color32::WHITE, width * scale))),
        ]
    };

    // Layout pass.
    let gap = size * style.line_gap.clamp(0.1, 1.6);
    let mut ys = Vec::with_capacity(items.len());
    let mut galleys = Vec::with_capacity(items.len());
    let mut subs = Vec::with_capacity(items.len());
    let mut y = 0.0;
    for it in &items {
        ys.push(y);
        match it {
            Item::Line(i) => {
                let l = &lyrics.lines[*i];
                let g = ui.fonts_mut(|f| f.layout(display_text(l), font(size, l.background), Color32::WHITE, width));
                let sg = subs_for(ui, *i, l.background, 1.0);
                let sub_h: f32 = sg.iter().flatten().map(|s| s.size().y + sub_gap).sum();
                y += g.size().y + sub_h + if l.background { gap * 0.5 } else { gap };
                galleys.push(Some(g));
                subs.push(sg);
            }
            Item::Gap { .. } => {
                y += size * 1.3;
                galleys.push(None);
                subs.push([None, None]);
            }
        }
    }

    // Smooth follow: spring the scroll toward the active item (unless the user scrolled).
    let default_anchor = (style.anchor - 0.32).abs() < 1e-3;
    let anchor_frac = if narrow && default_anchor { 0.42 } else { style.anchor.clamp(0.15, 0.6) };
    let anchor = rect.top() + rect.height() * anchor_frac;
    let target = active.map_or(0.0, |a| ys[a]) + size * 0.5;
    let sid = egui::Id::new("lyrics-scroll");
    let dt = ui.input(|i| i.stable_dt).min(0.1);
    let mut scroll: f32 = ui.data(|d| d.get_temp(sid)).unwrap_or(target);
    let hovered = ui.rect_contains_pointer(rect);
    let wheel = if hovered { ui.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
    let user_scrolling = now - app.lyrics_user_scroll <= 3.0;
    if wheel != 0.0 {
        scroll -= wheel;
        app.lyrics_user_scroll = now;
    } else if !user_scrolling {
        scroll += (target - scroll) * (1.0 - (-dt * 7.0).exp());
    }
    ui.data_mut(|d| d.insert_temp(sid, scroll));
    let origin_y = anchor - scroll;
    // Themes that hide sung lines show them again, blurred, while the user scrolls back.
    let reveal = theme::anim_bool(ui.ctx(), "lyr-reveal", user_scrolling, 0.3);

    let painter = ui.painter_at(rect);
    let fade = |yy: f32| {
        // Fade lines out toward the top/bottom edges.
        let t = ((yy - rect.top()) / 90.0).min((rect.bottom() - yy) / 120.0);
        t.clamp(0.0, 1.0)
    };
    let sung_color = crate::preset::paint(&style.sung_color, accent);
    let glow_color = crate::preset::paint(&style.glow_color, accent);
    for (k, it) in items.iter().enumerate() {
        let top = origin_y + ys[k];
        let is_active = Some(k) == active;
        match it {
            Item::Gap { start, end } => {
                let a = theme::anim_bool(ui.ctx(), ("gap", k), is_active && pos_ms < *end, 0.35);
                if a <= 0.01 {
                    continue;
                }
                let p = ((pos_ms.saturating_sub(*start)) as f32 / (end - start).max(1) as f32).clamp(0.0, 1.0);
                let breath = 1.0 + 0.12 * (now as f32 * 3.0).sin();
                let x0 = if centered { rect.center().x - 30.0 } else { rect.left() + 26.0 };
                for d in 0..3 {
                    let fill = ((p * 3.0) - d as f32).clamp(0.0, 1.0);
                    let c = pos2(x0 + d as f32 * 22.0, top + size * 0.5);
                    painter.circle_filled(c, size * 0.17 * breath * a, theme::with_alpha(Color32::WHITE, a * (0.3 + 0.7 * fill)));
                }
            }
            Item::Line(i) => {
                let l = &lyrics.lines[*i];
                let Some(base_g) = galleys[k].clone() else { continue };
                if top > rect.bottom() + size || top + base_g.size().y < rect.top() - size {
                    continue;
                }
                let sung = pos_ms >= l.start && pos_ms < l.end + 200;
                let lit = is_active || (l.background && sung);
                // A line that just finished stays fully lit while it fades, so its last
                // characters are seen highlighted even when the next line follows at once.
                let lit_amt = theme::anim_bool(ui.ctx(), ("lyr-lit", k), lit, 0.35);
                // The active line grows a little (laid out at the bigger size, with the
                // wrap width scaled to match so it breaks at the same words).
                let grow = if max_scale > 1.001 && lit_amt > 0.0 { ((1.0 + (max_scale - 1.0) * lit_amt) * 200.0).round() / 200.0 } else { 1.0 };
                let g = if grow > 1.0 {
                    ui.fonts_mut(|f| f.layout(display_text(l), font(size * grow, l.background), Color32::WHITE, width * grow))
                } else {
                    base_g.clone()
                };
                let x = if centered {
                    rect.center().x - g.size().x / 2.0
                } else if l.alt_singer {
                    rect.right() - 20.0 - g.size().x
                } else {
                    rect.left() + 20.0
                };
                // Grow from the line's anchor edge rather than its top-left.
                let pos = Pos2::new(x, top - (g.size().y - base_g.size().y) * 0.5);
                let line_rect = Rect::from_min_size(pos, g.size());
                // Where the glyphs actually are: CJK fallback glyphs sit higher and taller
                // than the Latin row box, so hover/click use the inked bounds.
                let ink = g.mesh_bounds.translate(pos.to_vec2()).union(Rect::from_x_y_ranges(line_rect.x_range(), line_rect.center().y..=line_rect.center().y));
                let edge_fade = fade(line_rect.center().y);

                let rel = active.map_or(1, |a| k as isize - a as isize);
                let dist = rel.unsigned_abs() as f32;
                let past = rel < 0;
                let upcoming_alpha = (style.inactive_alpha - dist.min(6.0) * style.falloff).max(style.min_alpha.min(style.inactive_alpha));
                let base_alpha = if lit {
                    style.active_alpha
                } else if past {
                    egui::lerp(style.past_alpha.unwrap_or(upcoming_alpha)..=upcoming_alpha, reveal)
                } else {
                    upcoming_alpha
                };
                let a = theme::anim(ui.ctx(), ("lyr-a", k), base_alpha.clamp(0.0, 1.0), 0.3) * edge_fade;
                let blur_on = match style.blur_scope {
                    BlurScope::All => true,
                    BlurScope::Past => past,
                    BlurScope::Upcoming => !past || reveal > 0.0,
                };
                let target_blur = if style.blur > 0.0 && blur_on && !lit { style.blur * (1.0 + 0.35 * (dist - 1.0).clamp(0.0, 3.0)) } else { 0.0 };
                let target_blur = target_blur.min(9.0).min(size * 0.22);
                // Blur fades in/out as a crossfade between the sharp line and a cached
                // blurred copy (radius is kept while fading back to sharp).
                let blur_key = egui::Id::new(("lyr-br", k));
                let last_r: f32 = ui.data(|d| d.get_temp(blur_key)).unwrap_or(0.0);
                if target_blur > 0.0 && (target_blur - last_r).abs() > 0.01 {
                    ui.data_mut(|d| d.insert_temp(blur_key, target_blur));
                }
                let radius = if target_blur > 0.0 { target_blur } else { last_r };
                let blur_mix = theme::anim(ui.ctx(), ("lyr-bm", k), if target_blur > 0.0 { 1.0 } else { 0.0 }, 0.35);

                let progress = if lit_amt > 0.0 {
                    if lit {
                        // Finish the fill before the next line takes over.
                        let next = lyrics.lines[*i + 1..].iter().find(|x| !x.background).map_or(u32::MAX, |x| x.start);
                        let done_by = next.saturating_sub(120);
                        match style.fill {
                            Fill::Sweep => char_progress(l, pos_ms, done_by),
                            Fill::Word => word_progress(l, pos_ms, done_by),
                            Fill::Line => f32::INFINITY,
                        }
                    } else {
                        f32::INFINITY
                    }
                } else {
                    0.0
                };
                let scale = g.size().x / base_g.size().x.max(1.0);
                let look = Paint {
                    a,
                    lit,
                    lit_amt,
                    soft: style.fill == Fill::Sweep,
                    radius,
                    blur_mix,
                    lift: vec2(0.0, -style.lift.clamp(0.0, 6.0) * lit_amt),
                    sung: theme::with_alpha(sung_color, lit_amt * edge_fade),
                    glow: (style.glow > 0.0).then(|| {
                        (style.glow.clamp(0.0, 1.0), theme::with_alpha(glow_color, lit_amt * edge_fade * (0.2 + 0.5 * style.glow.clamp(0.0, 1.0))))
                    }),
                };
                paint_text(&mut app.textfx, ui.ctx(), &painter, &g, (&base_g, grow), pos, progress, size * scale, &look);

                // Romanization and translation under the line, drawn the same way: same
                // font, scale, lift, colours and glow. Romanization follows the fill in
                // step with the line; the translation lights as a whole.
                let mut ink = ink;
                let sg = if scale > 1.001 { subs_for(ui, *i, l.background, scale) } else { subs[k].clone() };
                let glyphs = |g: &egui::Galley| g.rows.iter().map(|r| r.row.glyphs.len()).sum::<usize>() as f32;
                let frac = if progress.is_finite() { progress / glyphs(&g).max(1.0) } else { progress };
                let mut sy = line_rect.bottom().max(ink.bottom()) + sub_gap * scale;
                for (j, (sg, base_sg)) in sg.iter().zip(&subs[k]).enumerate() {
                    let (Some(sg), Some(base_sg)) = (sg, base_sg) else { continue };
                    let sx = if centered {
                        rect.center().x - sg.size().x / 2.0
                    } else if l.alt_singer {
                        rect.right() - 20.0 - sg.size().x
                    } else {
                        rect.left() + 20.0
                    };
                    let sp = pos2(sx, sy);
                    sy += sg.size().y + sub_gap * scale;
                    ink = ink.union(Rect::from_min_size(sp, sg.size()));
                    let (dim, sub_size) = if j == 0 { (0.85, size * scale * 0.5) } else { (0.7, size * scale * 0.46) };
                    let sub_progress = if j == 0 && frac.is_finite() { frac * glyphs(sg) } else if progress > 0.0 { f32::INFINITY } else { 0.0 };
                    // Small text drowns in a glow sized for the line, so it gets half as much.
                    let glow = look.glow.map(|(st, tint)| (st, theme::with_alpha(tint, 0.5)));
                    let sub = Paint { a: a * dim, radius: radius * 0.6, sung: theme::with_alpha(look.sung, dim), glow, ..look };
                    paint_text(&mut app.textfx, ui.ctx(), &painter, sg, (base_sg, if scale > 1.001 { scale } else { 1.0 }), sp, sub_progress, sub_size, &sub);
                }
                // Click a line to jump there.
                // Only inside the lyrics area: lines running past its edge mustn't take
                // clicks meant for the player bar.
                let resp = ui.interact(ink.intersect(rect), ui.id().with(("lyr", k)), Sense::click());
                let resp = named(resp, &l.text);
                if resp.hovered() {
                    painter.rect_filled(ink.expand2(vec2(10.0, 6.0)), 8.0, theme::glass(0.05));
                }
                if resp.clicked() {
                    app.actions.push(Action::Seek(l.start as f32 / 1000.0));
                }
            }
        }
    }
    painter.text(
        rect.right_bottom() - vec2(6.0, 4.0),
        Align2::RIGHT_BOTTOM,
        credit(lyrics, extras.is_some() && (show_roman || show_trans)),
        theme::regular(11.5),
        theme::TEXT_FAINT,
    );
}

/// How a lyric galley is drawn this frame.
#[derive(Clone, Copy)]
struct Paint {
    /// Alpha of unsung text.
    a: f32,
    lit: bool,
    lit_amt: f32,
    /// Feathered sweep edge (vs. a hard one).
    soft: bool,
    /// Blur radius and how far blurred it is.
    radius: f32,
    blur_mix: f32,
    lift: Vec2,
    /// Sung colour, alpha included.
    sung: Color32,
    /// Glow strength (0..1) and tint.
    glow: Option<(f32, Color32)>,
}

/// Draw one galley of a lyric line: unsung text (blurred while inactive), then the
/// sung part up to `progress` characters, glowing, lifted and feathered at the edge.
/// `fx_src`: the line at its resting size and how much `g` is grown from it. Blur and
/// glow are rasterized from that and drawn scaled, so a line growing as it lights
/// reuses one texture instead of rasterizing a new one every frame.
#[allow(clippy::too_many_arguments)]
fn paint_text(
    fx: &mut super::textfx::TextFx,
    ctx: &egui::Context,
    painter: &egui::Painter,
    g: &std::sync::Arc<egui::Galley>,
    fx_src: (&std::sync::Arc<egui::Galley>, f32),
    pos: Pos2,
    progress: f32,
    font_size: f32,
    p: &Paint,
) {
    let (src, grow) = fx_src;
    let rows = split_rows(g, pos, progress);
    if p.a > 0.003 {
        if p.lit {
            // Unsung text: only the part the fill hasn't reached.
            for r in &rows {
                let feathered = p.soft && r.edge > r.left && r.edge < r.right;
                // Past the feather the text is plainly unsung.
                let from = if feathered { r.edge + 6.0 } else { r.edge };
                if from < r.right {
                    let clip = Rect::from_min_max(pos2(from.max(r.left), r.top), pos2(r.right, r.bottom));
                    painter.with_clip_rect(clip.intersect(painter.clip_rect())).galley_with_override_text_color(pos, g.clone(), theme::with_alpha(Color32::WHITE, p.a));
                }
                // In the feather, unsung text crossfades with the sung bands drawn below,
                // at the same partial lift, so the glyph under the edge isn't seen twice.
                if feathered {
                    for b in 0..4 {
                        let x0 = r.edge - 6.0 + b as f32 * 3.0;
                        let band = Rect::from_min_max(pos2(x0, r.top), pos2(x0 + 3.0, r.bottom));
                        let t = 1.0 - (b as f32 + 0.5) / 4.0;
                        painter
                            .with_clip_rect(band.intersect(painter.clip_rect()))
                            .galley_with_override_text_color(pos + p.lift * t, g.clone(), theme::with_alpha(Color32::WHITE, p.a * (1.0 - t)));
                    }
                }
            }
        } else {
            let blurred = if p.blur_mix > 0.01 && p.radius > 0.2 { fx.get(ctx, src, p.radius / grow, 1.25) } else { None };
            match blurred {
                Some(b) => {
                    if p.blur_mix < 0.99 {
                        painter.galley_with_override_text_color(pos, g.clone(), theme::with_alpha(Color32::WHITE, p.a * (1.0 - p.blur_mix)));
                    }
                    b.paint(painter, pos, grow, theme::with_alpha(Color32::WHITE, p.a * p.blur_mix));
                }
                None => painter.galley_with_override_text_color(pos, g.clone(), theme::with_alpha(Color32::WHITE, p.a)),
            }
        }
    }
    if p.lit_amt <= 0.0 {
        return;
    }
    for r in &rows {
        let solid_end = if p.soft && r.edge < r.right { r.edge - 6.0 } else { r.edge };
        if solid_end > r.left {
            let clip = Rect::from_min_max(pos2(r.left, r.top), pos2(solid_end, r.bottom));
            if let Some((strength, tint)) = p.glow {
                // A soft halo that spreads with the theme's strength. A tight, bright one
                // fills the gaps inside letters and makes the text itself look blurred.
                let gr = (font_size * (0.12 + 0.25 * strength)).max(2.5);
                if let Some(b) = fx.get(ctx, src, gr / grow, 1.3) {
                    // Only sideways past the row: above and below are the neighbouring rows.
                    let gclip = Rect::from_min_max(pos2(r.left - gr * 2.0, r.top), pos2(solid_end, r.bottom));
                    b.paint(&painter.with_clip_rect(gclip.intersect(painter.clip_rect())), pos + p.lift, grow, tint);
                }
            }
            painter.with_clip_rect(clip.intersect(painter.clip_rect())).galley_with_override_text_color(pos + p.lift, g.clone(), p.sung);
        }
        // Feather: a few thin bands with decreasing alpha (and lift) at the edge.
        if p.soft && r.edge > r.left && r.edge < r.right {
            for b in 0..4 {
                let x0 = r.edge - 6.0 + b as f32 * 3.0;
                let band = Rect::from_min_max(pos2(x0, r.top), pos2(x0 + 3.0, r.bottom));
                let t = 1.0 - (b as f32 + 0.5) / 4.0;
                painter
                    .with_clip_rect(band.intersect(painter.clip_rect()))
                    .galley_with_override_text_color(pos + p.lift * t, g.clone(), theme::with_alpha(p.sung, t * p.sung.a() as f32 / 255.0));
            }
        }
    }
}

fn credit(lyrics: &Lyrics, extras: bool) -> String {
    if extras { format!("Lyrics from {} · Translated by Google", lyrics.source) } else { format!("Lyrics from {}", lyrics.source) }
}

fn display_text(l: &Line) -> String {
    if l.words.is_empty() { l.text.clone() } else { l.words.iter().map(|w| w.text.as_str()).collect::<String>().trim_end().to_owned() }
}

/// How many characters of the line have been sung (fractional), from word timing or,
/// for line-synced lyrics, spread evenly across the line like better-lyrics does.
/// Word fills are squeezed to end by `done_by` (just before the next line starts).
fn char_progress(l: &Line, pos: u32, done_by: u32) -> f32 {
    if l.words.is_empty() {
        let n = l.text.chars().count() as f32;
        let span = ((l.end.saturating_sub(l.start)) as f32 * 0.9).min(n * 90.0).max(1.0);
        return n * ((pos.saturating_sub(l.start)) as f32 / span).clamp(0.0, 1.0);
    }
    let mut chars = 0.0;
    for w in &l.words {
        let n = w.text.chars().count() as f32;
        let (start, end) = (w.start.min(done_by), w.end.min(done_by));
        if pos >= end {
            chars += n;
        } else if pos >= start {
            let f = (pos - start) as f32 / (end - start).max(1) as f32;
            // Count trailing space only once the word is done.
            let letters = w.text.trim_end().chars().count() as f32;
            chars += letters * f;
            break;
        } else {
            break;
        }
    }
    chars
}

/// Like `char_progress`, but whole words at a time: a word counts as sung the
/// moment it starts (opacity-based karaoke).
fn word_progress(l: &Line, pos: u32, done_by: u32) -> f32 {
    if l.words.is_empty() {
        // Line-synced: snap the even spread up to the end of the current word.
        let p = char_progress(l, pos, done_by);
        if p <= 0.0 {
            return 0.0;
        }
        let chars: Vec<char> = l.text.chars().collect();
        let mut i = (p.floor() as usize).min(chars.len());
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        return i as f32;
    }
    l.words.iter().take_while(|w| w.start.min(done_by) <= pos).map(|w| w.text.trim_end().chars().count() as f32 + (w.text.len() - w.text.trim_end().len()) as f32).sum()
}

/// One wrapped row of a lyric line and how far along it the fill has reached.
struct RowSplit {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    /// x where the sung part ends (`left` = nothing sung, `right` = all of it).
    edge: f32,
}

/// Split a line's rows at the fill position (`progress` characters).
fn split_rows(g: &std::sync::Arc<egui::Galley>, pos: Pos2, progress: f32) -> Vec<RowSplit> {
    let mut out = Vec::with_capacity(g.rows.len());
    let mut remaining = progress;
    let last = g.rows.len().saturating_sub(1);
    // Where each row's glyphs are inked. Row boxes come from the Latin font, and CJK
    // fallback glyphs stand taller, poking out above and below them. Rows meet halfway
    // between their inked glyphs, so a row's clip never reaches its neighbours' glyphs
    // (which would paint them twice: lit and unlit, a lift apart).
    let ink: Vec<(f32, f32)> = g
        .rows
        .iter()
        .map(|row| {
            let y = pos.y + row.pos.y;
            let (mut t, mut b) = (f32::INFINITY, f32::NEG_INFINITY);
            for gl in &row.row.glyphs {
                if gl.uv_rect.size.y > 0.0 {
                    t = t.min(y + gl.pos.y + gl.uv_rect.offset.y);
                    b = b.max(y + gl.pos.y + gl.uv_rect.offset.y + gl.uv_rect.size.y);
                }
            }
            if t.is_finite() { (t.min(y), b.max(y + row.row.size.y)) } else { (y, y + row.row.size.y) }
        })
        .collect();
    for (r, row) in g.rows.iter().enumerate() {
        let n = row.row.glyphs.len() as f32;
        let row_rect = Rect::from_min_size(pos + row.pos.to_vec2(), row.row.size);
        // The outer edges stay open (lift, tall glyphs).
        let top = if r == 0 { row_rect.top() - row.row.size.y } else { (ink[r - 1].1 + ink[r].0) / 2.0 };
        let bottom = if r == last { row_rect.bottom() + row.row.size.y } else { (ink[r].1 + ink[r + 1].0) / 2.0 };
        let (left, right) = (row_rect.left() - 4.0, row_rect.right() + 4.0);
        let edge = if n == 0.0 || remaining <= 0.0 {
            left
        } else if remaining >= n {
            right
        } else {
            let idx = remaining.floor() as usize;
            let frac = remaining - idx as f32;
            row_rect.left() + row.row.glyphs.get(idx).map_or(row.row.size.x, |gl| gl.pos.x + gl.advance_width * frac)
        };
        // Row breaks consume the implicit newline/space between rows.
        remaining -= n;
        out.push(RowSplit { left, right, top, bottom, edge });
    }
    out
}
