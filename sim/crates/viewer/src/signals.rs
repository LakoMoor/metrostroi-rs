//! Trackside traffic lights: poles, heads and lenses laid out from the entity's own tables
//! (`gmod_track_signal/shared.lua`), placed at the map positions from `signs_<map>.lua`.

use crate::renderer::{source_angles, Renderer, Scene, SCALE};
use crate::track::Sign;
use anyhow::Result;
use formats::vfs::Vfs;
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::path::Path;

struct Head {
    step: f32,
    model: String,
    glass: Vec<(String, Vec3)>,
}

#[derive(Default)]
struct Kit {
    offset: Vec3,
    poles: Vec<String>,
    heads: HashMap<String, Head>,
    noleft: bool,
}

struct Part {
    obj: usize,
    /// Relative to the sign (Source axes, metres).
    local: Mat4,
}

struct Light {
    sign: usize,
    parts: Vec<Part>,
    /// (object, colour letter) per lens
    lenses: Vec<(usize, char)>,
    pos: Vec3,
}

static PENDING: std::sync::Mutex<Option<Signals>> = std::sync::Mutex::new(None);

/// The scene loader builds the lights; the app picks them up once the scene exists.
pub fn store(s: Signals) {
    *PENDING.lock().unwrap() = Some(s);
}

pub fn take() -> Option<Signals> {
    PENDING.lock().unwrap().take()
}

pub struct Signals {
    lights: Vec<Light>,
    signs: Vec<Sign>,
    /// Standing trains: (object, pose in map space).
    trains: Vec<(usize, Mat4)>,
    /// How many of the trailing `trains` are moving ones (poses come from the sim every frame).
    lead: usize,
    /// The map's static props: (object, pose in map space, origin).
    statics: Vec<(usize, Mat4, Vec3)>,
    /// Luminance of the ambient light at each static prop (same order as `statics`).
    ambient: Vec<f32>,
}

fn vec(t: &mlua::Table) -> Vec3 {
    Vec3::new(t.get(1).unwrap_or(0.0), t.get(2).unwrap_or(0.0), t.get(3).unwrap_or(0.0))
}

const STUBS: &str = r#"
function Vector(x, y, z) return { x or 0, y or 0, z or 0 } end
ENT = { TrafficLightModels = {}, RenderOffset = {} }
local dummy = setmetatable({}, { __index = function(t) return t end, __call = function(t) return t end })
setmetatable(_G, { __index = function() return dummy end })
"#;

struct Plate {
    model: String,
    pos: Vec3,
    yaw: f32,
    noauto: bool,
    noleft: bool,
    rotate: bool,
}

/// `ENT.SignModels` of `gmod_track_signs` (speed limit and other plates), indexed by SignType - 1.
fn load_plates(lua_root: &Path) -> Vec<Option<Plate>> {
    let lua = mlua::Lua::new();
    if lua.load(STUBS).exec().is_err() {
        return Vec::new();
    }
    let _ = lua.load("function Angle(p, y, r) return { p or 0, y or 0, r or 0 } end ENT.SignModels = {}").exec();
    let Ok(src) = std::fs::read_to_string(lua_root.join("entities/gmod_track_signs/shared.lua")) else { return Vec::new() };
    let _ = lua.load(&src).exec();
    let Ok(ent) = lua.globals().get::<mlua::Table>("ENT") else { return Vec::new() };
    let Ok(models) = ent.get::<mlua::Table>("SignModels") else { return Vec::new() };
    let n = models.pairs::<i64, mlua::Value>().flatten().map(|p| p.0).max().unwrap_or(-1);
    (0..=n)
        .map(|i| {
            let t = models.get::<mlua::Table>(i).ok()?;
            Some(Plate {
                model: t.get("model").ok()?,
                pos: t.get::<mlua::Table>("pos").map(|p| vec(&p)).unwrap_or(Vec3::ZERO),
                yaw: t.get::<mlua::Table>("angles").and_then(|a| a.get::<f32>(2)).unwrap_or(0.0),
                noauto: t.get("noauto").unwrap_or(false),
                noleft: t.get("noleft").unwrap_or(false),
                rotate: t.get("rotate").unwrap_or(false),
            })
        })
        .collect()
}

