use crate::draw::Painter;
use crate::gemfind::{self, GemPass, GemWatch};
use crate::hypr::{self, GameWindow, Monitor};
use anyhow::Context;
use poe_core::{visible_lines, AppState};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{
    wl_output, wl_pointer, wl_region, wl_seat, wl_shm, wl_surface,
};
use smithay_client_toolkit::reexports::client::{Connection, Proxy, QueueHandle};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub fn run(state: Arc<Mutex<AppState>>) -> anyhow::Result<()> {
    let conn = Connection::connect_to_env().context("wayland connection")?;
    let (globals, mut queue) = registry_queue_init(&conn)?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).context("wl_compositor")?;
    let layer_shell = LayerShell::bind(&globals, &qh).context("layer shell")?;
    let shm = Shm::bind(&globals, &qh).context("wl_shm")?;
    let pool = SlotPool::new(256 * 256 * 4, &shm)?;
    let mut app = Overlay {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        pool,
        layer: None,
        pointer: None,
        width: 1,
        height: 1,
        painter: Painter::new(),
        shared: state,
        last_sig: String::new(),
        hits: Vec::new(),
        bound_output: None,
        button_layer: None,
        layer_ready: false,
        button_ready: false,
        pending: None,
        pending_button: None,
        cache: start_cache(),
        drag: None,
        panel_origin: (16, 16),
        panel_limit: (i32::MAX, i32::MAX),
        gem_layer: None,
        gem_ready: false,
        gem_geom: None,
        pending_gem: None,
        gem_watch: GemWatch::spawn(capture_grim),
        last_gem_submit: Instant::now() - Duration::from_secs(2),
    };
    loop {
        dispatch_timeout(&conn, &mut queue, &mut app, 40)?;
        app.sync(&qh);
    }
}

