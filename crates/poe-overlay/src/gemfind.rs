use image::{imageops::FilterType, RgbaImage, RgbImage};
use poe_core::{GemSpot, VisibleLine};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const GEM_SCALES: [f32; 8] = [0.5, 0.65, 0.8, 0.95, 1.1, 1.3, 1.55, 1.85];
const CREST_SCALES: [f32; 7] = [1.0, 1.4, 1.75, 2.05, 2.5, 3.1, 3.8];
const BANNER_SCALES: [f32; 7] = [0.67, 0.8, 1.0, 1.15, 1.35, 1.6, 2.0];
const GEM_LIMIT: f32 = 34.0;
const CREST_LIMIT: f32 = 46.0;
const BANNER_LIMIT: f32 = 40.0;

pub struct Needle {
    pub event_id: String,
    pub rgba: RgbaImage,
    pub vendor: bool,
    pub cost_icon: String,
}

#[derive(Clone, Debug)]
pub struct GemMark {
    pub event_id: String,
    pub x: i32,
    pub y: i32,
    pub side: u32,
    pub cost_icon: String,
    score: f32,
}

pub struct GemJob {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub window_w: u32,
    pub window_h: u32,
    pub needles: Vec<Needle>,
}

pub enum GemPass {
    Idle,
    Pending,
    Ready(Vec<GemMark>),
}

pub struct GemWatch {
    jobs: SyncSender<GemJob>,
    marks: Receiver<Vec<GemMark>>,
}

