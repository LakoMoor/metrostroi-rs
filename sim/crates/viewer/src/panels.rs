//! Cab button panels (`ENT.ButtonMap`): ray picking and per-button state.

use crate::renderer::{source_angles, SCALE};
use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

enum Shape {
    Rect { w: f32, h: f32 },
    Circle { r: f32 },
}

struct Button {
    id: String,
    tooltip: String,
    x: f32,
    y: f32,
    shape: Shape,
}

struct Panel {
    origin: Vec3,
    axes: Mat4, // columns: panel x, panel y, normal
    scale: f32, // metres per panel unit
    name: String,
    size: (f32, f32),
    buttons: Vec<Button>,
}

pub struct Hit {
    pub id: String,
    pub tooltip: String,
    /// Centre of the button on its panel (car space), so a tag can stay attached to it.
    pub pos: Vec3,
}

#[derive(Default)]
pub struct Panels {
    panels: Vec<Panel>,
    /// Latched state of each button ID (flipped on click).
    pub state: HashMap<String, bool>,
    /// Russian descriptions by button ID (`tooltips_ru.json` next to the controls file).
    tips: HashMap<String, String>,
}

fn vec3(v: &Value, key: &str) -> Option<Vec3> {
    let a = v.get(key)?.as_object()?.values().next()?.as_array()?;
    Some(Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32))
}

fn num(v: &Value, key: &str) -> f32 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

impl Panels {
    pub fn load(controls_json: &Path) -> Result<Self> {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(controls_json).with_context(|| controls_json.display().to_string())?)?;
        let mut out = Panels::default();
        for (name, p) in doc["button_panels"].as_object().context("button_panels missing")? {
            let (Some(pos), Some(ang)) = (vec3(p, "pos"), vec3(p, "ang")) else { continue };
            let buttons = p["buttons"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|b| {
                    let id = b["ID"].as_str()?.to_string();
                    // same rule as the cab's own hit test: a rectangle if it has w and h, else a circle (default radius 10)
                    let shape = if b.get("w").is_some() && b.get("h").is_some() {
                        Shape::Rect { w: num(b, "w"), h: num(b, "h") }
                    } else {
                        Shape::Circle { r: b.get("radius").and_then(Value::as_f64).unwrap_or(10.0) as f32 }
                    };
                    Some(Button { id, tooltip: b["tooltip"].as_str().unwrap_or("").to_string(), x: num(b, "x"), y: num(b, "y"), shape })
                })
                .collect();
            out.panels.push(Panel {
                origin: pos * SCALE,
                axes: source_angles(ang.x, ang.y, ang.z),
                scale: num(p, "scale") * SCALE,
                name: name.clone(),
                size: (num(p, "width"), num(p, "height")),
                buttons,
            });
        }
        if let Some(f) = controls_json.parent().map(|d| d.join("tooltips_ru.json")) {
            if let Ok(t) = std::fs::read_to_string(f) {
                out.tips = serde_json::from_str(&t).unwrap_or_default();
            }
        }
        Ok(out)
    }

    pub fn button_count(&self) -> usize {
        self.panels.iter().map(|p| p.buttons.len()).sum()
    }

    /// Nearest button under the ray (panels are two-sided, like the 3D2D hit test).
    pub fn pick(&self, origin: Vec3, dir: Vec3, hidden: &std::collections::HashSet<String>) -> Option<Hit> {
        let mut best: Option<(f32, &Button, Vec3)> = None;
        for p in &self.panels {
            let n = p.axes.z_axis.truncate();
            let denom = dir.dot(n);
            if denom.abs() < 1e-6 {
                continue;
            }
            let t = (p.origin - origin).dot(n) / denom;
            if t <= 0.0 || best.is_some_and(|(bt, _, _)| bt <= t) {
                continue;
            }
            let rel = origin + dir * t - p.origin;
            let px = rel.dot(p.axes.x_axis.truncate()) / p.scale;
            let py = rel.dot(p.axes.y_axis.truncate()) / p.scale;
            for b in p.buttons.iter().filter(|b| !b.id.starts_with('!') && !hidden.contains(&b.id)) {
                let inside = match b.shape {
                    Shape::Rect { w, h } => px >= b.x && px <= b.x + w && py >= b.y && py <= b.y + h,
                    Shape::Circle { r } => (px - b.x).hypot(py - b.y) <= r,
                };
                if inside {
                    let (cx, cy) = match b.shape {
                        Shape::Rect { w, h } => (b.x + w / 2.0, b.y + h / 2.0),
                        Shape::Circle { .. } => (b.x, b.y),
                    };
                    let at = p.origin + (p.axes.x_axis.truncate() * cx + p.axes.y_axis.truncate() * cy) * p.scale;
                    best = Some((t, b, at));
                }
            }
        }
        best.map(|(_, b, pos)| Hit { id: b.id.clone(), tooltip: self.tips.get(&b.id).cloned().unwrap_or_else(|| b.tooltip.clone()), pos })
    }

    pub fn toggle(&mut self, id: &str) -> bool {
        let s = self.state.entry(id.to_string()).or_insert(false);
        *s = !*s;
        *s
    }
}

