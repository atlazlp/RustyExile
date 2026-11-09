use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};
use poe_core::{EventKind, HudCorner, Theme, VisibleLine};
use std::collections::HashMap;
use tiny_skia::{Color as SkColor, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

#[derive(Clone)]
pub struct Hit {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub config: bool,
}

pub struct ButtonSprite {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub hits: Vec<Hit>,
    pub panel: [i32; 4],
    pub button: Option<ButtonSprite>,
}

pub struct Painter {
    font_system: FontSystem,
    cache: SwashCache,
    icons: HashMap<String, Pixmap>,
}

impl Painter {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            cache: SwashCache::new(),
            icons: HashMap::new(),
        }
    }

    pub fn paint(
        &mut self,
        width: u32,
        height: u32,
        lines: &[VisibleLine],
        theme: &Theme,
        opacity: f32,
        scale: f32,
        corner: HudCorner,
        show_button: bool,
    ) -> Frame {
        let width = width.max(1);
        let height = height.max(1);
        let scale = scale.clamp(0.6, 2.5);
        let opacity = opacity.clamp(0.15, 1.0);
        let margin = (16.0 * scale) as i32;
        let icon_px = (22.0 * scale) as u32;
        let row_h = (icon_px + 10).max(28);
        let panel_w = ((430.0 * scale) as u32).min(width.saturating_sub(margin as u32 * 2).max(1));
        let mut y_cursor = 10u32;
        let mut rows: Vec<RowDraw> = Vec::new();
        for line in lines {
            let text_h = if line.detail.is_empty() { row_h } else { row_h + (14.0 * scale) as u32 };
            rows.push(RowDraw {
                line,
                h: text_h,
            });
            y_cursor += text_h;
        }
        let panel_h = (y_cursor + 12).clamp(36, height.saturating_sub(margin as u32 * 2).max(36));
        let (px, py) = place(corner, width, height, panel_w, panel_h, margin);
        let panel_h = panel_h.min(height);
        let mut pixmap = Pixmap::new(panel_w, panel_h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
        fill_round(
            &mut pixmap,
            0.0,
            0.0,
            panel_w as f32,
            panel_h as f32,
            10.0 * scale,
            premul(theme.background, opacity),
        );
        let mut y = 8u32;
        let mut hits = Vec::new();
        for row in rows {
            if y + row.h > panel_h {
                break;
            }
            let icon_key = if row.line.is_header {
                "waypoint"
            } else if row.line.kind == EventKind::GemBuy {
                row.line.icon.as_str()
            } else {
                row.line.icon.as_str()
            };
            self.blit_icon(&mut pixmap, icon_key, 8, y as i32 + 4, icon_px);
            if row.line.kind == EventKind::GemBuy {
                self.blit_icon(&mut pixmap, "wisdom", 8 + icon_px as i32 - 8, y as i32 + icon_px as i32 - 6, 12);
            }
            let color = if row.line.is_header { theme.accent } else { theme.text };
            self.draw_text(
                &mut pixmap,
                &row.line.text,
                (12 + icon_px as i32) as f32,
                y as f32 + 2.0,
                (panel_w - icon_px - 24) as f32,
                16.0 * scale,
                color,
                opacity,
            );
            if !row.line.detail.is_empty() {
                self.draw_text(
                    &mut pixmap,
                    &row.line.detail,
                    (12 + icon_px as i32) as f32,
                    y as f32 + 18.0 * scale,
                    (panel_w - icon_px - 24) as f32,
                    12.0 * scale,
                    theme.muted,
                    opacity,
                );
            }
            if !row.line.is_header {
                hits.push(Hit {
                    id: row.line.event_id.clone(),
                    x: 0,
                    y: y as i32,
                    w: panel_w as i32,
                    h: row.h as i32,
                    config: false,
                });
            }
            y += row.h;
        }
        let button = if show_button {
            let bw = (120.0 * scale) as u32;
            let bh = (28.0 * scale) as u32;
            let mut button_px = Pixmap::new(bw, bh).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
            fill_round(&mut button_px, 0.0, 0.0, bw as f32, bh as f32, 8.0, premul(theme.background, opacity));
            self.draw_text(
                &mut button_px,
                "Config",
                28.0,
                4.0,
                bw as f32 - 36.0,
                14.0 * scale,
                theme.accent,
                opacity,
            );
            self.blit_icon(&mut button_px, "waypoint", 6, 4, bh.saturating_sub(8));
            Some(ButtonSprite {
                x: (width as i32 - bw as i32) / 2,
                y: 8,
                w: bw,
                h: bh,
                bgra: rgba_to_bgra(button_px.data()),
            })
        } else {
            None
        };
        Frame {
            width: pixmap.width(),
            height: pixmap.height(),
            bgra: rgba_to_bgra(pixmap.data()),
            hits,
            panel: [px, py, pixmap.width() as i32, pixmap.height() as i32],
            button,
        }
    }

    fn blit_icon(&mut self, pixmap: &mut Pixmap, key: &str, x: i32, y: i32, size: u32) {
        if size == 0 {
            return;
        }
        let icon = self.icon(key);
        let Some(icon) = icon else {
            return;
        };
        let paint = tiny_skia::PixmapPaint::default();
        let sx = size as f32 / icon.width() as f32;
        let sy = size as f32 / icon.height() as f32;
        pixmap.draw_pixmap(
            0,
            0,
            icon.as_ref(),
            &paint,
            Transform::from_scale(sx, sy).post_translate(x as f32, y as f32),
            None,
        );
    }

    fn icon(&mut self, key: &str) -> Option<&Pixmap> {
        if !self.icons.contains_key(key) {
            if let Some(image) = poe_data::load_icon(key) {
                if let Some(pm) = rgba_image_to_pixmap(&image) {
                    self.icons.insert(key.to_string(), pm);
                }
            }
        }
        self.icons.get(key)
    }

    fn draw_text(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        width: f32,
        size: f32,
        color: [u8; 4],
        opacity: f32,
    ) {
        let metrics = Metrics::new(size, size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        {
            let mut borrowed = buffer.borrow_with(&mut self.font_system);
            borrowed.set_size(Some(width), Some(size * 3.0));
            let attrs = Attrs::new().family(Family::SansSerif);
            borrowed.set_text(text, &attrs, Shaping::Advanced);
            borrowed.shape_until_scroll(false);
        }
        let text_color = Color::rgba(color[0], color[1], color[2], (color[3] as f32 * opacity) as u8);
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((x, y), 1.0);
                self.cache.with_pixels(
                    &mut self.font_system,
                    physical.cache_key,
                    text_color,
                    |gx, gy, pixel| {
                        let px = physical.x + gx;
                        let py = run.line_y as i32 + physical.y + gy;
                        blend(pixmap, px, py, pixel.0);
                    },
                );
            }
        }
    }
}