impl GemWatch {
    pub fn spawn(
        capture: impl Fn(i32, i32, u32, u32) -> Option<RgbImage> + Send + 'static,
    ) -> Self {
        let (jobs_tx, jobs_rx) = mpsc::sync_channel::<GemJob>(1);
        let (marks_tx, marks_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("poe-gems".into())
            .spawn(move || {
                let mut previous = Vec::new();
                while let Ok(job) = jobs_rx.recv() {
                    let shot = capture(job.x, job.y, job.width, job.height);
                    let fresh = shot
                        .as_ref()
                        .map(|image| {
                            locate(
                                image,
                                job.window_w,
                                job.window_h,
                                job.x,
                                job.y,
                                &job.needles,
                            )
                        })
                        .unwrap_or_default();
                    let shown = hold(
                        &mut previous,
                        fresh,
                        shot.as_ref(),
                        job.x,
                        job.y,
                        &job.needles,
                    );
                    let _ = marks_tx.send(shown);
                }
            })
            .ok();
        Self {
            jobs: jobs_tx,
            marks: marks_rx,
        }
    }

    pub fn submit(&self, job: GemJob) -> bool {
        !matches!(self.jobs.try_send(job), Err(TrySendError::Full(_)))
    }

    pub fn poll(&self) -> Option<Vec<GemMark>> {
        self.marks.try_recv().ok()
    }

    pub fn advance(
        &self,
        last: &mut Instant,
        town: bool,
        x: i32,
        y: i32,
        window_w: u32,
        window_h: u32,
        lines: &[VisibleLine],
    ) -> GemPass {
        if !town || !lines.iter().any(gem_line) {
            while self.poll().is_some() {}
            return GemPass::Idle;
        }
        if last.elapsed() > Duration::from_millis(450) {
            *last = Instant::now();
            let (width, height) = search_size(window_w, window_h);
            let _ = self.submit(GemJob {
                x,
                y,
                width,
                height,
                window_w,
                window_h,
                needles: needles_from(lines),
            });
        }
        self.poll()
            .map(|marks| GemPass::Ready(live_marks(marks, lines)))
            .unwrap_or(GemPass::Pending)
    }
}

pub fn search_size(window_w: u32, window_h: u32) -> (u32, u32) {
    ((window_w * 72 / 100).max(1), (window_h * 82 / 100).max(1))
}

pub fn spots(marks: &[GemMark]) -> Vec<GemSpot> {
    marks
        .iter()
        .map(|mark| GemSpot {
            event_id: mark.event_id.clone(),
            x: mark.x - 4,
            y: mark.y - 4,
            w: mark.side as i32 + 20,
            h: mark.side as i32 + 20,
        })
        .collect()
}

fn gem_line(line: &VisibleLine) -> bool {
    line.kind.is_gem() && !line.is_header
}

fn live_marks(marks: Vec<GemMark>, lines: &[VisibleLine]) -> Vec<GemMark> {
    marks
        .into_iter()
        .filter(|mark| {
            lines
                .iter()
                .any(|line| gem_line(line) && line.event_id == mark.event_id)
        })
        .collect()
}

pub fn needles_from(lines: &[VisibleLine]) -> Vec<Needle> {
    let mut out = Vec::new();
    for line in lines {
        if !gem_line(line) {
            continue;
        }
        let Some(icon) = poe_data::load_icon(&line.icon) else {
            continue;
        };
        out.push(Needle {
            event_id: line.event_id.clone(),
            rgba: icon,
            vendor: line.kind == poe_core::EventKind::GemBuy,
            cost_icon: line.kind.shown_cost(&line.cost_icon).to_string(),
        });
        if out.len() == 24 {
            break;
        }
    }
    out
}

pub fn locate(
    image: &RgbImage,
    _window_w: u32,
    window_h: u32,
    origin_x: i32,
    origin_y: i32,
    needles: &[Needle],
) -> Vec<GemMark> {
    let mut found = Vec::new();
    if let Some(plaque) = find_templates(image, window_h, banners()) {
        push_matches(
            &mut found,
            image,
            needles,
            below_plaque(image, plaque),
            true,
            origin_x,
            origin_y,
        );
    } else {
        if let Some(region) = band(image, window_h, 20, 80) {
            push_matches(&mut found, image, needles, region, false, origin_x, origin_y);
        }
        if let Some(plaque) = find_templates(image, window_h, purchase_crests()) {
            push_matches(
                &mut found,
                image,
                needles,
                below_plaque(image, plaque),
                true,
                origin_x,
                origin_y,
            );
        }
    }
    found.sort_by(|a, b| a.score.total_cmp(&b.score));
    let mut kept = Vec::new();
    for hit in found {
        let crowded = kept.iter().any(|other: &GemMark| {
            let ax = hit.x + hit.side as i32 / 2;
            let ay = hit.y + hit.side as i32 / 2;
            let bx = other.x + other.side as i32 / 2;
            let by = other.y + other.side as i32 / 2;
            (ax - bx).abs() < hit.side as i32 / 2 && (ay - by).abs() < hit.side as i32 / 2
        });
        if !crowded {
            kept.push(hit);
        }
    }
    kept
}

pub fn mark_color(event_id: &str) -> [u8; 3] {
    const COLORS: [[u8; 3]; 6] = [
        [80, 220, 255],
        [255, 196, 60],
        [255, 90, 160],
        [120, 255, 140],
        [180, 140, 255],
        [255, 120, 60],
    ];
    let index = event_id
        .bytes()
        .fold(0usize, |acc, byte| acc.wrapping_add(byte as usize));
    COLORS[index % COLORS.len()]
}

pub fn square_bgra(icon_side: u32, color: [u8; 3], badge: Option<&RgbaImage>) -> (u32, Vec<u8>) {
    let pad = 4u32;
    let border = 3u32;
    let frame = icon_side.max(12) + pad * 2;
    let badge_side = if badge.is_some() { 22u32 } else { 0 };
    let hang = badge_side / 2;
    let side = frame + hang;
    let mut buf = vec![0u8; (side * side * 4) as usize];
    for y in 0..frame {
        for x in 0..frame {
            let edge = x < border || y < border || x + border >= frame || y + border >= frame;
            if !edge {
                continue;
            }
            let i = ((y * side + x) * 4) as usize;
            buf[i] = color[2];
            buf[i + 1] = color[1];
            buf[i + 2] = color[0];
            buf[i + 3] = 255;
        }
    }
    if let Some(badge) = badge {
        let ox = frame - hang;
        let oy = frame - hang;
        let cx = frame;
        let cy = frame;
        let radius = hang;
        let r2 = (radius as i32) * (radius as i32);
        for y in oy..oy + badge_side {
            for x in ox..ox + badge_side {
                if x >= side || y >= side {
                    continue;
                }
                let dx = x as i32 - cx as i32;
                let dy = y as i32 - cy as i32;
                if dx * dx + dy * dy > r2 {
                    continue;
                }
                let i = ((y * side + x) * 4) as usize;
                let a = 235u16;
                buf[i] = (12 * a / 255) as u8;
                buf[i + 1] = (10 * a / 255) as u8;
                buf[i + 2] = (8 * a / 255) as u8;
                buf[i + 3] = a as u8;
            }
        }
        let scaled = image::imageops::resize(badge, badge_side, badge_side, FilterType::Triangle);
        for (x, y, pixel) in scaled.enumerate_pixels() {
            let alpha = pixel[3];
            if alpha < 16 {
                continue;
            }
            let dx = ox + x;
            let dy = oy + y;
            if dx >= side || dy >= side {
                continue;
            }
            let i = ((dy * side + dx) * 4) as usize;
            let a = alpha as u16;
            buf[i] = (pixel[2] as u16 * a / 255) as u8;
            buf[i + 1] = (pixel[1] as u16 * a / 255) as u8;
            buf[i + 2] = (pixel[0] as u16 * a / 255) as u8;
            buf[i + 3] = alpha;
        }
    }
    (side, buf)
}

pub fn compose_canvas(
    marks: &[GemMark],
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
    badge: impl Fn(&str) -> Option<RgbaImage>,
) -> Vec<u8> {
    let mut bgra = vec![0u8; (width as usize).saturating_mul(height as usize).saturating_mul(4)];
    paint_squares(&mut bgra, width, height, origin_x, origin_y, marks, &badge);
    bgra
}

fn paint_squares(
    bgra: &mut [u8],
    width: u32,
    height: u32,
    origin_x: i32,
    origin_y: i32,
    marks: &[GemMark],
    badge: &impl Fn(&str) -> Option<RgbaImage>,
) {
    let pad = 4i32;
    for mark in marks {
        let icon = if mark.cost_icon.is_empty() {
            None
        } else {
            badge(&mark.cost_icon)
        };
        let (outer, square) = square_bgra(mark.side, mark_color(&mark.event_id), icon.as_ref());
        let x = mark.x - pad - origin_x;
        let y = mark.y - pad - origin_y;
        if x < 0 || y < 0 {
            continue;
        }
        blit(bgra, width, height, x as u32, y as u32, outer, &square);
    }
}

#[cfg_attr(windows, allow(dead_code))]
pub fn parse_ppm(data: &[u8]) -> Option<RgbImage> {
    if data.len() < 8 || &data[..2] != b"P6" {
        return None;
    }
    let mut index = 2;
    let mut nums = [0u32; 3];
    let mut count = 0;
    while count < 3 && index < data.len() {
        while index < data.len() && matches!(data[index], b' ' | b'\n' | b'\r' | b'\t') {
            index += 1;
        }
        if index < data.len() && data[index] == b'#' {
            while index < data.len() && data[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        let start = index;
        while index < data.len() && data[index].is_ascii_digit() {
            index += 1;
        }
        if start == index {
            return None;
        }
        nums[count] = std::str::from_utf8(&data[start..index]).ok()?.parse().ok()?;
        count += 1;
    }
    if index < data.len() && matches!(data[index], b' ' | b'\n' | b'\r' | b'\t') {
        index += 1;
    }
    let (width, height, max) = (nums[0], nums[1], nums[2]);
    if max != 255 || width == 0 || height == 0 || width > 8000 || height > 8000 {
        return None;
    }
    let bytes = (width as usize).checked_mul(height as usize)?.checked_mul(3)?;
    let pixels = data.get(index..index + bytes)?.to_vec();
    RgbImage::from_raw(width, height, pixels)
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Clone, Copy, Debug)]
struct Hit {
    x: u32,
    y: u32,
    side: u32,
    score: f32,
}

fn hold(
    previous: &mut Vec<GemMark>,
    fresh: Vec<GemMark>,
    image: Option<&RgbImage>,
    origin_x: i32,
    origin_y: i32,
    needles: &[Needle],
) -> Vec<GemMark> {
    let Some(image) = image else {
        previous.clear();
        return Vec::new();
    };
    const STICK: i32 = 12;
    let mut shown = Vec::new();
    for hit in fresh {
        if let Some(old) = previous.iter().find(|old| {
            old.event_id == hit.event_id
                && (old.x - hit.x).abs() <= STICK
                && (old.y - hit.y).abs() <= STICK
        }) {
            let mut kept = old.clone();
            kept.cost_icon = hit.cost_icon;
            shown.push(kept);
        } else {
            shown.push(hit);
        }
    }
    for old in previous.iter() {
        if shown.iter().any(|hit| hit.event_id == old.event_id) {
            continue;
        }
        let Some(needle) = needles.iter().find(|needle| needle.event_id == old.event_id) else {
            continue;
        };
        let x = old.x - origin_x;
        let y = old.y - origin_y;
        if x < 0 || y < 0 {
            continue;
        }
        if score_at(image, &needle.rgba, x as u32, y as u32, old.side) <= GEM_LIMIT {
            shown.push(old.clone());
        }
    }
    *previous = shown.clone();
    shown
}

fn push_matches(
    found: &mut Vec<GemMark>,
    image: &RgbImage,
    needles: &[Needle],
    region: Rect,
    vendor: bool,
    origin_x: i32,
    origin_y: i32,
) {
    for needle in needles.iter().filter(|needle| needle.vendor == vendor) {
        if let Some(hit) = best_match(image, &needle.rgba, region, &GEM_SCALES, GEM_LIMIT) {
            found.push(mark(needle, hit, origin_x, origin_y));
        }
    }
}

fn mark(needle: &Needle, hit: Hit, origin_x: i32, origin_y: i32) -> GemMark {
    GemMark {
        event_id: needle.event_id.clone(),
        x: origin_x + hit.x as i32,
        y: origin_y + hit.y as i32,
        side: hit.side,
        cost_icon: needle.cost_icon.clone(),
        score: hit.score,
    }
}

fn band(image: &RgbImage, window_h: u32, top_pct: u32, bottom_pct: u32) -> Option<Rect> {
    let y0 = window_h.saturating_mul(top_pct) / 100;
    let y1 = (window_h.saturating_mul(bottom_pct) / 100).min(image.height());
    let y0 = y0.min(image.height());
    if image.width() < 16 || y1 <= y0 {
        return None;
    }
    Some(Rect {
        x: 0,
        y: y0,
        w: image.width(),
        h: y1 - y0,
    })
}

struct Template {
    image: RgbaImage,
    scales: &'static [f32],
    limit: f32,
}

fn load_template(bytes: &[u8], scales: &'static [f32], limit: f32) -> Option<Template> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(Template {
        image,
        scales,
        limit,
    })
}

