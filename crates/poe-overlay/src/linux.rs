use crate::draw::{Frame, Painter};
use crate::hypr::{self, GameWindow, Monitor};
use anyhow::Context;
use poe_core::{visible_lines, AppState};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
use smithay_client_toolkit::reexports::client::protocol::{wl_output, wl_pointer, wl_region, wl_seat, wl_shm, wl_surface};
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
    };
    loop {
        dispatch_timeout(&conn, &mut queue, &mut app, 40)?;
        app.sync(&qh);
        if !running(&app.shared) && app.layer.is_none() {
            // Config UI can keep running with no game. Stay idle.
        }
    }
}

fn running(shared: &Mutex<AppState>) -> bool {
    shared.lock().map(|s| s.running.is_some() && s.hud_enabled && s.settings.experience_chosen).unwrap_or(false)
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
}

impl Overlay {
    fn sync(&mut self, qh: &QueueHandle<Self>) {
        let Ok(shared) = self.shared.lock() else {
            return;
        };
        let enabled = shared.running.is_some() && shared.hud_enabled && shared.settings.experience_chosen;
        let window = hypr::game_window().filter(|w| w.focused && w.w > 0 && w.h > 0);
        if !enabled || window.is_none() {
            drop(shared);
            self.unmap();
            return;
        }
        let window = window.unwrap();
        let monitors = hypr::monitors();
        let cursor = hypr::cursor();
        let near_button = cursor.is_some_and(|(x, y)| near_top_center(&window, x, y));
        let sig = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            window.x,
            window.y,
            window.w,
            window.h,
            near_button,
            shared.settings.opacity,
            shared.settings.corner as u8
        );
        let character = shared.active_character().cloned();
        let lines = character
            .as_ref()
            .map(|c| {
                visible_lines(
                    &c.route,
                    &c.progress,
                    shared.settings.acts,
                    shared.settings.gems,
                    shared.settings.tips,
                )
            })
            .unwrap_or_default();
        let theme = shared.settings.theme.clone();
        let opacity = shared.settings.opacity;
        let scale = shared.settings.scale;
        let corner = shared.settings.corner;
        let line_sig = lines.iter().map(|l| l.event_id.clone()).collect::<Vec<_>>().join("|");
        let capture = shared.ctrl_down && shared.alt_down;
        let full_sig = format!("{sig}|{capture}|{line_sig}");
        drop(shared);
        let output = pick_output(&self.output_state, &window, &monitors);
        let output_id = output.as_ref().and_then(|output| self.output_state.info(output).map(|info| info.id));
        if self.layer.is_some() && output_id.is_some() && self.bound_output != output_id {
            self.unmap();
        }
        self.bound_output = output_id;
        if self.layer.is_none() || self.last_sig != full_sig {
            self.ensure_layer(qh, &window, &monitors, output);
            self.last_sig = full_sig.clone();
            let frame = self.painter.paint(
                window.w,
                window.h,
                &lines,
                &theme,
                opacity,
                scale,
                corner,
                near_button,
            );
            self.present(qh, &frame, capture);
            self.hits = frame.hits;
        }
    }

    fn ensure_layer(
        &mut self,
        qh: &QueueHandle<Self>,
        window: &GameWindow,
        monitors: &[Monitor],
        output: Option<wl_output::WlOutput>,
    ) {
        if self.layer.is_some() {
            if let Some(layer) = &self.layer {
                position_layer(layer, window, monitors);
            }
            return;
        }
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Top,
            Some("poe-campaign"),
            output.as_ref(),
        );
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(-1);
        position_layer(&layer, window, monitors);
        layer.commit();
        self.layer = Some(layer);
    }

    fn unmap(&mut self) {
        self.layer = None;
        self.last_sig.clear();
        self.hits.clear();
    }

    fn present(&mut self, qh: &QueueHandle<Self>, frame: &Frame, capture_tasks: bool) {
        let Some(layer) = &self.layer else {
            return;
        };
        let stride = frame.width as i32 * 4;
        let Ok((buffer, canvas)) = self.pool.create_buffer(
            frame.width as i32,
            frame.height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) else {
            return;
        };
        let copy = frame.bgra.len().min(canvas.len());
        canvas[..copy].copy_from_slice(&frame.bgra[..copy]);
        let surface = layer.wl_surface().clone();
        set_input_region(&self.compositor, qh, &surface, frame, capture_tasks);
        surface.damage_buffer(0, 0, frame.width as i32, frame.height as i32);
        let _ = buffer.attach_to(&surface);
        layer.commit();
    }
}

