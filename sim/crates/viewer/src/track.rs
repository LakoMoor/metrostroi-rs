//! Metrostroi track paths (`metrostroi_data/track_<map>.lua`, a JSON table of Source-unit node positions).

use anyhow::{Context, Result};
use glam::Vec3;
use serde_json::Value;
use std::path::Path;

/// Metrostroi's world scale: one Source unit is 0.01905 m.
pub const UNIT: f32 = crate::renderer::SCALE;

pub struct Path3 {
    /// Node positions in metres (Source axes: x east, y north, z up).
    pub pts: Vec<Vec3>,
    /// Distance from the first node to each node, metres.
    pub cum: Vec<f32>,
}

pub struct Sample {
    pub pos: Vec3,
    /// Unit tangent.
    pub dir: Vec3,
    /// Gradient in per mille, positive uphill along the direction of travel.
    pub grade: f32,
}

pub struct Track {
    pub paths: Vec<Path3>,
}

impl Path3 {
    /// Curve radius (m) over `window` metres around `s`: the chord's heading change against its length. Straight = infinity.
    pub fn radius_over(&self, s: f32, window: f32) -> f32 {
        let (a, b, c) = (self.at(s - window / 2.0), self.at(s), self.at(s + window / 2.0));
        let (d1, d2) = (a.dir.truncate().normalize_or_zero(), c.dir.truncate().normalize_or_zero());
        let turn = d1.perp_dot(d2).abs().asin().max(1e-6);
        let _ = b;
        window / turn
    }

    pub fn length(&self) -> f32 {
        *self.cum.last().unwrap_or(&0.0)
    }

    pub fn at(&self, s: f32) -> Sample {
        let s = s.clamp(0.0, self.length());
        let i = self.cum.partition_point(|&c| c <= s).clamp(1, self.pts.len() - 1);
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let seg = (self.cum[i] - self.cum[i - 1]).max(1e-4);
        let t = (s - self.cum[i - 1]) / seg;
        let d = b - a;
        let flat = d.truncate().length().max(1e-4);
        Sample { pos: a.lerp(b, t), dir: d.normalize_or_zero(), grade: d.z / flat * 1000.0 }
    }
}

impl Path3 {
    /// Gradient averaged over `window` metres around `s`, to smooth node-to-node noise.
    pub fn grade_over(&self, s: f32, window: f32) -> f32 {
        let (a, b) = (self.at(s - window / 2.0), self.at(s + window / 2.0));
        let flat = (b.pos - a.pos).truncate().length().max(1e-3);
        (b.pos.z - a.pos.z) / flat * 1000.0
    }
}