#[cfg(test)]
mod debug {
    use super::*;

    /// Screen pixels (1600x900, default cab camera) of some buttons, for driving the live window.
    #[test]
    fn button_pixels() {
        let path = Path::new(r"E:\metrostroi2pz\out\controls\717_mvm.json");
        if !path.exists() {
            return;
        }
        let ps = Panels::load(path).unwrap();
        let eye = Vec3::new(403.5, -4.72, 11.02) * crate::renderer::SCALE;
        let (yaw, pitch) = (0.0f32, -0.25f32);
        let dir = Vec3::new(yaw.cos() * pitch.cos(), yaw.sin() * pitch.cos(), pitch.sin());
        let vp = glam::Mat4::perspective_rh(65f32.to_radians(), 1600.0 / 900.0, 0.03, 200.0) * glam::Mat4::look_to_rh(eye, dir, Vec3::Z);
        for want in ["VBToggle", "A1Toggle", "R_UNchToggle", "L_4Toggle", "KVSet0", "RezMKToggle", "VUSToggle", "VozvratRPSet"] {
            for p in &ps.panels {
                for b in p.buttons.iter().filter(|b| b.id == want) {
                    let (cx, cy) = match b.shape {
                        Shape::Rect { w, h } => (b.x + w / 2.0, b.y + h / 2.0),
                        Shape::Circle { .. } => (b.x, b.y),
                    };
                    let w = p.origin + (p.axes.x_axis.truncate() * cx + p.axes.y_axis.truncate() * cy) * p.scale;
                    let c = vp.project_point3(w);
                    println!("PX {want} {:.0} {:.0}", (c.x + 1.0) / 2.0 * 1600.0, (1.0 - c.y) / 2.0 * 900.0);
                }
            }
        }
    }

    #[test]
    fn where_does_the_centre_ray_land() {
        let path = Path::new(r"E:\metrostroi2pz\out\controls\717_mvm.json");
        if !path.exists() {
            return;
        }
        let ps = Panels::load(path).unwrap();
        let eye = Vec3::new(403.5, -4.72, 11.02) * crate::renderer::SCALE;
        let (yaw, pitch) = (0.0f32, -0.25f32);
        let dir = Vec3::new(yaw.cos() * pitch.cos(), yaw.sin() * pitch.cos(), pitch.sin());
        for p in &ps.panels {
            let n = p.axes.z_axis.truncate();
            let d = dir.dot(n);
            if d.abs() < 1e-6 {
                continue;
            }
            let t = (p.origin - eye).dot(n) / d;
            if t <= 0.0 {
                continue;
            }
            let rel = eye + dir * t - p.origin;
            let (px, py) = (rel.dot(p.axes.x_axis.truncate()) / p.scale, rel.dot(p.axes.y_axis.truncate()) / p.scale);
            if t < 3.0 {
                println!("{:<14} t={t:.2} at ({px:.0},{py:.0}) of {:?} origin {:?}", p.name, p.size, p.origin);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ray fired at the centre of every rect button must hit that button (or one drawn over it).
    #[test]
    fn rays_at_button_centres_hit_panels() {
        let path = Path::new(r"E:\metrostroi2pz\out\controls\717_mvm.json");
        if !path.exists() {
            return;
        }
        let ps = Panels::load(path).unwrap();
        let (mut tried, mut ok) = (0, 0);
        for p in &ps.panels {
            for b in p.buttons.iter().filter(|b| !b.id.starts_with('!')) {
                let (cx, cy) = match b.shape {
                    Shape::Rect { w, h } => (b.x + w / 2.0, b.y + h / 2.0),
                    Shape::Circle { .. } => (b.x, b.y),
                };
                let target = p.origin + (p.axes.x_axis.truncate() * cx + p.axes.y_axis.truncate() * cy) * p.scale;
                let n = p.axes.z_axis.truncate();
                let eye = target + n * 0.5;
                tried += 1;
                if ps.pick(eye, -n, &Default::default()).is_some() {
                    ok += 1;
                }
            }
        }
        println!("{ok}/{tried}");
        assert_eq!(ok, tried);
    }
}