fn banners() -> &'static [Template] {
    static TEMPLATES: OnceLock<Vec<Template>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        load_template(
            include_bytes!("../../../assets/icons/ui/plaque-banner.png"),
            &BANNER_SCALES,
            BANNER_LIMIT,
        )
        .into_iter()
        .collect()
    })
}

fn purchase_crests() -> &'static [Template] {
    static PURCHASE: OnceLock<Vec<Template>> = OnceLock::new();
    PURCHASE.get_or_init(|| {
        load_template(
            include_bytes!("../../../assets/icons/ui/plaque-purchase.png"),
            &CREST_SCALES,
            CREST_LIMIT,
        )
        .into_iter()
        .collect()
    })
}

fn find_templates(image: &RgbImage, window_h: u32, templates: &[Template]) -> Option<Rect> {
    let region = band(image, window_h.max(image.height()), 0, 72)?;
    let mut best: Option<(Hit, f32)> = None;
    for template in templates {
        let Some(hit) = best_match(image, &template.image, region, template.scales, template.limit)
        else {
            continue;
        };
        if best.as_ref().is_none_or(|(current, _)| hit.score < current.score) {
            let aspect = template.image.height() as f32 / template.image.width().max(1) as f32;
            best = Some((hit, aspect));
        }
    }
    best.map(|(hit, aspect)| Rect {
        x: hit.x,
        y: hit.y,
        w: hit.side,
        h: (hit.side as f32 * aspect).round().max(1.0) as u32,
    })
}