/// Read the model tables out of the entity's Lua without running the rest of it.
fn load_kits(lua_root: &Path) -> Result<(Vec<Kit>, Vec3)> {
    let lua = mlua::Lua::new();
    lua.load(STUBS).exec()?;
    let src = std::fs::read_to_string(lua_root.join("entities/gmod_track_signal/shared.lua"))?;
    // stops at the first thing the stubs cannot handle; the tables come before that
    let _ = lua.load(&src).exec();
    let ent: mlua::Table = lua.globals().get("ENT")?;
    let models: mlua::Table = ent.get("TrafficLightModels")?;
    let offsets: mlua::Table = ent.get("RenderOffset")?;
    let base = ent.get::<mlua::Table>("BasePosition").map(|t| vec(&t)).unwrap_or(Vec3::new(-110.0, 32.0, 0.0));
    let mut kits = Vec::new();
    for ty in 0..3 {
        let mut kit = Kit::default();
        if let Ok(t) = offsets.get::<mlua::Table>(ty) {
            kit.offset = vec(&t);
        }
        if let Ok(tlm) = models.get::<mlua::Table>(ty) {
            let mut poles: Vec<(String, String)> = Vec::new();
            for pair in tlm.pairs::<mlua::Value, mlua::Value>().flatten() {
                match pair {
                    (mlua::Value::String(k), mlua::Value::String(v)) => {
                        let k = k.to_string_lossy().to_string();
                        if k.starts_with('m') && !k.contains("long") && k.len() == 2 {
                            poles.push((k, v.to_string_lossy().to_string()));
                        }
                    }
                    (mlua::Value::String(k), mlua::Value::Boolean(b)) if &*k.to_string_lossy() == "noleft" => kit.noleft = b,
                    (k, mlua::Value::Table(h)) => {
                        let key = match k {
                            mlua::Value::Integer(i) => i.to_string(),
                            mlua::Value::Number(n) => (n as i64).to_string(),
                            mlua::Value::String(s) => s.to_string_lossy().to_string(),
                            _ => continue,
                        };
                        let (Ok(step), Ok(model)) = (h.get::<f32>(1), h.get::<String>(2)) else { continue };
                        let mut glass = Vec::new();
                        if let Ok(d) = h.get::<mlua::Table>(3) {
                            if let Ok(g) = d.get::<mlua::Table>("glass") {
                                for e in g.sequence_values::<mlua::Table>().flatten() {
                                    if let (Ok(m), Ok(p)) = (e.get::<String>(1), e.get::<mlua::Table>(2)) {
                                        glass.push((m, vec(&p)));
                                    }
                                }
                            }
                        }
                        kit.heads.insert(key, Head { step, model, glass });
                    }
                    _ => {}
                }
            }
            poles.sort();
            kit.poles = poles.into_iter().map(|p| p.1).collect();
        }
        kits.push(kit);
    }
    Ok((kits, base))
}

fn colour(c: char, lit: bool) -> [f32; 4] {
    let (r, g, b) = match c {
        'R' => (1.0, 0.1, 0.05),
        'Y' => (1.0, 0.75, 0.1),
        'G' => (0.1, 1.0, 0.25),
        'B' => (0.2, 0.35, 1.0),
        _ => (1.0, 1.0, 0.9),
    };
    let k = if lit { 1.6 } else { 0.12 };
    [r * k, g * k, b * k, 1.0]
}

/// Aspect shown when the line ahead is free: the colour of the lens that should be lit.
fn lit_colour(ars: Option<u8>) -> char {
    match ars {
        Some(7) | Some(8) => 'G',
        Some(4) | Some(5) | Some(6) => 'Y',
        _ => 'R',
    }
}