impl Track {
    pub fn load(file: &Path) -> Result<Track> {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(file).with_context(|| file.display().to_string())?)?;
        let mut paths = Vec::new();
        // either {"1": {"1": "[x y z]", ...}, ...} or [["[x y z]", ...], ...]
        let lists: Vec<&Value> = match &doc {
            Value::Object(o) => o.values().collect(),
            Value::Array(a) => a.iter().collect(),
            _ => anyhow::bail!("track file is neither an object nor an array"),
        };
        for p in lists {
            let entries: Vec<(u32, &Value)> = match p {
                Value::Object(o) => o.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect(),
                Value::Array(a) => a.iter().enumerate().map(|(i, v)| (i as u32, v)).collect(),
                _ => continue,
            };
            let mut nodes: Vec<(u32, Vec3)> = entries
                .into_iter()
                .filter_map(|(n, v)| {
                    let c: Vec<f32> = v.as_str()?.trim_matches(|c| c == '[' || c == ']').split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    (c.len() == 3).then(|| (n, Vec3::new(c[0], c[1], c[2]) * UNIT))
                })
                .collect();
            nodes.sort_by_key(|n| n.0);
            let pts: Vec<Vec3> = nodes.into_iter().map(|n| n.1).collect();
            if pts.len() < 2 {
                continue;
            }
            let mut cum = vec![0.0];
            for w in pts.windows(2) {
                cum.push(cum.last().unwrap() + w[0].distance(w[1]));
            }
            paths.push(Path3 { pts, cum });
        }
        paths.sort_by(|a, b| b.length().total_cmp(&a.length()));
        // The running line continues onto a path that starts within reach of its end (the return track of a
        // loop line), so the train does not dead-end; side branches (depots, crossovers) are not followed.
        if paths.len() > 1 {
            let end = *paths[0].pts.last().unwrap();
            let next = (1..paths.len())
                .filter(|&i| paths[i].length() > 1000.0)
                .min_by(|&a, &b| paths[a].pts[0].distance(end).total_cmp(&paths[b].pts[0].distance(end)));
            if let Some(i) = next.filter(|&i| paths[i].pts[0].distance(end) < 30.0) {
                let tail = paths.remove(i);
                let main = &mut paths[0];
                main.pts.extend(tail.pts);
                let mut cum = vec![0.0];
                for w in main.pts.windows(2) {
                    cum.push(cum.last().unwrap() + w[0].distance(w[1]));
                }
                main.cum = cum;
            }
        }
        Ok(Track { paths })
    }

    /// The longest path: the main line.
    pub fn main(&self) -> &Path3 {
        &self.paths[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossline_c4_main_line_continues_onto_the_return_track() {
        let f = Path::new("E:/metrostroi2pz/game/content/maps/crossline_c4/track.lua");
        if !f.exists() {
            return;
        }
        let t = Track::load(f).unwrap();
        println!("main line {:.0} m", t.main().length());
        assert!(t.main().length() > 20_000.0);
    }

    #[test]
    fn b46_lite_loads_and_has_gradients() {
        let f = Path::new(r"E:\metrostroi2pz\game\content\maps\b46_lite\track.lua");
        if !f.exists() {
            return;
        }
        let t = Track::load(f).unwrap();
        assert_eq!(t.paths.len(), 30);
        let m = t.main();
        println!("main line {:.0} m, {} nodes", m.length(), m.pts.len());
        let g: Vec<f32> = (0..m.length() as i32).step_by(10).map(|s| m.grade_over(s as f32, 30.0).abs()).collect();
        let steep = g.iter().filter(|&&x| x > 60.0).count();
        println!("{steep}/{} samples steeper than 60 per mille", g.len());
        assert!(m.length() > 5000.0 && steep * 2 < g.len());
        let a = m.at(0.0);
        let b = m.at(m.length());
        assert!(a.pos != b.pos && (a.dir.length() - 1.0).abs() < 1e-3);
    }
}

/// A trackside object from `signs_<map>.lua` (traffic light or switch), placed on the main line.
#[derive(Debug, Clone)]
pub struct Sign {
    pub class: String,
    pub pos: Vec3,
    /// Yaw in degrees (Source `Angle.y`).
    pub yaw: f32,
    pub settings: i64,
    pub name: String,
    /// `SignalType` (0 inside, 1 outside, 2 outside box), lens groups (`YY-GR`) and handedness of a traffic light.
    pub light_type: usize,
    pub lenses: String,
    pub left: bool,
    /// `SignType` and (y, z) offsets of a `gmod_track_signs` plate.
    pub plate: (usize, f32, f32),
    /// ARS code shown on a free line: the last digit of the default route's `ARSCodes` (the digits are the codes
    /// for 1, 2, 3... blocks of clear track, the first being the red/occupied case).
    pub ars: Option<u8>,
    /// Digits of the default route's `ARSCodes`: index k = code with k free blocks ahead (0 = this block is taken).
    pub codes: Vec<u8>,
    /// Distance along the main line and lateral offset (metres) when within reach of it.
    pub on_main: Option<(f32, f32)>,
}

fn vec3_of(s: &str) -> Option<Vec3> {
    let c: Vec<f32> = s.trim_matches(|c| c == '[' || c == ']' || c == '{' || c == '}').split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (c.len() == 3).then(|| Vec3::new(c[0], c[1], c[2]))
}

impl Path3 {
    /// Nearest point on the path: (distance along, lateral distance in the horizontal plane).
    pub fn project(&self, p: Vec3) -> (f32, f32) {
        let mut best = (0.0, f32::MAX);
        for i in 1..self.pts.len() {
            let (a, b) = (self.pts[i - 1], self.pts[i]);
            let ab = (b - a).truncate();
            let t = ((p - a).truncate().dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            let d = (a.truncate() + ab * t - p.truncate()).length();
            if d < best.1 {
                best = (self.cum[i - 1] + t * (self.cum[i] - self.cum[i - 1]), d);
            }
        }
        best
    }
}

pub fn load_signs(file: &Path, main: &Path3) -> Result<Vec<Sign>> {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(file).with_context(|| file.display().to_string())?)?;
    let mut out: Vec<Sign> = doc
        .as_object()
        .context("signs file is not an object")?
        .values()
        .filter_map(|v| {
            let pos = vec3_of(v["Pos"].as_str()?)? * UNIT;
            let yaw = v["Angles"].as_str().and_then(vec3_of).map_or(0.0, |a| a.y);
            let (s, lat) = main.project(pos);
            Some(Sign {
                class: v["Class"].as_str()?.to_string(),
                pos,
                yaw,
                settings: v["Settings"].as_i64().unwrap_or(0),
                name: v["Name"].as_str().unwrap_or("").to_string(),
                light_type: v["SignalType"].as_f64().unwrap_or(0.0) as usize,
                lenses: v["LensesStr"].as_str().unwrap_or("").to_string(),
                left: v["Left"].as_bool().unwrap_or(false),
                plate: (v["SignType"].as_f64().unwrap_or(0.0) as usize, v["YOffset"].as_f64().unwrap_or(0.0) as f32, v["ZOffset"].as_f64().unwrap_or(0.0) as f32),
                ars: v["Routes"][0]["ARSCodes"].as_str().and_then(|c| c.chars().last()).and_then(|c| c.to_digit(10)).map(|d| d as u8),
                codes: v["Routes"][0]["ARSCodes"].as_str().map(|c| c.chars().filter_map(|d| d.to_digit(10)).map(|d| d as u8).collect()).unwrap_or_default(),
                on_main: (lat < 8.0).then_some((s, lat)),
            })
        })
        .collect();
    out.sort_by(|a, b| a.on_main.map_or(f32::MAX, |m| m.0).total_cmp(&b.on_main.map_or(f32::MAX, |m| m.0)));
    Ok(out)
}

#[cfg(test)]
mod sign_tests {
    use super::*;

    #[test]
    fn signals_sit_along_the_main_line() {
        let dir = Path::new(r"E:\metrostroi2pz\game\content\maps\b46_lite");
        if !dir.join("signs.lua").exists() {
            return;
        }
        let t = Track::load(&dir.join("track.lua")).unwrap();
        let signs = load_signs(&dir.join("signs.lua"), t.main()).unwrap();
        let on: Vec<_> = signs.iter().filter(|s| s.on_main.is_some()).collect();
        let lights = on.iter().filter(|s| s.class == "gmod_track_signal").count();
        println!("{} signs, {} on the main line ({lights} signals)", signs.len(), on.len());
        assert_eq!(signs.len(), 523);
        assert!(lights > 20);
    }
}

/// Automatic block signals for a generated line: one every `spacing` metres on the right of the track, none within
/// 120 m of a station stop (`stops` = distances along the path), codes 0/40/60/80 for 0..3 clear blocks ahead.
pub fn generate_signs(main: &Path3, stops: &[f32], spacing: f32, rail_z: f32) -> Vec<Sign> {
    let mut out = Vec::new();
    let mut s = spacing;
    while s < main.length() - 50.0 {
        if !stops.iter().any(|&st| (st - s).abs() < 120.0) {
            let a = main.at(s);
            let d = Vec3::new(a.dir.x, a.dir.y, 0.0).normalize_or_zero();
            let right = Vec3::new(d.y, -d.x, 0.0);
            out.push(Sign {
                class: "gmod_track_signal".into(),
                pos: a.pos + right * 2.3 + Vec3::Z * rail_z,
                yaw: d.y.atan2(d.x).to_degrees(),
                settings: 0,
                name: format!("{}", out.len() + 1),
                light_type: 0,
                lenses: "YGR".into(),
                left: false,
                plate: (0, 0.0, 0.0),
                ars: Some(8),
                codes: vec![0, 4, 6, 8],
                on_main: Some((s, 2.3)),
            });
        }
        s += spacing;
    }
    out
}

/// The next signal ahead on the main line that faces the direction of travel: (distance ahead, sign).
pub fn next_signal<'a>(signs: &'a [Sign], main: &Path3, s: f32) -> Option<(f32, &'a Sign)> {
    let dir = main.at(s).dir;
    signs
        .iter()
        .filter(|g| g.class == "gmod_track_signal" && g.ars.is_some())
        .filter_map(|g| {
            let (gs, lat) = g.on_main?;
            let face = Vec3::new(g.yaw.to_radians().cos(), g.yaw.to_radians().sin(), 0.0);
            (gs > s && lat < 6.0 && face.dot(dir) > 0.5).then_some((gs - s, g))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// Signals on the main line facing travel in direction `dir`, in order along it.
fn facing<'a>(signs: &'a [Sign], dir: Vec3) -> impl Iterator<Item = (f32, &'a Sign)> {
    signs.iter().filter(|g| g.class == "gmod_track_signal" && g.ars.is_some()).filter_map(move |g| {
        let (gs, lat) = g.on_main?;
        let face = Vec3::new(g.yaw.to_radians().cos(), g.yaw.to_radians().sin(), 0.0);
        (lat < 6.0 && face.dot(dir) > 0.5).then_some((gs, g))
    })
}

/// ARS code a signal shows given trains standing at `obstacles` (metres along the main line): the number of
/// signals between it and the nearest obstacle ahead picks the digit; no obstacle = the free-line code.
pub fn aspect(signs: &[Sign], main: &Path3, sign: &Sign, obstacles: &[f32]) -> Option<u8> {
    let (s, _) = sign.on_main?;
    let free_line = sign.ars?;
    let Some(o) = obstacles.iter().copied().filter(|&o| o > s).min_by(f32::total_cmp) else { return Some(free_line) };
    let between = facing(signs, main.at(s).dir).filter(|(gs, _)| *gs > s && *gs < o).count();
    if between == 0 {
        return Some(0);
    }
    Some(sign.codes.get(between).or(sign.codes.last()).copied().unwrap_or(free_line))
}

#[cfg(test)]
mod ars_tests {
    use super::*;

    #[test]
    fn crossline_c4_has_signals_ahead_with_codes() {
        let dir = Path::new(r"E:\metrostroi2pz\game\content\maps\crossline_c4");
        if !dir.join("signs.lua").exists() {
            return;
        }
        let t = Track::load(&dir.join("track.lua")).unwrap();
        let signs = load_signs(&dir.join("signs.lua"), t.main()).unwrap();
        let mut found = 0;
        for s in (0..t.main().length() as i32).step_by(500) {
            if let Some((d, g)) = next_signal(&signs, t.main(), s as f32) {
                found += 1;
                if found < 4 {
                    println!("s={s}: next {} in {d:.0} m, ARS {:?}", g.name, g.ars);
                }
            }
        }
        println!("{found} probes found a signal");
        assert!(found > 3);
    }

    #[test]
    fn a_standing_train_turns_the_signals_behind_it_red_then_yellow() {
        let dir = Path::new(r"E:\metrostroi2pz\game\content\maps\crossline_c4");
        if !dir.join("signs.lua").exists() {
            return;
        }
        let t = Track::load(&dir.join("track.lua")).unwrap();
        let signs = load_signs(&dir.join("signs.lua"), t.main()).unwrap();
        let (_, first) = next_signal(&signs, t.main(), 300.0).unwrap();
        let s0 = first.on_main.unwrap().0;
        let (d1, second) = next_signal(&signs, t.main(), s0 + 1.0).unwrap();
        let block = s0 + 1.0 + d1 + 50.0; // a train just past the second signal
        let first_code = aspect(&signs, t.main(), first, &[block]).unwrap();
        let second_code = aspect(&signs, t.main(), second, &[block]).unwrap();
        println!("{} -> {first_code}, {} -> {second_code}", first.name, second.name);
        assert_eq!(second_code, 0, "the signal guarding the occupied block is red");
        assert_ne!(first_code, 0, "the one before it is not");
        assert_eq!(aspect(&signs, t.main(), first, &[]), first.ars);
    }
}

#[cfg(test)]
mod moscow_profile {
    use super::*;

    /// The generated saddle profile: flat at the platform, falling after it, rising into the next one, never steeper than 35 per mille.
    #[test]
    fn moscow_curve_radii_are_plausible() {
        let f = std::path::Path::new("E:/metrostroi2pz/game/content/maps/moscow/305810/track.lua");
        if !f.exists() {
            return;
        }
        let p = Track::load(f).unwrap();
        let p = p.main();
        let mut tight = 0;
        let mut min = f32::MAX;
        let mut s = 0.0;
        while s < p.length() {
            let r = p.radius_over(s, 40.0);
            min = min.min(r);
            if r < 150.0 {
                tight += 1;
            }
            s += 20.0;
        }
        println!("tightest {min:.0} m, {tight} samples under 150 m of {}", (p.length() / 20.0) as u32);
        assert!(min > 80.0, "tightest curve {min} m");
    }

    #[test]
    fn moscow_lines_dip_between_stations() {
        let f = std::path::Path::new("E:/metrostroi2pz/game/content/maps/moscow/305810/track.lua");
        if !f.exists() {
            return;
        }
        let t = Track::load(f).unwrap();
        let p = t.main();
        let (mut down, mut up, mut steepest) = (false, false, 0.0f32);
        let mut s = 20.0;
        while s < 1500.0 {
            let g = p.grade_over(s, 30.0);
            down |= g < -3.0;
            up |= g > 3.0;
            steepest = steepest.max(g.abs());
            s += 20.0;
        }
        assert!(down && up, "no dip between the first two stations");
        assert!(steepest < 36.0, "steepest {steepest}");
        assert!(p.grade_over(2.0, 30.0).abs() < 3.0, "not flat at the first platform");
    }
}