fn below_plaque(image: &RgbImage, plaque: Rect) -> Rect {
    let x = plaque.x.saturating_sub(plaque.w / 2);
    let w = plaque.w.saturating_mul(3).min(image.width().saturating_sub(x));
    let y = (plaque.y + plaque.h).min(image.height());
    let h = image.height().saturating_sub(y);
    Rect { x, y, w, h }
}

fn best_match(
    image: &RgbImage,
    rgba: &RgbaImage,
    region: Rect,
    scales: &[f32],
    limit: f32,
) -> Option<Hit> {
    if region.w < 16 || region.h < 16 {
        return None;
    }
    let found = std::thread::scope(|scope| {
        let mut guards = Vec::with_capacity(scales.len());
        for &scale in scales {
            guards.push(scope.spawn(move || {
                score_scale(image, rgba, region, scale, limit)
            }));
        }
        guards
            .into_iter()
            .map(|guard| guard.join().ok().flatten())
            .collect::<Vec<_>>()
    });
    let mut best: Option<Hit> = None;
    for hit in found.into_iter().flatten() {
        if best.as_ref().is_none_or(|current| hit.score < current.score) {
            best = Some(hit);
        }
    }
    best.filter(|hit| hit.score <= limit)
}

fn score_scale(
    image: &RgbImage,
    rgba: &RgbaImage,
    region: Rect,
    scale: f32,
    limit: f32,
) -> Option<Hit> {
    let side = (rgba.width() as f32 * scale).round() as u32;
    let height = (rgba.height() as f32 * scale).round().max(1.0) as u32;
    if side < 14 || height < 10 || side > region.w || height > region.h {
        return None;
    }
    let scaled = image::imageops::resize(rgba, side, height, FilterType::Triangle);
    let samples = samples_of(&scaled);
    if samples.len() < 12 {
        return None;
    }
    let step = if height * 2 < side {
        2
    } else {
        (side.min(height) / 12).max(2)
    };
    let hit = scan(image, &samples, region, side, height, step, limit)?;
    Some(refine(
        image,
        rgba,
        region,
        hit,
        step as i32,
        step as i32,
        limit,
    ))
}