impl Signals {
    pub fn spawn(r: &Renderer, scene: &mut Scene, vfs: &Vfs, lua_root: &Path, signs: Vec<Sign>, train_poses: Vec<Mat4>, n_lead: usize) -> Signals {
        let (kits, base) = match load_kits(lua_root) {
            Ok(k) => k,
            Err(e) => {
                log::warn!("signal models: {e:#}");
                return Signals { lights: Vec::new(), signs, trains: Vec::new(), lead: 0, statics: Vec::new(), ambient: Vec::new() };
            }
        };
        let mut lights = Vec::new();
        let mut loaded: HashMap<String, Option<usize>> = HashMap::new();
        let plates = load_plates(lua_root);
        for (si, s) in signs.iter().enumerate() {
            if s.class == "gmod_track_signs" && s.on_main.is_some() {
                let Some(Some(pl)) = s.plate.0.checked_sub(1).and_then(|i| plates.get(i)) else { continue };
                let mut model = pl.model.clone();
                if s.left && !pl.noleft {
                    model = if model.contains("_r.mdl") { model.replace("_r.mdl", "_l.mdl") } else { model.replace("_l.mdl", "_r.mdl") };
                }
                let id = match loaded.entry(model.clone()).or_insert_with(|| r.load_model(scene, vfs, model.trim_end_matches(".mdl")).ok()) {
                    Some(id) => *id,
                    None => continue,
                };
                let mut pos = pl.pos + Vec3::new(0.0, s.plate.1, s.plate.2);
                let mut yaw = pl.yaw;
                if s.left {
                    pos.y = -pos.y;
                    if pl.rotate {
                        yaw -= 180.0;
                    }
                }
                let _ = (pl.noauto, pl.rotate);
                let obj = scene.objects.len();
                r.add_object(scene, "sign", id, Mat4::IDENTITY, false);
                let local = Mat4::from_translation(pos * SCALE) * source_angles(0.0, yaw, 0.0);
                lights.push(Light { sign: si, parts: vec![Part { obj, local }], lenses: Vec::new(), pos: s.pos });
                continue;
            }
            if s.class != "gmod_track_signal" || s.on_main.is_none() || s.lenses.is_empty() {
                continue;
            }
            let Some(kit) = kits.get(s.light_type) else { continue };
            let mirror = if s.left { Vec3::new(-1.0, 1.0, 1.0) } else { Vec3::ONE };
            let root = base * mirror;
            let mut parts = Vec::new();
            let mut lenses = Vec::new();
            let mut add = |scene: &mut Scene, m: &str, at: Vec3| -> Option<usize> {
                let slot = loaded.entry(m.to_string()).or_insert_with(|| r.load_model(scene, vfs, m.trim_end_matches(".mdl")).ok());
                let id = (*slot)?;
                let obj = scene.objects.len();
                r.add_object(scene, "signal", id, Mat4::IDENTITY, false);
                parts.push(Part { obj, local: Mat4::from_translation(at * SCALE) });
                Some(obj)
            };
            for p in &kit.poles {
                add(scene, p, root);
            }
            let mut offset = kit.offset;
            for (gi, group) in s.lenses.split('-').enumerate() {
                let head = kit.heads.get(group).or_else(|| group.len().checked_sub(1).and_then(|n| kit.heads.get(&n.to_string())));
                let Some(head) = head else { continue };
                if gi > 0 {
                    offset.z -= head.step;
                }
                let at = root + offset;
                let m = if s.left && !kit.noleft { head.model.replace(".mdl", "_mirror.mdl") } else { head.model.clone() };
                if add(scene, &m, at).is_none() {
                    add(scene, &head.model, at);
                }
                if group == "M" {
                    continue;
                }
                for (i, (gm, gp)) in head.glass.iter().enumerate() {
                    if let Some(o) = add(scene, gm, at + *gp * mirror) {
                        lenses.push((o, group.chars().nth(i).unwrap_or('W')));
                    }
                }
            }
            lights.push(Light { sign: si, parts, lenses, pos: s.pos });
        }
        log::info!("{} traffic lights placed ({} models)", lights.len(), loaded.len());
        let mut trains = Vec::new();
        if !train_poses.is_empty() {
            if let Ok(id) = r.load_model(scene, vfs, "models/metrostroi_train/81-717/81-717_mvm") {
                for pose in train_poses {
                    trains.push((scene.objects.len(), pose));
                    r.add_object(scene, "standing train", id, Mat4::IDENTITY, false);
                }
            }
        }
        let lead = n_lead.min(trains.len());
        Signals { lights, signs, trains, lead, statics: Vec::new(), ambient: Vec::new() }
    }