fn set_input_region(
    compositor: &CompositorState,
    qh: &QueueHandle<Overlay>,
    surface: &wl_surface::WlSurface,
    frame: &Frame,
    capture_tasks: bool,
) {
    let region = compositor.wl_compositor().create_region(qh, RegionData);
    for hit in &frame.hits {
        if hit.config || capture_tasks {
            region.add(hit.x, hit.y, hit.w, hit.h);
        }
    }
    surface.set_input_region(Some(&region));
    region.destroy();
}

fn position_layer(layer: &LayerSurface, window: &GameWindow, monitors: &[Monitor]) {
    let monitor = monitors.iter().find(|m| m.id == window.monitor).or_else(|| {
        monitors.iter().find(|m| {
            window.x >= m.x && window.y >= m.y && window.x < m.x + m.w && window.y < m.y + m.h
        })
    });
    let (ox, oy) = monitor.map(|m| (m.x, m.y)).unwrap_or((0, 0));
    layer.set_size(window.w, window.h);
    layer.set_margin(window.y - oy, 0, 0, window.x - ox);
}

fn near_top_center(window: &GameWindow, x: i32, y: i32) -> bool {
    let cx = window.x + window.w as i32 / 2;
    let top = window.y;
    (x - cx).abs() < 90 && (y - top).abs() < 48 && y >= top - 8
}

fn pick_output(outputs: &OutputState, window: &GameWindow, monitors: &[Monitor]) -> Option<wl_output::WlOutput> {
    let monitor = monitors.iter().find(|monitor| {
        window.x >= monitor.x
            && window.y >= monitor.y
            && window.x < monitor.x + monitor.w
            && window.y < monitor.y + monitor.h
    });
    let all: Vec<_> = outputs.outputs().collect();
    if let Some(monitor) = monitor {
        for output in &all {
            let Some(info) = outputs.info(output) else { continue };
            if info.name.as_deref() == Some(monitor.name.as_str()) {
                return Some(output.clone());
            }
            let (x, y) = info.logical_position.unwrap_or(info.location);
            if x == monitor.x && y == monitor.y {
                return Some(output.clone());
            }
        }
    }
    all.into_iter().next()
}

impl CompositorHandler for Overlay {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
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
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        if configure.new_size.0 > 0 {
            self.width = configure.new_size.0;
        }
        if configure.new_size.1 > 0 {
            self.height = configure.new_size.1;
        }
    }
}

impl SeatHandler for Overlay {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
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
            let PointerEventKind::Press { button, .. } = event.kind else {
                continue;
            };
            if button != 0x110 {
                continue;
            }
            let x = event.position.0 as i32;
            let y = event.position.1 as i32;
            let hit = self.hits.iter().find(|hit| {
                x >= hit.x && y >= hit.y && x < hit.x + hit.w && y < hit.y + hit.h
            });
            let Some(hit) = hit else { continue };
            let Ok(mut shared) = self.shared.lock() else { continue };
            if hit.config {
                shared.show_config = true;
            } else if shared.ctrl_down && shared.alt_down {
                if let Some(character) = shared.active_character_mut() {
                    poe_core::force_complete(&mut character.progress, &hit.id);
                    shared.dirty_characters = true;
                }
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