fn scan(
    image: &RgbImage,
    samples: &[(u32, u32, u8, u8, u8)],
    region: Rect,
    side: u32,
    height: u32,
    step: u32,
    limit: f32,
) -> Option<Hit> {
    if side < 8 || height < 6 || side > region.w || height > region.h {
        return None;
    }
    let mut coarse: Option<Hit> = None;
    let mut y = region.y;
    while y + height <= region.y + region.h {
        let mut x = region.x;
        while x + side <= region.x + region.w {
            let score = sad(image, x, y, samples, limit);
            if coarse.as_ref().is_none_or(|hit| score < hit.score) {
                coarse = Some(Hit {
                    x,
                    y,
                    side,
                    score,
                });
            }
            x += step;
        }
        y += step;
    }
    coarse
}

fn refine(
    image: &RgbImage,
    rgba: &RgbaImage,
    region: Rect,
    hit: Hit,
    radius_x: i32,
    radius_y: i32,
    limit: f32,
) -> Hit {
    let scaled = image::imageops::resize(rgba, hit.side, (rgba.height() as f32 * hit.side as f32 / rgba.width().max(1) as f32).round().max(1.0) as u32, FilterType::Triangle);
    let samples = samples_of(&scaled);
    let height = scaled.height();
    let mut best = hit;
    if samples.len() < 12 {
        return best;
    }
    for dy in -radius_y..=radius_y {
        for dx in -radius_x..=radius_x {
            let x = best_origin(hit.x as i32 + dx, region.x, region.w, hit.side);
            let y = best_origin(hit.y as i32 + dy, region.y, region.h, height);
            let Some((x, y)) = x.zip(y) else {
                continue;
            };
            let score = sad(image, x, y, &samples, limit);
            if score <= limit && score < best.score {
                best.x = x;
                best.y = y;
                best.score = score;
            }
        }
    }
    best
}

fn best_origin(value: i32, origin: u32, span: u32, size: u32) -> Option<u32> {
    if value < 0 {
        return None;
    }
    let value = value as u32;
    if value < origin || value + size > origin + span {
        return None;
    }
    Some(value)
}

