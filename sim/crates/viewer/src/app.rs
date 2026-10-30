//! Interactive window: orbit camera outside (key 1), driver's eye in the cab (key 2).

use crate::menu::{Action, Menu, Screen};
use crate::panels::Panels;
use crate::sim::Sim;
use crate::ui::Ui;
use crate::renderer::{self, Renderer, Scene};
use anyhow::Result;
use glam::{Mat4, Vec3};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Driver's eye in car space, metres (Source axes: X forward, Y left, Z up).
/// Car origin above the track node, metres. Track nodes sit 2.21 m over the tunnel floor in the Metrostroi maps
/// (measured in the BSP) and the 81-717 wheel treads are 2.22 m below the car origin, so the origin is the node
/// itself. Override with `MS_CAR_HEIGHT` to compare.
pub fn car_above_rail() -> f32 {
    static H: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *H.get_or_init(|| std::env::var("MS_CAR_HEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0))
}
/// Standing eye height above the feet, metres.
const EYE_HEIGHT: f32 = 1.7;
/// Car floor in car space, and the interior / outer-wall half-widths and half-length (metres).
const FLOOR_Z: f32 = -0.85;
const INNER_Y: f32 = 1.25;
const OUTER_Y: f32 = 1.45;
const HALF_LEN: f32 = 9.2;
/// Passenger door centres along the car (metres) and their half-width.
const DOOR_X: [f32; 4] = [-6.6, -2.2, 2.2, 6.6];
const DOOR_HALF: f32 = 0.7;
/// Driver's eye in Source units of the car model.
const CAB_EYE: Vec3 = Vec3::new(403.5 * crate::renderer::SCALE, -4.72 * crate::renderer::SCALE, 11.02 * crate::renderer::SCALE);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Orbit,
    Cab,
    /// On foot outside the train.
    Walk,
}

struct Camera {
    mode: Mode,
    // orbit
    center: Vec3,
    radius: f32,
    yaw: f32,
    pitch: f32,
    dist: f32,
    // cab look direction (yaw 0 = looking along +X)
    cab_yaw: f32,
    cab_pitch: f32,
    /// head offset from the driver's seat, metres (arrows / PgUp / PgDn)
    head: Vec3,
    // on foot: feet position in car space, heading (0 = +X) and pitch; `world` maps map space to car space
    walk_pos: Vec3,
    walk_yaw: f32,
    walk_pitch: f32,
    world: Mat4,
}

impl Camera {
    /// Near / far planes and the vertical field of view the matrices above use (for turning depth back into metres).
    fn depth_params(&self, aspect: f32, fov: f32) -> crate::post::Params {
        let (near, far, fovy) = match self.mode {
            Mode::Orbit => ((self.radius * 0.005).max(0.02), self.radius * 20.0, 45.0f32),
            Mode::Cab => (0.03, 200.0, fov),
            Mode::Walk => (0.03, 400.0, fov),
        };
        crate::post::Params { near, far, tan_half: (fovy.to_radians() / 2.0).tan(), aspect, ao: 0.85, radius: 0.5, _pad: [0.0; 2] }
    }

    fn view_proj(&self, aspect: f32, fov: f32) -> Mat4 {
        match self.mode {
            Mode::Orbit => renderer::orbit_view_proj(self.center, self.radius, self.yaw, self.pitch, self.dist, aspect),
            Mode::Cab => {
                let dir = Vec3::new(
                    self.cab_yaw.cos() * self.cab_pitch.cos(),
                    self.cab_yaw.sin() * self.cab_pitch.cos(),
                    self.cab_pitch.sin(),
                );
                let view = Mat4::look_to_rh(CAB_EYE + self.head, dir, Vec3::Z);
                Mat4::perspective_rh(fov.to_radians(), aspect, 0.03, 200.0) * view
            }
            Mode::Walk => {
                let dir = Vec3::new(
                    self.walk_yaw.cos() * self.walk_pitch.cos(),
                    self.walk_yaw.sin() * self.walk_pitch.cos(),
                    self.walk_pitch.sin(),
                );
                let view = Mat4::look_to_rh(self.walk_pos + Vec3::Z * EYE_HEIGHT, dir, Vec3::Z);
                Mat4::perspective_rh(fov.to_radians(), aspect, 0.03, 400.0) * view
            }
        }
    }
}

/// Where the feet end up if they move to `p` (car space): the car floor inside, through an open door
/// and the map ground outside. `None` = blocked by a wall or an edge.
fn floor_at(p: Vec3, z: f32, doors: (bool, bool), ground: &Option<crate::ground::Ground>, world: Mat4) -> Option<f32> {
    if p.x.abs() < HALF_LEN && p.y.abs() < OUTER_Y {
        if p.y.abs() < INNER_Y {
            return Some(FLOOR_Z);
        }
        // in the wall: only through an open door on that side
        let open = if p.y > 0.0 { doors.0 } else { doors.1 };
        return (open && DOOR_X.iter().any(|d| (p.x - d).abs() < DOOR_HALF)).then_some(FLOOR_Z);
    }
    let g = ground.as_ref()?;
    // ground height in map space, brought back to car space; a step of half a metre at most
    let inv = world.inverse();
    let m = inv.transform_point3(Vec3::new(p.x, p.y, z));
    let h = g.height(m.x, m.y, m.z + 0.5)?;
    Some(world.transform_point3(Vec3::new(m.x, m.y, h)).z)
}

impl Gfx {
    /// Depth, multisample and post targets at the render resolution (the window size, scaled down in the HDR path).
    fn rebuild_targets(&mut self, scale: f32) {
        let hdr = self.renderer.hdr.get();
        let s = if hdr { scale } else { 1.0 };
        let (w, h) = (((self.config.width as f32 * s) as u32).max(1), ((self.config.height as f32 * s) as u32).max(1));
        self.depth = self.renderer.depth_view(w, h);
        self.msaa = self.renderer.msaa_view(w, h);
        self.post = hdr.then(|| crate::post::Post::new(&self.renderer.device, self.config.format, w, h));
        self.scale = scale;
    }
}

struct Gfx {
    /// Render scale the targets were built for.
    scale: f32,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    depth: wgpu::TextureView,
    /// Multisampled colour target (high/ultra quality).
    msaa: Option<wgpu::TextureView>,
    /// HDR scene + bloom chain (ultra tier only).
    post: Option<crate::post::Post>,
    ui: Ui,
}

struct App {
    vfs_scene: Option<Box<dyn FnOnce(&Renderer) -> Result<Scene>>>,
    gfx: Option<Gfx>,
    scene: Option<Scene>,
    signals: Option<crate::signals::Signals>,
    camera: Option<Camera>,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    start_cab: bool,
    panels: Panels,
    hovered: Option<String>,
    sim: Option<Sim>,
    names: std::collections::HashMap<String, usize>,
    last_tick: std::time::Instant,
    cursor: (f64, f64),
    press_at: (f64, f64),
    frames: u64,
    /// Brightness of the car body and cab from the light around it (smoothed).
    car_light: f32,
    /// Smoothed frames per second for the HUD.
    fps: f32,
    /// Last failure message and when it was shown (F7/F8/F9).
    fail_note: Option<(String, std::time::Instant)>,
    held: Option<String>,
    ground: Option<crate::ground::Ground>,
    /// W, S, A, D, Shift while walking
    move_keys: [bool; 5],
    /// Description of the cab button under the cursor.
    tip: Option<(String, Vec3)>,
    /// forward, back, left, right, up, down
    head_keys: [bool; 6],
    menu: Menu,
    make_sim: Box<dyn Fn() -> Result<Sim>>,
    audio: Option<crate::audio::Audio>,
}

impl App {
    fn init(&mut self, el: &ActiveEventLoop) -> Result<()> {
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title(concat!("Metrostroi-rs ", env!("CARGO_PKG_VERSION")))
                    .with_window_icon(window_icon())
                    .with_inner_size(LogicalSize::new(1600.0, 900.0)),
            )?,
        );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        log::info!("adapter: {}", adapter.get_info().name);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("surface not supported by adapter"))?;
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = f;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let renderer = Renderer::new(device, queue, config.format);
        renderer.samples.set(if self.menu.settings.quality >= 1 { 4 } else { 1 });
        let depth = renderer.depth_view(config.width, config.height);
        let msaa = renderer.msaa_view(config.width, config.height);
        let load = self.vfs_scene.take().expect("scene loader");
        let scene = load(&renderer)?;
        let (mn, mx) = scene.bounds();
        let radius = (mx - mn).length() * 0.5;
        self.camera = Some(Camera {
            mode: if self.start_cab { Mode::Cab } else { Mode::Orbit },
            center: (mn + mx) * 0.5,
            radius,
            yaw: -0.8,
            pitch: 0.35,
            dist: radius * 2.4,
            cab_yaw: 0.0,
            cab_pitch: -0.25,
            head: Vec3::ZERO,
            walk_pos: Vec3::ZERO,
            walk_yaw: 0.0,
            walk_pitch: 0.0,
            world: Mat4::IDENTITY,
        });
        let mut scene = scene;
        crate::car::reset_poses(&renderer, &mut scene);
        self.names = crate::car::index(&scene);
        self.signals = crate::signals::take();
        self.ground = crate::ground::take();
        self.scene = Some(scene);
        let ui = Ui::new(&renderer.device, &renderer.queue, config.format);
        self.gfx = Some(Gfx { window, surface, config, renderer, depth, msaa, post: None, scale: 1.0, ui });
        Ok(())
    }

    /// Click on a panel button: flip its latched state and report it in the title.
    fn cursor_hit(&self) -> Option<crate::panels::Hit> {
        let (g, cam) = (self.gfx.as_ref()?, self.camera.as_ref()?);
        if cam.mode == Mode::Orbit {
            return None;
        }
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let ndc = glam::Vec2::new(self.cursor.0 as f32 / w * 2.0 - 1.0, 1.0 - self.cursor.1 as f32 / h * 2.0);
        let inv = cam.view_proj(w / h, self.menu.settings.fov).inverse();
        let (near, far) = (inv.project_point3(ndc.extend(0.0)), inv.project_point3(ndc.extend(1.0)));
        let hidden = self.sim.as_ref().map(|s| s.hidden_buttons()).unwrap_or_default();
        // on foot you have to be within arm's length (plus a step) of the button; from the seat everything in the cab is reachable
        let reach = if cam.mode == Mode::Walk { 2.2 } else { f32::MAX };
        self.panels.pick(near, (far - near).normalize(), reach, &hidden)
    }

    /// Show the button under the cursor in the window title.
    fn hover(&mut self) {
        if self.menu.screen != Screen::Playing {
            self.menu.hover(self.cursor.0 as f32, self.cursor.1 as f32);
            return;
        }
        let hit = self.cursor_hit();
        let id = hit.as_ref().map(|h| h.id.clone());
        log::debug!("cursor {:?} -> {:?}", self.cursor, id);
        if id == self.hovered {
            return;
        }
        if let Some(g) = &self.gfx {
            g.window.set_title(&match &hit {
                Some(h) => format!("Metrostroi-rs | {} {}", h.id, h.tooltip),
                None => "Metrostroi-rs".to_string(),
            });
        }
        self.hovered = id;
    }

    fn click(&mut self) {
        let Some(hit) = self.cursor_hit() else { return };
        let Some(g) = &self.gfx else { return };
        let on = self.panels.toggle(&hit.id);
        let value = self.sim.as_ref().and_then(|s| s.press(&hit.id));
        let st = value.unwrap_or_else(|| if on { "ON".into() } else { "off".into() });
        log::info!("button {} -> {} {}", hit.id, st, hit.tooltip);
        g.window.set_title(&format!("Metrostroi-rs | {} = {} {}", hit.id, st, hit.tooltip));
    }

    fn handle_action(&mut self, el: &ActiveEventLoop, act: Action) {
        match act {
            Action::None | Action::Resume => {}
            Action::Quit => el.exit(),
            Action::PickLine(i) => {
                // a line is a different world: remember it and start a fresh process on it
                let Some(line) = crate::menu::lines().get(i).cloned() else { return };
                self.menu.settings.map = format!("content/maps/moscow/{}", line.id);
                self.menu.settings.save(&self.menu.settings_path());
                if let Ok(exe) = std::env::current_exe() {
                    let mut skip = false;
                    let args: Vec<String> = std::env::args()
                        .skip(1)
                        .filter(|a| {
                            if skip {
                                skip = false;
                                false
                            } else if a == "--map" {
                                skip = true;
                                false
                            } else {
                                a != "--nomenu"
                            }
                        })
                        .collect();
                    match std::process::Command::new(exe).args(args).spawn() {
                        Ok(_) => el.exit(),
                        Err(e) => log::error!("restart: {e}"),
                    }
                }
            }
            Action::Start { quick } => {
                if quick {
                    if let Some(s) = &self.sim {
                        s.quick_start();
                    }
                }
            }
            Action::QuickStart => {
                if let Some(s) = &self.sim {
                    s.quick_start();
                }
            }
            Action::SaveCab => {
                if let Some(s) = &self.sim {
                    let file = self.menu.settings_path().with_file_name("cab.json");
                    let state = s.save_cab(&self.panels.toggle_ids());
                    match std::fs::write(&file, serde_json::to_string_pretty(&state).unwrap_or_default()) {
                        Ok(()) => log::info!("cab saved to {}", file.display()),
                        Err(e) => log::warn!("cab save: {e}"),
                    }
                }
            }
            Action::LoadCab => {
                if let Some(s) = &self.sim {
                    let file = self.menu.settings_path().with_file_name("cab.json");
                    match std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()) {
                        Some(state) => s.load_cab(&state),
                        None => log::warn!("no saved cab at {}", file.display()),
                    }
                }
            }
            Action::ResetAutostop => {
                if let Some(s) = &self.sim {
                    s.reset_autostop();
                }
            }
            Action::ResetTrain | Action::ToMain => match (self.make_sim)() {
                Ok(s) => {
                    self.sim = Some(s);
                    if let Some(a) = &mut self.audio {
                        a.restart();
                    }
                    if let (Some(g), Some(scene)) = (&self.gfx, &mut self.scene) {
                        scene.last_pose.clear();
                        crate::car::reset_poses(&g.renderer, scene);
                    }
                    self.panels.state.clear();
                }
                Err(e) => log::error!("reset: {e:#}"),
            },
        }
    }

    fn redraw(&mut self) {
        let now = std::time::Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
        if dt > 0.0 {
            self.fps += (1.0 / dt - self.fps) * 0.05;
        }
        self.last_tick = now;
        let playing = self.menu.screen == Screen::Playing;
        if self.menu.screen == Screen::Trip {
            if let Some(s) = &self.sim {
                crate::menu::set_trip(s.trip_lines());
            }
        }
        // the tag belongs to the button under the cursor *now* (the camera may have moved since the mouse did)
        self.tip = if playing {
            self.cursor_hit().map(|h| (if h.tooltip.is_empty() { h.id.clone() } else { h.tooltip.clone() }, h.pos))
        } else {
            None
        };
        if let (Some(s), Some(g), Some(scene)) = (&self.sim, &self.gfx, &mut self.scene) {
            let t_tick = std::time::Instant::now();
            if playing {
                s.tick(dt);
            }
            prof(0, t_tick, self.fps);
            let t_audio = std::time::Instant::now();
            if let Some(a) = &mut self.audio {
                if playing {
                    let (loops, once) = s.sounds();
                    a.update(&loops, &once, self.menu.settings.volume);
                } else {
                    a.silence();
                }
            }
            prof(4, t_audio, self.fps);
            if let Some(cam) = self.camera.as_mut().filter(|c| playing && c.mode == Mode::Cab) {
                let k = |i: usize| self.head_keys[i] as i32 as f32;
                let fwd = Vec3::new(cam.cab_yaw.cos(), cam.cab_yaw.sin(), 0.0);
                let right = Vec3::new(cam.cab_yaw.sin(), -cam.cab_yaw.cos(), 0.0);
                let d = fwd * (k(0) - k(1)) + right * (k(3) - k(2)) + Vec3::Z * (k(4) - k(5));
                cam.head = (cam.head + d * 0.6 * dt).clamp(Vec3::new(-1.2, -1.2, -0.5), Vec3::new(1.2, 1.2, 0.5));
            }
            if let Some(cam) = self.camera.as_mut().filter(|c| playing && c.mode == Mode::Walk) {
                let k = |i: usize| self.move_keys[i] as i32 as f32;
                let fwd = Vec3::new(cam.walk_yaw.cos(), cam.walk_yaw.sin(), 0.0);
                let right = Vec3::new(cam.walk_yaw.sin(), -cam.walk_yaw.cos(), 0.0);
                let d = fwd * (k(0) - k(1)) + right * (k(3) - k(2));
                let mut np = cam.walk_pos;
                if d.length() > 0.0 {
                    np += d.normalize() * if self.move_keys[4] { 4.5 } else { 1.6 } * dt;
                }
                let doors = s.doors_open();
                let cur = cam.walk_pos;
                let inv = cam.world.inverse();
                // the full step first, then sliding along either axis
                for c in [np, Vec3::new(np.x, cur.y, cur.z), Vec3::new(cur.x, np.y, cur.z)] {
                    let Some(z) = floor_at(c, cur.z, doors, &self.ground, cam.world) else { continue };
                    let feet = Vec3::new(c.x, c.y, z);
                    if self.ground.as_ref().is_some_and(|g| g.blocked(inv.transform_point3(feet), 0.3)) {
                        continue;
                    }
                    cam.walk_pos = feet;
                    break;
                }
            }
            self.frames += 1;
            let t_state = std::time::Instant::now();
            crate::car::apply_state(&g.renderer, scene, &self.names, &s.state(), self.car_light);
            prof(1, t_state, self.fps);
            crate::car::spin_wheels(scene, &self.names, s.motion().1);
            // the train stays at the origin; the map moves around it
            if let (Some((pos, dir)), Some(w)) = (s.world_pose(), scene.objects.iter_mut().find(|o| o.name == "world")) {
                let x = dir.normalize_or_zero();
                let y = Vec3::Z.cross(x).normalize_or_zero();
                let z = x.cross(y);
                // the car's origin sits this far above the rails
                let origin = pos + Vec3::Z * car_above_rail();
                w.transform = Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), origin.extend(1.0)).inverse();
                // from outside the car the tunnel walls would hide it; the map is for the cab view
                w.visible = self.camera.as_ref().is_some_and(|c| c.mode != Mode::Orbit);
                if let Some(c) = self.camera.as_mut() {
                    c.world = w.transform;
                }

                let (xf, show) = (w.transform, w.visible);
                // light on the car: tunnel ambient measured at the train, eased so it fades in and out
                if let Some(sg) = &self.signals {
                    let target = sg.ambient_near(origin).map_or(1.0, |a| (a * 5.0).clamp(0.55, 1.15));
                    self.car_light += (target - self.car_light) * (dt * 2.0).min(1.0);
                }
                if let Some(sg) = &self.signals {
                    sg.update(scene, xf, pos, show, &|g| s.aspect(g), &s.other_poses());
                }
                if let Some(sky) = scene.objects.iter_mut().find(|o| o.name == "sky") {
                    // fixed in map space, centred on the train
                    sky.visible = show;
                    sky.transform = xf * Mat4::from_translation(origin);
                }
            }
        }
        let (Some(g), Some(scene), Some(cam)) = (&mut self.gfx, &self.scene, &self.camera) else { return };
        let t_acq = std::time::Instant::now();
        let frame = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                g.surface.configure(&g.renderer.device, &g.config);
                return;
            }
            _ => return,
        };
        prof(5, t_acq, self.fps);
        if let Some(s) = &self.sim {
            s.set_wet(self.menu.settings.wet);
        }
        let hdr = self.menu.settings.quality >= 2;
        // the HDR path tone-maps in the composite pass
        g.renderer.tonemap.set(0.0);
        let want = if self.menu.settings.quality >= 1 { 4 } else { 1 };
        let scale = self.menu.settings.scale.clamp(0.5, 1.0);
        if g.renderer.samples.get() != want || g.renderer.hdr.get() != hdr || (hdr && g.scale != scale) {
            g.renderer.samples.set(want);
            g.renderer.hdr.set(hdr);
            g.rebuild_targets(scale);
        }
        let t_render = std::time::Instant::now();
        let view = frame.texture.create_view(&Default::default());
        let aspect = g.config.width as f32 / g.config.height as f32;
        if let Some(post) = &g.post {
            g.renderer.render_ms(scene, cam.view_proj(aspect, self.menu.settings.fov), g.msaa.as_ref(), &post.scene, &g.depth);
            post.run(&g.renderer.device, &g.renderer.queue, &g.depth, cam.depth_params(aspect, self.menu.settings.fov), &view);
        } else {
            g.renderer.render_ms(scene, cam.view_proj(aspect, self.menu.settings.fov), g.msaa.as_ref(), &view, &g.depth);
        }
        g.ui.screen = (g.config.width as f32, g.config.height as f32);
        if self.menu.screen == Screen::Playing {
            let (sw, sh) = g.ui.screen;
            if cam.mode == Mode::Walk {
                g.ui.rect(sw / 2.0 - 2.0, sh / 2.0 - 2.0, 4.0, 4.0, [1.0, 1.0, 1.0, 0.8]);
            }
            if let (Some((tip, at)), true) = (&self.tip, cam.mode != Mode::Orbit) {
                // anchored to the button on its panel: project it through the current camera
                let c = cam.view_proj(aspect, self.menu.settings.fov).project_point3(*at);
                if c.z > 0.0 && c.z < 1.0 && c.x.abs() < 1.2 && c.y.abs() < 1.2 {
                    let (bx, by) = ((c.x + 1.0) / 2.0 * sw, (1.0 - c.y) / 2.0 * sh);
                    let w = g.ui.text_width(tip, 22.0);
                    let tx = (bx - w / 2.0).clamp(8.0, (sw - w - 8.0).max(8.0));
                    let ty = (by - 52.0).max(8.0);
                    g.ui.rect(tx - 8.0, ty - 4.0, w + 16.0, 32.0, [0.0, 0.0, 0.0, 0.85]);
                    g.ui.rect(bx - 1.0, ty + 28.0, 2.0, (by - ty - 28.0 - 6.0).max(0.0), [1.0, 1.0, 1.0, 0.7]);
                    g.ui.text(tx, ty, 22.0, tip, [1.0, 1.0, 1.0, 1.0]);
                }
            }
            if self.menu.settings.hud {
                if let Some(s) = &self.sim {
                    draw_hud(&mut g.ui, s);
                    let (sw, _) = g.ui.screen;
                    g.ui.text(sw - 90.0, 12.0, 18.0, &format!("{:.0} fps", self.fps), [0.6, 0.7, 0.85, 0.9]);
                    if let Some((msg, at)) = &self.fail_note {
                        if at.elapsed().as_secs_f32() < 6.0 {
                            let (sw, _) = g.ui.screen;
                            g.ui.text(sw * 0.5 - 220.0, 60.0, 24.0, msg, [1.0, 0.6, 0.4, 1.0]);
                        }
                    }
                }
            }
        } else {
            self.menu.draw(&mut g.ui);
        }
        g.ui.flush(&g.renderer.device, &g.renderer.queue, &view);
        g.window.pre_present_notify();
        g.renderer.queue.present(frame);
        prof(2, t_render, self.fps);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_none() {
            if let Err(e) = self.init(el) {
                log::error!("init failed: {e:#}");
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                if let Some(g) = &mut self.gfx {
                    g.config.width = s.width.max(1);
                    g.config.height = s.height.max(1);
                    g.surface.configure(&g.renderer.device, &g.config);
                    let scale = g.scale;
                    g.rebuild_targets(scale);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if self.menu.screen != Screen::Playing {
                    if state == ElementState::Pressed {
                        let act = self.menu.click(self.cursor.0 as f32, self.cursor.1 as f32);
                        self.handle_action(el, act);
                    }
                    return;
                }
                if state == ElementState::Pressed {
                    // pressing on a cab button holds it until the mouse is released
                    if let Some(hit) = self.cursor_hit() {
                        let on = self.panels.toggle(&hit.id);
                        let value = self.sim.as_ref().and_then(|s| s.button(&hit.id, true));
                        let st = value.unwrap_or_else(|| if on { "ON".into() } else { "off".into() });
                        log::info!("button {} -> {} {}", hit.id, st, hit.tooltip);
                        if let Some(g) = &self.gfx {
                            g.window.set_title(&format!("Metrostroi-rs | {} = {} {}", hit.id, st, hit.tooltip));
                        }
                        self.held = Some(hit.id);
                        return;
                    }
                } else if let Some(id) = self.held.take() {
                    if let Some(s) = &self.sim {
                        s.button(&id, false);
                    }
                    return;
                }
                self.dragging = state == ElementState::Pressed;
                if self.dragging {
                    self.press_at = self.cursor;
                } else {
                    self.last_cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let sens = self.menu.settings.mouse;
                if let (true, Some((lx, ly)), Some(cam)) = (self.dragging, self.last_cursor, &mut self.camera) {
                    let (dx, dy) = ((position.x - lx) as f32, (position.y - ly) as f32);
                    match cam.mode {
                        Mode::Orbit => {
                            cam.yaw -= dx * 0.006;
                            cam.pitch = (cam.pitch + dy * 0.006).clamp(-1.5, 1.5);
                        }
                        Mode::Walk => {
                            cam.walk_yaw -= dx * 0.004 * sens;
                            cam.walk_pitch = (cam.walk_pitch - dy * 0.004 * sens).clamp(-1.5, 1.5);
                        }
                        Mode::Cab => {
                            cam.cab_yaw -= dx * 0.004 * sens;
                            cam.cab_pitch = (cam.cab_pitch - dy * 0.004 * sens).clamp(-1.4, 1.4);
                        }
                    }
                }
                self.last_cursor = Some((position.x, position.y));
                self.cursor = (position.x, position.y);
                if !self.dragging {
                    self.hover();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if let Some(cam) = &mut self.camera {
                    if cam.mode == Mode::Orbit {
                        cam.dist = (cam.dist * (1.0 - y * 0.1)).clamp(cam.radius * 0.05, cam.radius * 12.0);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                if self.menu.screen != Screen::Playing {
                    if let (true, PhysicalKey::Code(code)) = (down, event.physical_key) {
                        let act = self.menu.key(code);
                        self.handle_action(el, act);
                    }
                    return;
                }
                if let (true, PhysicalKey::Code(KeyCode::Escape)) = (down, event.physical_key) {
                    // release anything held before pausing
                    if let (Some(id), Some(s)) = (self.held.take(), &self.sim) {
                        s.button(&id, false);
                    }
                    self.head_keys = [false; 6];
                    self.menu.escape();
                    return;
                }
                if let (PhysicalKey::Code(code), Some(cam)) = (event.physical_key, &mut self.camera) {
                    if cam.mode == Mode::Walk {
                        let k = match code {
                            KeyCode::KeyW => Some(0),
                            KeyCode::KeyS => Some(1),
                            KeyCode::KeyA => Some(2),
                            KeyCode::KeyD => Some(3),
                            KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(4),
                            _ => None,
                        };
                        if let Some(i) = k {
                            self.move_keys[i] = down;
                            return;
                        }
                    }
                    let slot = match code {
                        KeyCode::ArrowUp => Some(0),
                        KeyCode::ArrowDown => Some(1),
                        KeyCode::ArrowLeft => Some(2),
                        KeyCode::ArrowRight => Some(3),
                        KeyCode::PageUp => Some(4),
                        KeyCode::PageDown => Some(5),
                        _ => None,
                    };
                    if let Some(i) = slot {
                        self.head_keys[i] = down;
                        return;
                    }
                    if let (true, KeyCode::Home) = (down, code) {
                        cam.head = Vec3::ZERO;
                        cam.cab_yaw = 0.0;
                        cam.cab_pitch = -0.25;
                        return;
                    }
                    match code {
                        KeyCode::F2 if down => {
                            cam.mode = Mode::Orbit;
                            self.move_keys = [false; 5];
                        }
                        KeyCode::F3 if down => {
                            cam.mode = Mode::Cab;
                            self.move_keys = [false; 5];
                        }
                        KeyCode::F4 if down => {
                            if cam.mode == Mode::Walk {
                                // sit back down if the seat is within reach
                                let seat = CAB_EYE + cam.head;
                                if (cam.walk_pos.truncate() - seat.truncate()).length() < 1.2 {
                                    cam.cab_yaw = cam.walk_yaw;
                                    cam.cab_pitch = cam.walk_pitch;
                                    cam.mode = Mode::Cab;
                                    self.move_keys = [false; 5];
                                }
                            } else {
                                // stand up from the driver's seat
                                let seat = CAB_EYE + cam.head;
                                cam.walk_pos = Vec3::new(seat.x - 0.35, seat.y, FLOOR_Z);
                                cam.walk_yaw = cam.cab_yaw;
                                cam.walk_pitch = cam.cab_pitch;
                                cam.mode = Mode::Walk;
                            }
                        }
                        KeyCode::F7 if down => {
                            if let Some(s) = &self.sim {
                                self.fail_note = Some((s.trigger_failure().unwrap_or_else(|| "отказ не создан".into()), std::time::Instant::now()));
                            }
                        }
                        KeyCode::F8 if down => {
                            if let Some(s) = &self.sim {
                                s.reset_failures();
                                self.fail_note = Some(("отказы сброшены".into(), std::time::Instant::now()));
                            }
                        }
                        KeyCode::F9 if down => {
                            if let Some(s) = &self.sim {
                                let on = !s.auto_failures();
                                s.set_auto_failures(on);
                                self.fail_note = Some((if on { "отказы со временем: вкл" } else { "отказы со временем: выкл" }.into(), std::time::Instant::now()));
                            }
                        }
                        KeyCode::F6 if down => {
                            if let Some(s) = &self.sim {
                                s.reset_autostop();
                            }
                        }
                        KeyCode::F5 if down => {
                            if let Some(s) = &self.sim {
                                log::info!("quick start");
                                s.quick_start();
                            }
                        }
                        _ => {
                            if let (Some(name), Some(s), true) = (gmod_key_name(code), &self.sim, cam.mode != Mode::Walk) {
                                if !event.repeat {
                                    s.key(&name, down);
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let t_all = std::time::Instant::now();
                self.redraw();
                prof(3, t_all, self.fps);
                // `MS_EXIT_AFTER=<s>`: quit by itself (used to build asset packages unattended)
                static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
                let t0 = *T0.get_or_init(std::time::Instant::now);
                if std::env::var("MS_EXIT_AFTER").ok().and_then(|v| v.parse::<f32>().ok()).is_some_and(|s| t0.elapsed().as_secs_f32() > s) {
                    el.exit();
                }
                if let Some(g) = &self.gfx {
                    g.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// winit key -> the suffix of GMod's `KEY_*` constant.
static KEY_MAP: std::sync::OnceLock<std::collections::HashMap<String, String>> = std::sync::OnceLock::new();

/// Install the `keys` table of `game.json`.
pub fn set_key_map(m: std::collections::HashMap<String, String>) {
    let _ = KEY_MAP.set(m);
}

/// The Metrostroi key name for a physical key, after the user's remapping.
fn gmod_key_name(code: KeyCode) -> Option<String> {
    let name = physical_key_name(code)?;
    Some(KEY_MAP.get().and_then(|m| m.get(name)).cloned().unwrap_or_else(|| name.to_string()))
}

fn physical_key_name(code: KeyCode) -> Option<&'static str> {
    use KeyCode::*;
    Some(match code {
        Digit0 => "0", Digit1 => "1", Digit2 => "2", Digit3 => "3", Digit4 => "4",
        Digit5 => "5", Digit6 => "6", Digit7 => "7", Digit8 => "8", Digit9 => "9",
        KeyA => "A", KeyB => "B", KeyC => "C", KeyD => "D", KeyE => "E", KeyF => "F", KeyG => "G",
        KeyH => "H", KeyI => "I", KeyJ => "J", KeyK => "K", KeyL => "L", KeyM => "M", KeyN => "N",
        KeyO => "O", KeyP => "P", KeyQ => "Q", KeyR => "R", KeyS => "S", KeyT => "T", KeyU => "U",
        KeyV => "V", KeyW => "W", KeyX => "X", KeyY => "Y", KeyZ => "Z",
        Numpad0 => "PAD_0", Numpad1 => "PAD_1", Numpad2 => "PAD_2", Numpad3 => "PAD_3", Numpad4 => "PAD_4",
        Numpad5 => "PAD_5", Numpad6 => "PAD_6", Numpad7 => "PAD_7", Numpad8 => "PAD_8", Numpad9 => "PAD_9",
        NumpadDivide => "PAD_DIVIDE", NumpadMultiply => "PAD_MULTIPLY", NumpadSubtract => "PAD_MINUS",
        NumpadAdd => "PAD_PLUS", NumpadEnter => "PAD_ENTER", NumpadDecimal => "PAD_DECIMAL",
        BracketLeft => "LBRACKET", BracketRight => "RBRACKET", Semicolon => "SEMICOLON", Quote => "APOSTROPHE",
        Backquote => "BACKQUOTE", Comma => "COMMA", Period => "PERIOD", Slash => "SLASH", Backslash => "BACKSLASH",
        Minus => "MINUS", Equal => "EQUAL", Enter => "ENTER", Space => "SPACE", Backspace => "BACKSPACE", Tab => "TAB",
        CapsLock => "CAPSLOCK", Insert => "INSERT", Delete => "DELETE", Home => "HOME", End => "END",
        PageUp => "PAGEUP", PageDown => "PAGEDOWN",
        ShiftLeft => "LSHIFT", ShiftRight => "RSHIFT", AltLeft => "LALT", AltRight => "RALT",
        ControlLeft => "LCONTROL", ControlRight => "RCONTROL",
        ArrowUp => "UP", ArrowLeft => "LEFT", ArrowDown => "DOWN", ArrowRight => "RIGHT",
        _ => return None,
    })
}

pub fn run(
    load: Box<dyn FnOnce(&Renderer) -> Result<Scene>>,
    panels: Panels,
    sim: Option<Sim>,
    make_sim: Box<dyn Fn() -> Result<Sim>>,
    audio: Option<crate::audio::Audio>,
    start_cab: bool,
    settings_path: std::path::PathBuf,
    show_menu: bool,
) -> Result<()> {
    let el = EventLoop::new()?;
    let mut app = App {
        vfs_scene: Some(load),
        gfx: None,
        scene: None,
        signals: None,
        camera: None,
        dragging: false,
        last_cursor: None,
        start_cab,
        panels,
        hovered: None,
        sim,
        names: Default::default(),
        last_tick: std::time::Instant::now(),
        cursor: (0.0, 0.0),
        press_at: (0.0, 0.0),
        frames: 0,
        car_light: 1.0,
        fps: 60.0,
        fail_note: None,
        held: None,
        ground: None,
        move_keys: [false; 5],
        tip: None,
        head_keys: [false; 6],
        menu: Menu::new(settings_path, if show_menu { Screen::Main } else { Screen::Playing }),
        make_sim,
        audio,
    };
    el.run_app(&mut app)?;
    Ok(())
}

/// The game icon (embedded `icon.png`) for the title bar and taskbar.
fn window_icon() -> Option<winit::window::Icon> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(&include_bytes!("../icon.png")[..])).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    winit::window::Icon::from_rgba(buf, info.width, info.height).ok()
}

/// Speed, ARS limit and next signal in the corner while driving.
/// `MS_PROFILE=1`: average milliseconds per frame spent in the Lua tick, the state/scene update and rendering+present.
fn prof(slot: usize, since: std::time::Instant, fps: f32) {
    use std::sync::Mutex;
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static ACC: Mutex<([f64; 6], u32)> = Mutex::new(([0.0; 6], 0));
    if !*ON.get_or_init(|| std::env::var_os("MS_PROFILE").is_some()) {
        return;
    }
    let mut a = ACC.lock().unwrap();
    a.0[slot] += since.elapsed().as_secs_f64() * 1000.0;
    if slot == 3 {
        a.1 += 1;
        if a.1 == 120 {
            let n = a.1 as f64;
            log::info!("profile ms/frame: tick {:.2}  state {:.2}  render+present {:.2}  whole redraw {:.2}  audio {:.2}  acquire {:.2}  ({fps:.0} fps)", a.0[0] / n, a.0[1] / n, a.0[2] / n, a.0[3] / n, a.0[4] / n, a.0[5] / n);
            *a = ([0.0; 6], 0);
        }
    }
}

fn draw_hud(ui: &mut Ui, s: &Sim) {
    let (kmh, pos) = s.motion();
    let (sw, sh) = ui.screen;
    let (x, y) = (24.0, sh - 150.0);
    ui.rect(x - 12.0, y - 10.0, 330.0, 158.0, [0.0, 0.03, 0.08, 0.55]);
    ui.text(x, y, 56.0, &format!("{:.0}", kmh.abs()), [1.0, 1.0, 1.0, 1.0]);
    ui.text(x + 100.0, y + 28.0, 24.0, "км/ч", [0.7, 0.8, 1.0, 1.0]);
    let sig = match (s.next_signal(), s.ars_limit()) {
        (Some((d, name, _)), Some(l)) => format!("{name} через {d:.0} м · АРС {l:.0}"),
        (Some((d, name, 0)), _) => format!("{name} через {d:.0} м · КРАСНЫЙ"),
        (Some((d, name, _)), None) => format!("{name} через {d:.0} м"),
        _ => "сигналов впереди нет".to_string(),
    };
    ui.text(x, y + 74.0, 20.0, &sig, [0.9, 0.93, 1.0, 1.0]);
    ui.text(x, y + 98.0, 20.0, &format!("путь {pos:.0} м"), [0.6, 0.7, 0.85, 1.0]);
    if let Some((st, pl, d, side, left)) = s.platform_ahead() {
        let side = if side == crate::station::Side::Left { "слева" } else { "справа" };
        let st = s.station_name(st).unwrap_or_else(|| format!("Ст. {st}"));
        let line = if d > 0.0 {
            format!("{st} п.{pl} через {d:.0} м · двери {side}")
        } else {
            format!("{st} п.{pl} · до конца {left:.0} м · двери {side}")
        };
        ui.rect(x - 12.0, y - 40.0, 420.0, 28.0, [0.0, 0.03, 0.08, 0.55]);
        ui.text(x, y - 36.0, 20.0, &line, [0.75, 1.0, 0.8, 1.0]);
    }
    let (dl, dr) = s.doors_open();
    if dl || dr {
        let open = match (dl, dr) {
            (true, true) => "обе стороны",
            (true, false) => "слева",
            _ => "справа",
        };
        let wrong = match s.platform_ahead() {
            Some((_, _, d, side, _)) if d <= 0.0 => {
                let left = side == crate::station::Side::Left;
                (dl && !left) || (dr && left)
            }
            _ => true,
        };
        let msg = if kmh.abs() > 3.0 {
            Some("ДВЕРИ ОТКРЫТЫ В ДВИЖЕНИИ".to_string())
        } else if wrong {
            Some(format!("ДВЕРИ {open}: НЕ У ПЛАТФОРМЫ"))
        } else {
            None
        };
        ui.text(x, y + 122.0, 18.0, &format!("двери открыты: {open}"), [1.0, 0.9, 0.5, 1.0]);
        if let Some(m) = msg {
            let w = ui.text_width(&m, 30.0);
            ui.rect((sw - w) / 2.0 - 20.0, 110.0, w + 40.0, 52.0, [0.75, 0.35, 0.0, 0.85]);
            ui.text((sw - w) / 2.0, 118.0, 30.0, &m, [1.0, 1.0, 1.0, 1.0]);
        }
    }
    if let Some((short, _)) = s.stop_report() {
        let msg = if short > 8.0 {
            format!("Остановка: не дотянули {:.1} м до конца платформы", short)
        } else if short < -1.0 {
            format!("Остановка: проехали на {:.1} м", -short)
        } else {
            format!("Остановка: точно ({:.1} м до конца)", short)
        };
        let w = ui.text_width(&msg, 28.0);
        ui.rect((sw - w) / 2.0 - 20.0, sh - 110.0, w + 40.0, 48.0, [0.0, 0.25, 0.1, 0.85]);
        ui.text((sw - w) / 2.0, sh - 104.0, 28.0, &msg, [1.0, 1.0, 1.0, 1.0]);
    }
    if s.slipping() {
        let msg = "ПРОСКАЛЬЗЫВАНИЕ КОЛЁС";
        let w = ui.text_width(msg, 28.0);
        ui.rect((sw - w) / 2.0 - 20.0, 110.0, w + 40.0, 48.0, [0.6, 0.4, 0.0, 0.85]);
        ui.text((sw - w) / 2.0, 116.0, 28.0, msg, [1.0, 1.0, 1.0, 1.0]);
    }
    if s.ars_tripped() {
        let msg = "АВТОСТОП АРС — F6 сброс";
        let w = ui.text_width(msg, 34.0);
        ui.rect((sw - w) / 2.0 - 20.0, 40.0, w + 40.0, 60.0, [0.7, 0.05, 0.05, 0.85]);
        ui.text((sw - w) / 2.0, 48.0, 34.0, msg, [1.0, 1.0, 1.0, 1.0]);
    }
}