struct RowDraw<'a> {
    line: &'a VisibleLine,
    h: u32,
}

fn place(corner: HudCorner, sw: u32, sh: u32, pw: u32, ph: u32, margin: i32) -> (i32, i32) {
    let max_x = sw.saturating_sub(pw) as i32;
    let max_y = sh.saturating_sub(ph) as i32;
    match corner {
        HudCorner::TopLeft => (margin, margin),
        HudCorner::TopRight => (max_x - margin, margin),
        HudCorner::BottomLeft => (margin, max_y - margin),
        HudCorner::BottomRight => (max_x - margin, max_y - margin),
    }
}

fn premul(color: [u8; 4], opacity: f32) -> SkColor {
    let a = color[3] as f32 / 255.0 * opacity;
    SkColor::from_rgba8(
        (color[0] as f32 * a) as u8,
        (color[1] as f32 * a) as u8,
        (color[2] as f32 * a) as u8,
        (a * 255.0) as u8,
    )
}

fn fill_round(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, radius: f32, color: SkColor) {
    let mut paint = Paint::default();
    paint.set_color(color);
    let rect = Rect::from_xywh(x, y, w, h).unwrap_or(Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap());
    let path = PathBuilder::from_rect(rect);
    pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    let _ = radius;
}

fn rgba_image_to_pixmap(image: &image::RgbaImage) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(image.width(), image.height())?;
    for (i, pixel) in image.pixels().enumerate() {
        let a = pixel[3] as u16;
        let r = (pixel[0] as u16 * a / 255) as u8;
        let g = (pixel[1] as u16 * a / 255) as u8;
        let b = (pixel[2] as u16 * a / 255) as u8;
        let offset = i * 4;
        pixmap.data_mut()[offset] = r;
        pixmap.data_mut()[offset + 1] = g;
        pixmap.data_mut()[offset + 2] = b;
        pixmap.data_mut()[offset + 3] = pixel[3];
    }
    Some(pixmap)
}

fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; rgba.len()];
    for (src, dst) in rgba.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
    out
}

fn blend(pixmap: &mut Pixmap, x: i32, y: i32, pixel: u32) {
    if x < 0 || y < 0 || x as u32 >= pixmap.width() || y as u32 >= pixmap.height() {
        return;
    }
    let idx = (y as u32 * pixmap.width() + x as u32) as usize * 4;
    let data = pixmap.data_mut();
    let sa = ((pixel >> 24) & 0xff) as u32;
    if sa == 0 {
        return;
    }
    let sr = (pixel >> 16) & 0xff;
    let sg = (pixel >> 8) & 0xff;
    let sb = pixel & 0xff;
    let inv = 255 - sa;
    data[idx] = ((sr * sa + data[idx] as u32 * inv) / 255) as u8;
    data[idx + 1] = ((sg * sa + data[idx + 1] as u32 * inv) / 255) as u8;
    data[idx + 2] = ((sb * sa + data[idx + 2] as u32 * inv) / 255) as u8;
    data[idx + 3] = (sa + data[idx + 3] as u32 * inv / 255) as u8;
}