fn samples_of(image: &RgbaImage) -> Vec<(u32, u32, u8, u8, u8)> {
    let mut all = Vec::new();
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] > 140 {
            all.push((x, y, pixel[0], pixel[1], pixel[2]));
        }
    }
    if all.len() <= 96 {
        return all;
    }
    let step = all.len() as f32 / 96.0;
    (0..96)
        .map(|index| all[(index as f32 * step) as usize])
        .collect()
}

fn sad(
    image: &RgbImage,
    x: u32,
    y: u32,
    samples: &[(u32, u32, u8, u8, u8)],
    limit: f32,
) -> f32 {
    let mut acc = 0u32;
    let count = samples.len() as u32;
    for (index, (sx, sy, r, g, b)) in samples.iter().enumerate() {
        let pixel = image.get_pixel(x + sx, y + sy);
        acc += (pixel[0] as i16 - *r as i16).unsigned_abs() as u32;
        acc += (pixel[1] as i16 - *g as i16).unsigned_abs() as u32;
        acc += (pixel[2] as i16 - *b as i16).unsigned_abs() as u32;
        if index == 23 {
            let lower = acc as f32 / (count as f32 * 3.0);
            if lower > limit {
                return 255.0;
            }
        }
    }
    acc as f32 / (count as f32 * 3.0)
}

fn score_at(image: &RgbImage, rgba: &RgbaImage, x: u32, y: u32, side: u32) -> f32 {
    let height = (rgba.height() as f32 * side as f32 / rgba.width().max(1) as f32)
        .round()
        .max(1.0) as u32;
    if x.saturating_add(side) > image.width() || y.saturating_add(height) > image.height() {
        return 255.0;
    }
    let scaled = image::imageops::resize(rgba, side, height, FilterType::Triangle);
    let samples = samples_of(&scaled);
    if samples.len() < 12 {
        return 255.0;
    }
    sad(image, x, y, &samples, GEM_LIMIT)
}