fn capture_grim(x: i32, y: i32, width: u32, height: u32) -> Option<image::RgbImage> {
    if width == 0 || height == 0 || hypr::screenshot_paused() {
        return None;
    }
    let output = std::process::Command::new("grim")
        .args([
            "-g",
            &format!("{x},{y} {width}x{height}"),
            "-t",
            "ppm",
            "-",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    gemfind::parse_ppm(&output.stdout)
}

fn dispatch_timeout(
    conn: &Connection,
    queue: &mut smithay_client_toolkit::reexports::client::EventQueue<Overlay>,
    state: &mut Overlay,
    timeout_ms: i32,
) -> anyhow::Result<()> {
    queue.dispatch_pending(state)?;
    if let Some(guard) = queue.prepare_read() {
        conn.flush()?;
        let fd = guard.connection_fd();
        let mut pfd = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if rc > 0 {
            guard.read()?;
        }
    }
    queue.dispatch_pending(state)?;
    Ok(())
}

struct RegionData;

impl smithay_client_toolkit::dispatch2::Dispatch2<wl_region::WlRegion, Overlay> for RegionData {
    fn event(
        &self,
        _: &mut Overlay,
        _: &wl_region::WlRegion,
        _: <wl_region::WlRegion as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Overlay>,
    ) {
    }
}

struct HudSnap {
    enabled: bool,
    opacity: f32,
    scale: f32,
    corner: poe_core::HudCorner,
    custom_x: Option<i32>,
    custom_y: Option<i32>,
    placing: bool,
    show_config_button: bool,
    theme: poe_core::Theme,
    capture: bool,
    lines: Vec<poe_core::VisibleLine>,
}

struct PanelDrag {
    cursor_x: i32,
    cursor_y: i32,
    origin_x: i32,
    origin_y: i32,
}

struct Overlay {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    layer: Option<LayerSurface>,
    pointer: Option<wl_pointer::WlPointer>,
    width: u32,
    height: u32,
    painter: Painter,
    shared: Arc<Mutex<AppState>>,
    last_sig: String,
    hits: Vec<crate::draw::Hit>,
    bound_output: Option<u32>,
    button_layer: Option<LayerSurface>,
    layer_ready: bool,
    button_ready: bool,
    pending: Option<PendingPaint>,
    pending_button: Option<PendingPaint>,
    cache: Arc<Mutex<HudCache>>,
    drag: Option<PanelDrag>,
    panel_origin: (i32, i32),
    panel_limit: (i32, i32),
    gem_layer: Option<LayerSurface>,
    gem_ready: bool,
    gem_geom: Option<(i32, i32, u32, u32)>,
    pending_gem: Option<PendingPaint>,
    gem_watch: GemWatch,
    last_gem_submit: Instant,
}

#[derive(Clone)]
struct PendingPaint {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
    hits: Vec<crate::draw::Hit>,
    interactive: bool,
}

struct HudCache {
    window: Option<GameWindow>,
    monitors: Vec<Monitor>,
    cursor: Option<(i32, i32)>,
}

fn start_cache() -> Arc<Mutex<HudCache>> {
    let cache = Arc::new(Mutex::new(HudCache {
        window: None,
        monitors: Vec::new(),
        cursor: None,
    }));
    let background = Arc::clone(&cache);
    std::thread::Builder::new()
        .name("poe-overlay-ipc".into())
        .spawn(move || loop {
            if hypr::screenshot_paused() {
                std::thread::sleep(Duration::from_millis(30));
                continue;
            }
            let window = hypr::game_window().filter(|window| window.w > 0 && window.h > 0);
            let monitors = if window.is_some() {
                hypr::monitors()
            } else {
                Vec::new()
            };
            let cursor = if window.is_some() {
                hypr::cursor()
            } else {
                None
            };
            if let Ok(mut slot) = background.lock() {
                slot.window = window;
                slot.monitors = monitors;
                slot.cursor = cursor;
            }
            std::thread::sleep(Duration::from_millis(200));
        })
        .ok();
    cache
}

impl Overlay {
    fn follow_drag(&mut self) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        let (origin_x, origin_y, cursor_x, cursor_y) =
            (drag.origin_x, drag.origin_y, drag.cursor_x, drag.cursor_y);
        let limit = self.panel_limit;
        let Some((cx, cy)) = hypr::cursor() else {
            return;
        };
        let nx = (origin_x + cx - cursor_x).clamp(0, limit.0);
        let ny = (origin_y + cy - cursor_y).clamp(0, limit.1);
        let Ok(mut shared) = self.shared.lock() else {
            return;
        };
        if !shared.hud_placing {
            drop(shared);
            self.drag = None;
            return;
        }
        shared.settings.custom_x = Some(nx);
        shared.settings.custom_y = Some(ny);
    }

    fn confirm_place(&mut self) {
        self.drag = None;
        let Ok(mut shared) = self.shared.lock() else {
            return;
        };
        shared.settings.custom_x = Some(self.panel_origin.0);
        shared.settings.custom_y = Some(self.panel_origin.1);
        shared.settings.corner = poe_core::HudCorner::Custom;
        shared.hud_placing = false;
        shared.dirty_settings = true;
    }

    fn press_panel(&mut self, x: i32, y: i32) {
        let placing = self
            .shared
            .lock()
            .map(|shared| shared.hud_placing)
            .unwrap_or(false);
        if placing {
            let confirm = self.hits.iter().any(|hit| {
                hit.id == "confirm"
                    && x >= hit.x
                    && y >= hit.y
                    && x < hit.x + hit.w
                    && y < hit.y + hit.h
            });
            if confirm {
                self.confirm_place();
                return;
            }
            let (ox, oy) = self
                .shared
                .lock()
                .map(|shared| {
                    (
                        shared.settings.custom_x.unwrap_or(self.panel_origin.0),
                        shared.settings.custom_y.unwrap_or(self.panel_origin.1),
                    )
                })
                .unwrap_or(self.panel_origin);
            if let Ok(mut shared) = self.shared.lock() {
                shared.settings.custom_x = Some(ox);
                shared.settings.custom_y = Some(oy);
            }
            let (cx, cy) = hypr::cursor().unwrap_or((0, 0));
            self.drag = Some(PanelDrag {
                cursor_x: cx,
                cursor_y: cy,
                origin_x: ox,
                origin_y: oy,
            });
            return;
        }
        let hit = self
            .hits
            .iter()
            .find(|hit| x >= hit.x && y >= hit.y && x < hit.x + hit.w && y < hit.y + hit.h);
        let Some(hit) = hit else { return };
        let Ok(mut shared) = self.shared.lock() else {
            return;
        };
        if shared.ctrl_down && shared.alt_down {
            if let Some(character) = shared.active_character_mut() {
                poe_core::force_complete(&mut character.progress, &hit.id);
                shared.dirty_characters = true;
            }
        }
    }

    fn sync(&mut self, qh: &QueueHandle<Self>) {
        self.follow_drag();
        if hypr::capture_suppressed() {
            self.unmap();
            return;
        }
        let snap = {
            let Ok(shared) = self.shared.lock() else {
                return;
            };
            HudSnap {
                enabled: shared.running.is_some()
                    && shared.hud_enabled
                    && shared.settings.experience_chosen,
                opacity: shared.settings.opacity,
                scale: shared.settings.scale,
                corner: shared.settings.corner,
                custom_x: shared.settings.custom_x,
                custom_y: shared.settings.custom_y,
                placing: shared.hud_placing,
                show_config_button: shared.settings.show_config_button,
                theme: shared.settings.theme.clone(),
                capture: shared.ctrl_down && shared.alt_down,
                lines: shared
                    .active_character()
                    .map(|character| {
                        let mut lines = visible_lines(
                            &character.route,
                            &character.progress,
                            shared.settings.acts,
                            shared.settings.gems,
                            shared.settings.tips,
                        );
                        if shared.settings.show_landmarks {
                            lines.extend(poe_core::visible_landmarks(
                                &shared.settings.landmarks,
                                &character.progress,
                            ));
                        }
                        lines
                    })
                    .unwrap_or_default(),
            }
        };
        if !snap.enabled {
            self.unmap();
            return;
        }
        let (window, monitors, cursor) = {
            let Ok(cache) = self.cache.lock() else {
                return;
            };
            (cache.window.clone(), cache.monitors.clone(), cache.cursor)
        };
        let Some(window) = window else {
            self.unmap();
            return;
        };
        let near_button = snap.show_config_button
            && !snap.placing
            && cursor.is_some_and(|(x, y)| near_top_center(&window, x, y));
        let custom = match (snap.custom_x, snap.custom_y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        };
        let sig = format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            window.x,
            window.y,
            window.w,
            window.h,
            near_button,
            snap.opacity,
            snap.corner as u8,
            snap.placing,
            custom.map(|(x, _)| x).unwrap_or(-1),
            custom.map(|(_, y)| y).unwrap_or(-1)
        );
        let line_sig = snap
            .lines
            .iter()
            .map(|l| format!("{}:{}:{}", l.event_id, l.text, l.detail))
            .collect::<Vec<_>>()
            .join("|");
        let full_sig = format!("{sig}|{}|{line_sig}", snap.capture);
        let Some(output) = pick_output(&self.output_state, &window, &monitors) else {
            self.unmap();
            return;
        };
        let Some((ox, oy)) = monitor_origin(&window, &monitors) else {
            self.unmap();
            return;
        };
        let (space_w, space_h, base_x, base_y) = placement_space(&window, &monitors, ox, oy);
        let output_id = self.output_state.info(&output).map(|info| info.id);
        if self.layer.is_some() && output_id.is_some() && self.bound_output != output_id {
            self.unmap();
        }
        self.bound_output = output_id;
        if self.layer.is_none() || self.last_sig != full_sig {
            self.last_sig = full_sig.clone();
            let frame = self.painter.paint(
                space_w,
                space_h,
                &snap.lines,
                &snap.theme,
                snap.opacity,
                snap.scale,
                snap.corner,
                near_button,
                custom,
                snap.placing,
            );
            let [px, py, pw, ph] = frame.panel;
            self.panel_origin = (px, py);
            self.panel_limit = ((space_w as i32 - pw).max(0), (space_h as i32 - ph).max(0));
            self.ensure_layer(
                qh,
                &output,
                base_x - ox + px,
                base_y - oy + py,
                pw.max(1) as u32,
                ph.max(1) as u32,
            );
            self.pending = Some(PendingPaint {
                width: frame.width,
                height: frame.height,
                bgra: frame.bgra,
                hits: frame.hits.clone(),
                interactive: snap.capture || snap.placing,
            });
            self.hits = frame.hits;
            self.flush_layer(qh);
            if let Some(button) = frame.button {
                self.ensure_button(
                    qh,
                    &output,
                    base_x - ox + button.x,
                    base_y - oy + button.y,
                    button.w,
                    button.h,
                );
                self.pending_button = Some(PendingPaint {
                    width: button.w,
                    height: button.h,
                    bgra: button.bgra,
                    hits: Vec::new(),
                    interactive: false,
                });
                self.flush_button(qh);
            } else {
                self.button_layer = None;
                self.button_ready = false;
                self.pending_button = None;
            }
        }
        self.pump_gems(qh, &output, ox, oy, &window, &snap.lines);
    }

    fn ensure_layer(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &wl_output::WlOutput,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) {
        if self.layer.is_none() {
            let surface = self.compositor.create_surface(qh);
            let layer = self.layer_shell.create_layer_surface(
                qh,
                surface,
                Layer::Overlay,
                Some("RustyExile"),
                Some(output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.set_exclusive_zone(0);
            self.layer = Some(layer);
            self.layer_ready = false;
        }
        if let Some(layer) = &self.layer {
            place_layer(layer, x, y, width, height);
        }
    }

    fn ensure_button(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &wl_output::WlOutput,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) {
        if self.button_layer.is_none() {
            let surface = self.compositor.create_surface(qh);
            let layer = self.layer_shell.create_layer_surface(
                qh,
                surface,
                Layer::Overlay,
                Some("RustyExile-config"),
                Some(output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.set_exclusive_zone(0);
            self.button_layer = Some(layer);
            self.button_ready = false;
        }
        if let Some(layer) = &self.button_layer {
            place_layer(layer, x, y, width, height);
        }
    }

    fn unmap(&mut self) {
        self.layer = None;
        self.button_layer = None;
        self.gem_layer = None;
        self.layer_ready = false;
        self.button_ready = false;
        self.gem_ready = false;
        self.gem_geom = None;
        self.pending = None;
        self.pending_button = None;
        self.pending_gem = None;
        self.last_sig.clear();
        self.hits.clear();
        self.clear_gem_spots();
    }

    fn flush_layer(&mut self, qh: &QueueHandle<Self>) {
        self.flush_paint(
            qh,
            self.layer.clone(),
            self.layer_ready,
            self.pending.clone(),
            true,
        );
    }

    fn flush_button(&mut self, qh: &QueueHandle<Self>) {
        self.flush_paint(
            qh,
            self.button_layer.clone(),
            self.button_ready,
            self.pending_button.clone(),
            false,
        );
    }

    fn flush_paint(
        &mut self,
        qh: &QueueHandle<Self>,
        layer: Option<LayerSurface>,
        ready: bool,
        pending: Option<PendingPaint>,
        input: bool,
    ) {
        let Some(layer) = layer else {
            return;
        };
        if !ready {
            layer.commit();
            return;
        }
        let Some(paint) = pending else {
            return;
        };
        self.attach(&layer, qh, &paint, input);
    }

    fn pump_gems(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &wl_output::WlOutput,
        ox: i32,
        oy: i32,
        window: &GameWindow,
        lines: &[poe_core::VisibleLine],
    ) {
        let town = self.shared.lock().ok().is_some_and(|shared| shared.in_town());
        match self.gem_watch.advance(
            &mut self.last_gem_submit,
            town,
            window.x,
            window.y,
            window.w,
            window.h,
            lines,
        ) {
            GemPass::Idle => self.clear_gems(),
            GemPass::Pending => {}
            GemPass::Ready(marks) => {
                if let Ok(mut shared) = self.shared.lock() {
                    shared.gem_spots = gemfind::spots(&marks);
                }
                self.show_gems(qh, output, ox, oy, window, &marks);
            }
        }
    }

    fn clear_gems(&mut self) {
        if self.gem_layer.is_none() && self.pending_gem.is_none() {
            self.clear_gem_spots();
            return;
        }
        self.gem_layer = None;
        self.gem_ready = false;
        self.gem_geom = None;
        self.pending_gem = None;
        self.clear_gem_spots();
    }

    fn clear_gem_spots(&mut self) {
        if let Ok(mut shared) = self.shared.lock() {
            if !shared.gem_spots.is_empty() {
                shared.gem_spots.clear();
            }
        }
    }

    fn show_gems(
        &mut self,
        qh: &QueueHandle<Self>,
        output: &wl_output::WlOutput,
        ox: i32,
        oy: i32,
        window: &GameWindow,
        marks: &[gemfind::GemMark],
    ) {
        let (width, height) = gemfind::search_size(window.w, window.h);
        let geom = (window.x - ox, window.y - oy, width, height);
        if marks.is_empty() && self.gem_layer.is_none() {
            return;
        }
        if self.gem_layer.is_none() {
            let surface = self.compositor.create_surface(qh);
            let layer = self.layer_shell.create_layer_surface(
                qh,
                surface,
                Layer::Overlay,
                Some("RustyExile-gems"),
                Some(output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.set_exclusive_zone(0);
            self.gem_layer = Some(layer);
            self.gem_ready = false;
            self.gem_geom = None;
        }
        if self.gem_geom != Some(geom) {
            if let Some(layer) = &self.gem_layer {
                place_layer(layer, geom.0, geom.1, geom.2, geom.3);
            }
            self.gem_geom = Some(geom);
        }
        let bgra = gemfind::compose_canvas(marks, window.x, window.y, width, height, poe_data::load_icon);
        self.pending_gem = Some(PendingPaint {
            width,
            height,
            bgra,
            hits: Vec::new(),
            interactive: false,
        });
        self.flush_gem(qh);
    }

    fn flush_gem(&mut self, qh: &QueueHandle<Self>) {
        self.flush_paint(
            qh,
            self.gem_layer.clone(),
            self.gem_ready,
            self.pending_gem.clone(),
            true,
        );
    }

    fn attach(
        &mut self,
        layer: &LayerSurface,
        qh: &QueueHandle<Self>,
        paint: &PendingPaint,
        input: bool,
    ) {
        let stride = paint.width as i32 * 4;
        let Ok((buffer, canvas)) = self.pool.create_buffer(
            paint.width as i32,
            paint.height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) else {
            return;
        };
        let copy = paint.bgra.len().min(canvas.len());
        canvas[..copy].copy_from_slice(&paint.bgra[..copy]);
        let surface = layer.wl_surface().clone();
        if input {
            set_input_region(
                &self.compositor,
                qh,
                &surface,
                &paint.hits,
                paint.interactive,
            );
        }
        surface.damage_buffer(0, 0, paint.width as i32, paint.height as i32);
        let _ = buffer.attach_to(&surface);
        layer.commit();
    }
}

fn set_input_region(
    compositor: &CompositorState,
    qh: &QueueHandle<Overlay>,
    surface: &wl_surface::WlSurface,
    hits: &[crate::draw::Hit],
    interactive: bool,
) {
    let region = compositor.wl_compositor().create_region(qh, RegionData);
    if interactive {
        for hit in hits {
            region.add(hit.x, hit.y, hit.w, hit.h);
        }
    }
    surface.set_input_region(Some(&region));
    region.destroy();
}

fn place_layer(layer: &LayerSurface, x: i32, y: i32, width: u32, height: u32) {
    layer.set_size(width, height);
    layer.set_margin(y, 0, 0, x);
}

fn window_monitor<'a>(window: &GameWindow, monitors: &'a [Monitor]) -> Option<&'a Monitor> {
    monitors
        .iter()
        .find(|monitor| monitor.id == window.monitor)
        .or_else(|| monitors.iter().find(|monitor| contains_window(window, monitor)))
}

fn monitor_origin(window: &GameWindow, monitors: &[Monitor]) -> Option<(i32, i32)> {
    window_monitor(window, monitors).map(|monitor| (monitor.x, monitor.y))
}

fn placement_space(
    window: &GameWindow,
    monitors: &[Monitor],
    origin_x: i32,
    origin_y: i32,
) -> (u32, u32, i32, i32) {
    let Some(monitor) = window_monitor(window, monitors) else {
        return (window.w, window.h, window.x, window.y);
    };
    let near = |a: i32, b: i32| (a - b).abs() <= 80;
    if near(window.w as i32, monitor.w)
        && near(window.h as i32, monitor.h)
        && near(window.x, origin_x)
        && near(window.y, origin_y)
    {
        (monitor.w as u32, monitor.h as u32, origin_x, origin_y)
    } else {
        (window.w, window.h, window.x, window.y)
    }
}

fn contains_window(window: &GameWindow, monitor: &Monitor) -> bool {
    window.x >= monitor.x
        && window.y >= monitor.y
        && window.x < monitor.x + monitor.w
        && window.y < monitor.y + monitor.h
}

fn near_top_center(window: &GameWindow, x: i32, y: i32) -> bool {
    let cx = window.x + window.w as i32 / 2;
    let top = window.y;
    (x - cx).abs() < 90 && (y - top).abs() < 48 && y >= top - 8
}

fn pick_output(
    outputs: &OutputState,
    window: &GameWindow,
    monitors: &[Monitor],
) -> Option<wl_output::WlOutput> {
    let monitor = window_monitor(window, monitors)?;
    for output in outputs.outputs() {
        let Some(info) = outputs.info(&output) else {
            continue;
        };
        if info.name.as_deref() == Some(monitor.name.as_str()) {
            return Some(output);
        }
        let (x, y) = info.logical_position.unwrap_or(info.location);
        if x == monitor.x && y == monitor.y {
            return Some(output);
        }
    }
    None
}

impl CompositorHandler for Overlay {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Overlay {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Overlay {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.layer = None;
    }
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if self.layer.as_ref() == Some(layer) {
            if configure.new_size.0 > 0 {
                self.width = configure.new_size.0;
            }
            if configure.new_size.1 > 0 {
                self.height = configure.new_size.1;
            }
            self.layer_ready = true;
            self.flush_layer(qh);
        } else if self.button_layer.as_ref() == Some(layer) {
            self.button_ready = true;
            self.flush_button(qh);
        } else if self.gem_layer.as_ref() == Some(layer) {
            self.gem_ready = true;
            self.flush_gem(qh);
        }
    }
}

impl SeatHandler for Overlay {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Overlay {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let x = event.position.0 as i32;
            let y = event.position.1 as i32;
            let on_button = self
                .button_layer
                .as_ref()
                .is_some_and(|layer| layer.wl_surface().id() == event.surface.id());
            let on_layer = self
                .layer
                .as_ref()
                .is_some_and(|layer| layer.wl_surface().id() == event.surface.id());
            match event.kind {
                PointerEventKind::Press { button, .. } if button == 0x110 && on_button => {
                    crate::hypr::raise_config_window();
                    if let Ok(mut shared) = self.shared.lock() {
                        shared.show_config = true;
                    }
                }
                PointerEventKind::Press { button, .. } if button == 0x110 && on_layer => {
                    self.press_panel(x, y);
                }
                PointerEventKind::Release { button, .. } if button == 0x110 => {
                    self.drag = None;
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Overlay {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

smithay_client_toolkit::delegate_dispatch2!(Overlay);
smithay_client_toolkit::delegate_registry!(Overlay);

impl ProvidesRegistryState for Overlay {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