    /// Place the map's static props (station architecture, lamps, ...); only those near the train are drawn.
    pub fn with_statics(mut self, r: &Renderer, scene: &mut Scene, vfs: &Vfs, props: &[formats::bsp::StaticProp]) -> Signals {
        let mut ids: std::collections::HashMap<&str, Option<usize>> = std::collections::HashMap::new();
        for p in props {
            let id = *ids.entry(p.model.as_str()).or_insert_with(|| {
                let key = p.model.trim_end_matches(".mdl");
                match r.load_model(scene, vfs, key) {
                    Ok(id) => Some(id),
                    Err(e) => {
                        log::debug!("static prop {key}: {e}");
                        None
                    }
                }
            });
            let Some(id) = id else { continue };
            let origin = Vec3::from(p.origin) * SCALE;
            let pose = Mat4::from_translation(origin) * source_angles(p.angles[0], p.angles[1], p.angles[2]);
            self.statics.push((scene.objects.len(), pose, origin));
            self.ambient.push(p.ambient[0] * 0.3 + p.ambient[1] * 0.59 + p.ambient[2] * 0.11);
            r.add_object(scene, "static prop", id, pose, false);
            // ambient from the map's light cube, brightened like the lightmaps (x4 decode)
            let a = p.ambient.map(|c| (c * 7.0).clamp(0.35, 1.6));
            scene.objects.last_mut().unwrap().tint = [a[0], a[1], a[2], 5.0];
        }
        log::info!("{} of {} static props placed ({} models)", self.statics.len(), props.len(), ids.values().filter(|v| v.is_some()).count());
        self
    }

    /// Ambient light around `train` (map space): mean luminance of the static props within 25 m, if any.
    pub fn ambient_near(&self, train: Vec3) -> Option<f32> {
        let (mut sum, mut n) = (0.0, 0);
        for (i, &(_, _, o)) in self.statics.iter().enumerate() {
            if (o - train).length() < 25.0 {
                sum += self.ambient[i];
                n += 1;
            }
        }
        (n > 0).then(|| sum / n as f32)
    }

    /// Follow the world transform; show only lights near the train, lit by their free-line aspect.
    pub fn update(&self, scene: &mut Scene, world: Mat4, train: Vec3, show: bool, code: &dyn Fn(&Sign) -> Option<u8>, lead: &[Mat4]) {
        for (i, &(obj, mut pose)) in self.trains.iter().enumerate() {
            let first = self.trains.len() - self.lead;
            if i >= first {
                if let Some(&l) = lead.get(i - first) {
                    pose = l;
                }
            }
            let o = &mut scene.objects[obj];
            o.visible = show && (pose.w_axis.truncate() - train).length() < 600.0;
            o.transform = world * pose;
        }
        for &(obj, pose, origin) in &self.statics {
            let o = &mut scene.objects[obj];
            o.visible = show && (origin - train).length() < 300.0;
            if o.visible {
                o.transform = world * pose;
            }
        }
        for l in &self.lights {
            let s = &self.signs[l.sign];
            let near = show && (l.pos - train).length() < 450.0;
            let frame = world * Mat4::from_translation(s.pos) * source_angles(0.0, s.yaw, 0.0);
            for p in &l.parts {
                let o = &mut scene.objects[p.obj];
                o.visible = near;
                if near {
                    o.transform = frame * p.local;
                }
            }
            if near {
                let want = lit_colour(code(s));
                let mut done = false;
                for &(o, c) in &l.lenses {
                    let lit = !done && c == want;
                    done |= lit;
                    scene.objects[o].tint = colour(c, lit);
                }
            }
        }
    }
}
