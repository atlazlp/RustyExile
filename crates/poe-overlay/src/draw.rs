use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use poe_core::{parse_markup, tone_rgb, EventKind, HudCorner, TextTone, Theme, VisibleLine};
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
        custom: Option<(i32, i32)>,
        placing: bool,
    ) -> Frame {
        let width = width.max(1);
        let height = height.max(1);
        let scale = scale.clamp(0.6, 2.5);
        let opacity = opacity.clamp(0.15, 1.0);
        let margin = (16.0 * scale) as i32;
        let icon_px = (22.0 * scale) as u32;
        let row_h = (icon_px + 10).max(28);
        let panel_w = ((300.0 * scale) as u32).min(width.saturating_sub(margin as u32 * 2).max(1));
        let confirm_h = if placing { ((32.0 * scale) as u32).max(28) } else { 0 };
        let text_w = (panel_w as f32 - icon_px as f32 - 24.0).max(40.0);
        let header_w = (panel_w as f32 - 24.0).max(40.0);
        let mut y_cursor = 10u32;
        let mut rows: Vec<RowDraw> = Vec::new();
        for line in lines {
            let measure_w = if line.is_header { header_w } else { text_w };
            let text_lines = self
                .text_line_count(&line.text, measure_w, 16.0 * scale, line.is_header)
                .clamp(1, 2);
            let text_h = if line.is_header {
                (26.0 * scale) as u32 + if text_lines > 1 { (16.0 * scale) as u32 } else { 0 }
            } else if text_lines > 1 {
                row_h + (18.0 * scale) as u32
            } else {
                row_h
            };
            let text_h = if line.detail.is_empty() {
                text_h
            } else {
                text_h + (14.0 * scale) as u32
            };
            rows.push(RowDraw {
                line,
                h: text_h,
                text_lines,
                sep: 0,
            });
            y_cursor += text_h;
        }
        let sep_h = (9.0 * scale).round().max(7.0) as u32;
        for index in 0..rows.len() {
            if rows[index].line.is_header || index + 1 == rows.len() {
                continue;
            }
            rows[index].sep = sep_h;
            y_cursor += sep_h;
        }
        let panel_h = (y_cursor + 12 + confirm_h).clamp(36 + confirm_h, height.saturating_sub(margin as u32 * 2).max(36 + confirm_h));
        let (px, py) = place(corner, width, height, panel_w, panel_h, margin, custom, placing);
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
        let content_bottom = panel_h.saturating_sub(confirm_h);
        for row in rows {
            if y + row.h > content_bottom {
                break;
            }
            if row.line.is_header {
                let (_, line_w) = self.draw_text(
                    &mut pixmap,
                    &row.line.text,
                    12.0,
                    y as f32 + 2.0,
                    header_w,
                    16.0 * scale,
                    theme.accent,
                    theme.accent,
                    opacity,
                    true,
                    true,
                    2,
                    None,
                );
                let underline_x = 12.0 + (header_w - line_w).max(0.0) / 2.0;
                let underline_y = y as f32 + 18.0 * scale;
                let mut paint = Paint::default();
                paint.set_color(premul(theme.accent, opacity));
                if let Some(rect) = Rect::from_xywh(underline_x, underline_y, line_w.max(8.0), (2.0 * scale).max(1.0)) {
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
                y += row.h;
                continue;
            }
            let text_block = if row.text_lines > 1 {
                row_h + (18.0 * scale) as u32
            } else {
                row_h
            };
            let mid = y as f32 + text_block as f32 / 2.0;
            let icon_y = (mid - icon_px as f32 / 2.0).round() as i32;
            self.blit_icon(&mut pixmap, row.line.icon.as_str(), 8, icon_y, icon_px);
            if row.line.kind == EventKind::GemBuy {
                self.blit_icon(&mut pixmap, "wisdom", 8 + icon_px as i32 - 8, icon_y + icon_px as i32 - 6, 12);
            }
            self.draw_text(
                &mut pixmap,
                &row.line.text,
                (12 + icon_px as i32) as f32,
                y as f32,
                text_w,
                16.0 * scale,
                theme.text,
                theme.accent,
                opacity,
                false,
                false,
                2,
                Some(text_block as f32),
            );
            if !row.line.detail.is_empty() {
                self.draw_text(
                    &mut pixmap,
                    &row.line.detail,
                    (12 + icon_px as i32) as f32,
                    y as f32 + text_block as f32 + 1.0,
                    (panel_w - icon_px - 24) as f32,
                    12.0 * scale,
                    theme.muted,
                    theme.accent,
                    opacity,
                    false,
                    false,
                    2,
                    None,
                );
            }
            if !placing && !row.line.is_header {
                hits.push(Hit {
                    id: row.line.event_id.clone(),
                    x: 0,
                    y: y as i32,
                    w: panel_w as i32,
                    h: (row.h + row.sep) as i32,
                    config: false,
                });
            }
            if row.sep > 0 {
                let line_y = (y + row.h) as f32 + (row.sep as f32 - 1.0) / 2.0;
                let inset = 16.0 * scale;
                let mut paint = Paint::default();
                paint.set_color(premul(theme.text, opacity * 0.16));
                if let Some(rect) = Rect::from_xywh(inset, line_y, (panel_w as f32 - inset * 2.0).max(1.0), 1.0) {
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
            y += row.h + row.sep;
        }
        if placing {
            let by = content_bottom as f32;
            fill_round(
                &mut pixmap,
                0.0,
                by,
                panel_w as f32,
                confirm_h as f32,
                0.0,
                premul(theme.accent, opacity * 0.85),
            );
            self.draw_text(
                &mut pixmap,
                "Confirm",
                14.0,
                by + 6.0,
                panel_w as f32 - 28.0,
                15.0 * scale,
                theme.background,
                theme.accent,
                1.0,
                false,
                false,
                1,
                None,
            );
            hits.insert(
                0,
                Hit {
                    id: "confirm".into(),
                    x: 0,
                    y: content_bottom as i32,
                    w: panel_w as i32,
                    h: confirm_h as i32,
                    config: false,
                },
            );
            hits.push(Hit {
                id: "move".into(),
                x: 0,
                y: 0,
                w: panel_w as i32,
                h: panel_h as i32,
                config: false,
            });
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
                theme.accent,
                opacity,
                false,
                false,
                1,
                None,
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

    fn text_line_count(&mut self, text: &str, width: f32, size: f32, bold: bool) -> u32 {
        let buffer = self.shaped(text, width, size, bold, 2, [236, 228, 210, 255], [196, 148, 74, 255], 1.0);
        buffer.layout_runs().count().max(1) as u32
    }

    fn shaped(
        &mut self,
        text: &str,
        width: f32,
        size: f32,
        bold: bool,
        max_lines: usize,
        color: [u8; 4],
        accent: [u8; 4],
        opacity: f32,
    ) -> Buffer {
        let metrics = Metrics::new(size, size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        let pieces = colored_pieces(text, color, accent, opacity);
        {
            let mut borrowed = buffer.borrow_with(&mut self.font_system);
            borrowed.set_size(Some(width), Some(size * 1.35 * max_lines as f32));
            let attrs = if bold {
                Attrs::new().family(Family::SansSerif).weight(Weight::BOLD)
            } else {
                Attrs::new().family(Family::SansSerif)
            };
            let spans: Vec<(&str, Attrs)> = pieces
                .iter()
                .map(|(piece, piece_color)| (piece.as_str(), attrs.clone().color(*piece_color)))
                .collect();
            if spans.is_empty() {
                borrowed.set_text("", &attrs, Shaping::Advanced);
            } else {
                borrowed.set_rich_text(spans, &attrs, Shaping::Advanced, None);
            }
            borrowed.shape_until_scroll(false);
        }
        buffer
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
        accent: [u8; 4],
        opacity: f32,
        bold: bool,
        center: bool,
        max_lines: usize,
        box_h: Option<f32>,
    ) -> (u32, f32) {
        let buffer = self.shaped(text, width, size, bold, max_lines, color, accent, opacity);
        let fallback = Color::rgba(color[0], color[1], color[2], (color[3] as f32 * opacity) as u8);
        let y = if let Some(box_h) = box_h {
            let mut top = None;
            let mut bottom = 0.0;
            let mut seen = 0usize;
            for run in buffer.layout_runs() {
                if seen >= max_lines {
                    break;
                }
                if top.is_none() {
                    top = Some(run.line_top);
                }
                bottom = run.line_top + run.line_height;
                seen += 1;
            }
            let center_y = (top.unwrap_or(0.0) + bottom) / 2.0;
            y + box_h / 2.0 - center_y
        } else {
            y
        };
        let mut count = 0u32;
        let mut widest = 0.0f32;
        for run in buffer.layout_runs() {
            if count as usize >= max_lines {
                break;
            }
            let draw_x = if center { x + (width - run.line_w).max(0.0) / 2.0 } else { x };
            widest = widest.max(run.line_w);
            for glyph in run.glyphs.iter() {
                let physical = glyph.physical((draw_x, y), 1.0);
                let glyph_color = glyph.color_opt.unwrap_or(fallback);
                self.cache.with_pixels(
                    &mut self.font_system,
                    physical.cache_key,
                    glyph_color,
                    |gx, gy, pixel| {
                        let px = physical.x + gx;
                        let py = run.line_y as i32 + physical.y + gy;
                        blend(pixmap, px, py, pixel.0);
                    },
                );
            }
            count += 1;
        }
        (count.max(1), widest)
    }
}

fn colored_pieces(text: &str, color: [u8; 4], accent: [u8; 4], opacity: f32) -> Vec<(String, Color)> {
    let spans = parse_markup(text);
    if spans.is_empty() {
        return vec![(text.to_string(), rgba(color, opacity))];
    }
    spans
        .into_iter()
        .filter(|span| !span.text.is_empty())
        .map(|span| {
            let rgb = match span.tone {
                TextTone::Plain => color,
                TextTone::Location => accent,
                _ => tone_rgb(span.tone).map(|[r, g, b]| [r, g, b, 255]).unwrap_or(color),
            };
            (span.text, rgba(rgb, opacity))
        })
        .collect()
}

fn rgba(color: [u8; 4], opacity: f32) -> Color {
    Color::rgba(color[0], color[1], color[2], (color[3] as f32 * opacity) as u8)
}

struct RowDraw<'a> {
    line: &'a VisibleLine,
    h: u32,
    text_lines: u32,
    sep: u32,
}

fn place(
    corner: HudCorner,
    sw: u32,
    sh: u32,
    pw: u32,
    ph: u32,
    margin: i32,
    custom: Option<(i32, i32)>,
    placing: bool,
) -> (i32, i32) {
    let max_x = sw.saturating_sub(pw) as i32;
    let max_y = sh.saturating_sub(ph) as i32;
    let clamp = |x: i32, y: i32| (x.clamp(0, max_x.max(0)), y.clamp(0, max_y.max(0)));
    if placing {
        if let Some((x, y)) = custom {
            return clamp(x, y);
        }
    }
    match corner {
        HudCorner::TopLeft => (margin, margin),
        HudCorner::TopRight => (max_x - margin, margin),
        HudCorner::BottomLeft => (margin, max_y - margin),
        HudCorner::BottomRight => (max_x - margin, max_y - margin),
        HudCorner::Custom => custom.map(|(x, y)| clamp(x, y)).unwrap_or((max_x - margin, margin)),
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