fn blit(dest: &mut [u8], dest_w: u32, dest_h: u32, x: u32, y: u32, side: u32, src: &[u8]) {
    for row in 0..side {
        let dy = y + row;
        if dy >= dest_h {
            continue;
        }
        for col in 0..side {
            let dx = x + col;
            if dx >= dest_w {
                continue;
            }
            let si = ((row * side + col) * 4) as usize;
            if src.get(si + 3).copied().unwrap_or(0) == 0 {
                continue;
            }
            let di = (((dy * dest_w) + dx) * 4) as usize;
            if di + 3 < dest.len() && si + 3 < src.len() {
                dest[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn paste(dst: &mut RgbImage, src: &RgbaImage, x: i32, y: i32) {
        for (sx, sy, pixel) in src.enumerate_pixels() {
            let alpha = pixel[3] as f32 / 255.0;
            if alpha < 0.04 {
                continue;
            }
            let dx = x + sx as i32;
            let dy = y + sy as i32;
            if dx < 0 || dy < 0 || dx >= dst.width() as i32 || dy >= dst.height() as i32 {
                continue;
            }
            let bg = dst.get_pixel(dx as u32, dy as u32);
            let mix = |color: u8, base: u8| (color as f32 * alpha + base as f32 * (1.0 - alpha)) as u8;
            dst.put_pixel(
                dx as u32,
                dy as u32,
                Rgb([
                    mix(pixel[0], bg[0]),
                    mix(pixel[1], bg[1]),
                    mix(pixel[2], bg[2]),
                ]),
            );
        }
    }

    fn icon(name: &str) -> RgbaImage {
        poe_data::load_icon(&format!("gem:{name}")).unwrap_or_else(|| panic!("missing {name}"))
    }

    #[test]
    fn a_pasted_gem_is_found_and_a_different_gem_is_not() {
        let fireball = icon("Fireball");
        let drawn = image::imageops::resize(&fireball, 62, 62, FilterType::Triangle);
        let mut scene = RgbImage::from_pixel(420, 300, Rgb([16, 14, 18]));
        paste(&mut scene, &drawn, 90, 80);
        let hits = locate(
            &scene,
            420,
            300,
            10,
            20,
            &[Needle {
                event_id: "fireball".into(),
                rgba: fireball,
                vendor: false,
                cost_icon: String::new(),
            }],
        );
        assert_eq!(hits.len(), 1, "score search missed the pasted icon");
        assert!((hits[0].x - 100).abs() <= 8, "x {}", hits[0].x);
        assert!((hits[0].y - 100).abs() <= 8, "y {}", hits[0].y);
        let wrong = locate(
            &scene,
            420,
            300,
            0,
            0,
            &[Needle {
                event_id: "spark".into(),
                rgba: icon("Spark"),
                vendor: false,
                cost_icon: String::new(),
            }],
        );
        assert!(wrong.is_empty());
    }

    #[test]
    fn vendor_gems_wait_for_the_purchase_plaque() {
        let gem = icon("Clarity");
        let drawn = image::imageops::resize(&gem, 62, 62, FilterType::Triangle);
        let plaque = purchase_crests()[0].image.clone();
        let mut scene = RgbImage::from_pixel(640, 420, Rgb([12, 10, 14]));
        paste(&mut scene, &drawn, 220, 180);
        let needle = Needle {
            event_id: "clarity".into(),
            rgba: gem,
            vendor: true,
            cost_icon: "alteration".into(),
        };
        assert!(locate(&scene, 640, 420, 0, 0, &[needle_clone(&needle)]).is_empty());
        paste(&mut scene, &plaque, 180, 40);
        let hits = locate(&scene, 640, 420, 5, 7, &[needle]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].cost_icon, "alteration");
        assert!((hits[0].x - 225).abs() <= 10, "x {}", hits[0].x);
        assert!((hits[0].y - 187).abs() <= 10, "y {}", hits[0].y);
    }

    fn needle_clone(needle: &Needle) -> Needle {
        Needle {
            event_id: needle.event_id.clone(),
            rgba: needle.rgba.clone(),
            vendor: needle.vendor,
            cost_icon: needle.cost_icon.clone(),
        }
    }

    #[test]
    fn the_vendor_banner_highlights_the_buy_and_leaves_the_quest_gem() {
        let banner = banners()[0].image.clone();
        let fireball = icon("Fireball");
        let tendrils = icon("Lightning Tendrils");
        let fire = image::imageops::resize(&fireball, 51, 51, FilterType::Triangle);
        let bolt = image::imageops::resize(&tendrils, 51, 51, FilterType::Triangle);
        let mut scene = RgbImage::from_pixel(980, 420, Rgb([12, 10, 14]));
        paste(&mut scene, &fire, 460, 140);
        paste(&mut scene, &bolt, 340, 140);
        let quest = Needle {
            event_id: "tendrils".into(),
            rgba: tendrils,
            vendor: false,
            cost_icon: String::new(),
        };
        let buy = Needle {
            event_id: "fireball".into(),
            rgba: fireball,
            vendor: true,
            cost_icon: "wisdom".into(),
        };
        let open_quest = locate(&scene, 980, 420, 0, 0, &[needle_clone(&quest), needle_clone(&buy)]);
        assert_eq!(open_quest.len(), 1);
        assert_eq!(open_quest[0].event_id, "tendrils");
        paste(&mut scene, &banner, 200, 30);
        let open_vendor = locate(&scene, 980, 420, 0, 0, &[quest, buy]);
        assert_eq!(open_vendor.len(), 1, "quest gem must stay off the vendor grid");
        assert_eq!(open_vendor[0].event_id, "fireball");
        assert_eq!(open_vendor[0].cost_icon, "wisdom");
        assert!((open_vendor[0].x - 460).abs() <= 8, "x {}", open_vendor[0].x);

        let spark = icon("Spark");
        let drawn = image::imageops::resize(&spark, 51, 51, FilterType::Triangle);
        let mut other = RgbImage::from_pixel(980, 420, Rgb([12, 10, 14]));
        paste(&mut other, &banner, 200, 30);
        paste(&mut other, &drawn, 460, 140);
        paste(&mut other, &bolt, 340, 140);
        let hits = locate(
            &other,
            980,
            420,
            0,
            0,
            &[
                Needle {
                    event_id: "tendrils".into(),
                    rgba: icon("Lightning Tendrils"),
                    vendor: false,
                    cost_icon: String::new(),
                },
                Needle {
                    event_id: "spark".into(),
                    rgba: spark,
                    vendor: true,
                    cost_icon: "transmutation".into(),
                },
            ],
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].event_id, "spark");
        assert_eq!(hits[0].cost_icon, "transmutation");
    }

    #[test]
    fn a_gem_stays_fixed_while_it_remains_and_blinks_off_when_it_leaves() {
        let fireball = icon("Fireball");
        let drawn = image::imageops::resize(&fireball, 51, 51, FilterType::Triangle);
        let mut scene = RgbImage::from_pixel(200, 160, Rgb([16, 14, 18]));
        paste(&mut scene, &drawn, 40, 30);
        let needle = Needle {
            event_id: "fireball".into(),
            rgba: fireball,
            vendor: true,
            cost_icon: "wisdom".into(),
        };
        let mut previous = vec![GemMark {
            event_id: "fireball".into(),
            x: 40,
            y: 30,
            side: 51,
            cost_icon: "wisdom".into(),
            score: 10.0,
        }];
        let jumped = GemMark {
            event_id: "fireball".into(),
            x: 52,
            y: 38,
            side: 48,
            cost_icon: "wisdom".into(),
            score: 12.0,
        };
        let shown = hold(
            &mut previous,
            vec![jumped],
            Some(&scene),
            0,
            0,
            &[needle_clone(&needle)],
        );
        assert_eq!((shown[0].x, shown[0].y), (40, 30));
        let shown = hold(&mut previous, Vec::new(), Some(&scene), 0, 0, &[needle_clone(&needle)]);
        assert_eq!(shown.len(), 1);
        let gone = RgbImage::from_pixel(200, 160, Rgb([16, 14, 18]));
        let shown = hold(
            &mut previous,
            Vec::new(),
            Some(&gone),
            0,
            0,
            &[needle_clone(&needle)],
        );
        assert!(shown.is_empty());
        let mut parked = vec![GemMark {
            event_id: "fireball".into(),
            x: 40,
            y: 30,
            side: 51,
            cost_icon: "wisdom".into(),
            score: 10.0,
        }];
        let scrolled = GemMark {
            event_id: "fireball".into(),
            x: 80,
            y: 30,
            side: 48,
            cost_icon: "wisdom".into(),
            score: 12.0,
        };
        let shown = hold(
            &mut parked,
            vec![scrolled],
            Some(&scene),
            0,
            0,
            &[needle_clone(&needle)],
        );
        assert_eq!((shown[0].x, shown[0].y), (80, 30));
        let shown = hold(&mut parked, Vec::new(), None, 0, 0, &[]);
        assert!(shown.is_empty());
    }

    #[test]
    fn a_vendor_square_carries_the_cost_icon() {
        let badge = poe_data::load_icon("wisdom").expect("wisdom");
        let icon_side = 51u32;
        let frame = icon_side + 8;
        let (side, buf) = square_bgra(icon_side, [80, 220, 255], Some(&badge));
        let badge_side = 22u32;
        let ox = frame - badge_side / 2;
        let oy = frame - badge_side / 2;
        assert!(ox + badge_side <= side && oy + badge_side <= side);
        let mut inside = 0u32;
        let mut outside = 0u32;
        for y in oy..oy + badge_side {
            for x in ox..ox + badge_side {
                let i = ((y * side + x) * 4) as usize;
                if buf[i + 3] < 16 {
                    continue;
                }
                if x < frame && y < frame {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
        let share = inside as f32 / (inside + outside) as f32;
        assert!(
            (share - 0.25).abs() < 0.06,
            "currency overlap {share:.2} inside {inside} outside {outside}"
        );
    }

    #[test]
    fn ppm_round_trip() {
        let mut bytes = b"P6\n2 1\n255\n".to_vec();
        bytes.extend_from_slice(&[10, 20, 30, 40, 50, 60]);
        let image = parse_ppm(&bytes).unwrap();
        assert_eq!(image.dimensions(), (2, 1));
        assert_eq!(image.get_pixel(1, 0).0, [40, 50, 60]);
    }
}
