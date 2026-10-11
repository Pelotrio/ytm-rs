//! Real blur and glow for lyric lines. egui can't blur, and stacking offset copies of
//! the text reads as doubled outlines (worst on thin CJK strokes). Instead a line's
//! glyphs are copied out of egui's font atlas into a small coverage image, blurred on
//! the CPU (three box passes ≈ Gaussian) and cached as a texture per galley + radius.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{Color32, ColorImage, Galley, Pos2, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2};

/// A blurred line, positioned relative to the galley's top-left.
#[derive(Clone)]
pub struct Blurred {
    tex: TextureHandle,
    /// Offset from the galley origin to the image's top-left, in points.
    offset: Vec2,
    size: Vec2,
}

impl Blurred {
    pub fn paint(&self, painter: &egui::Painter, galley_pos: Pos2, tint: Color32) {
        let rect = Rect::from_min_size(galley_pos + self.offset, self.size);
        painter.image(self.tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
    }
}

#[derive(Default)]
pub struct TextFx {
    cache: HashMap<(u64, u32), (Blurred, f64)>,
    /// The font atlas copied this frame (only when something had to be rasterized).
    atlas: Option<(u64, Arc<ColorImage>)>,
    built_this_frame: usize,
    frame: u64,
}

/// Rasterizations allowed per frame; the rest fall back to sharp text for a frame.
const BUDGET: usize = 6;

impl TextFx {
    /// The line blurred by `radius` points, if available (rasterized on first use).
    /// `gain` brightens the result (glows want more than the soft blur gives).
    pub fn get(&mut self, ctx: &egui::Context, g: &Arc<Galley>, radius: f32, gain: f32) -> Option<Blurred> {
        let frame = ctx.cumulative_frame_nr();
        let now = ctx.input(|i| i.time);
        if frame != self.frame {
            self.frame = frame;
            self.built_this_frame = 0;
            // Drop textures not drawn for a while (lines scrolled away, old songs).
            self.cache.retain(|_, (_, used)| now - *used < 4.0);
        }
        let ppp = ctx.pixels_per_point();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        g.job.hash(&mut h);
        ppp.to_bits().hash(&mut h);
        gain.to_bits().hash(&mut h);
        let key = (h.finish(), (radius * 4.0).round() as u32);
        if let Some((b, used)) = self.cache.get_mut(&key) {
            *used = now;
            return Some(b.clone());
        }
        if self.built_this_frame >= BUDGET {
            ctx.request_repaint();
            return None;
        }
        self.built_this_frame += 1;
        let atlas = match &self.atlas {
            Some((f, a)) if *f == frame => a.clone(),
            _ => {
                let a = Arc::new(ctx.fonts(|f| f.image()));
                self.atlas = Some((frame, a.clone()));
                a
            }
        };
        let (img, offset, size) = rasterize(g, &atlas, ppp, radius, gain);
        let tex = ctx.load_texture("lyric-fx", img, TextureOptions::LINEAR);
        let b = Blurred { tex, offset, size };
        self.cache.insert(key, (b.clone(), now));
        Some(b)
    }
}

fn rasterize(g: &Galley, atlas: &ColorImage, ppp: f32, radius: f32, gain: f32) -> (ColorImage, Vec2, Vec2) {
    let pad = radius * 2.0 + 2.0;
    let bounds = g.mesh_bounds.union(Rect::from_min_size(Pos2::ZERO, g.size())).expand(pad);
    let w = (bounds.width() * ppp).ceil() as usize;
    let h = (bounds.height() * ppp).ceil() as usize;
    let mut buf = vec![0.0f32; w * h];
    let aw = atlas.size[0];
    for row in &g.rows {
        for glyph in &row.row.glyphs {
            let uv = glyph.uv_rect;
            if uv.is_nothing() {
                continue;
            }
            let lt = row.pos + glyph.pos.to_vec2() + uv.offset;
            let x0 = ((lt.x - bounds.left()) * ppp).round() as isize;
            let y0 = ((lt.y - bounds.top()) * ppp).round() as isize;
            for ty in 0..(uv.max[1] - uv.min[1]) as usize {
                let y = y0 + ty as isize;
                if y < 0 || y >= h as isize {
                    continue;
                }
                let src = (uv.min[1] as usize + ty) * aw + uv.min[0] as usize;
                for tx in 0..(uv.max[0] - uv.min[0]) as usize {
                    let x = x0 + tx as isize;
                    if x < 0 || x >= w as isize {
                        continue;
                    }
                    let a = atlas.pixels.get(src + tx).map_or(0, |p| p.a()) as f32 / 255.0;
                    let d = &mut buf[y as usize * w + x as usize];
                    *d = d.max(a);
                }
            }
        }
    }
    // Three box passes of radius r approximate a Gaussian with sigma ≈ r.
    let r = ((radius * ppp) * 0.75).round().max(1.0) as usize;
    let mut tmp = vec![0.0f32; w * h];
    for _ in 0..3 {
        box_pass(&buf, &mut tmp, w, h, r, true);
        box_pass(&tmp, &mut buf, w, h, r, false);
    }
    let pixels = buf
        .iter()
        .map(|v| {
            let a = ((v * gain).min(1.0) * 255.0).round() as u8;
            Color32::from_rgba_premultiplied(a, a, a, a)
        })
        .collect();
    let img = ColorImage { size: [w, h], source_size: vec2(w as f32, h as f32), pixels };
    (img, bounds.min.to_vec2(), vec2(w as f32, h as f32) / ppp)
}

/// One running-sum box blur pass along rows (`horizontal`) or columns.
fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    let (n, lines) = if horizontal { (w, h) } else { (h, w) };
    let idx = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut acc = 0.0;
        for i in 0..=r.min(n - 1) {
            acc += src[idx(line, i)];
        }
        for i in 0..n {
            dst[idx(line, i)] = acc * norm;
            if i + r + 1 < n {
                acc += src[idx(line, i + r + 1)];
            }
            if i >= r {
                acc -= src[idx(line, i - r)];
            }
        }
    }
}
