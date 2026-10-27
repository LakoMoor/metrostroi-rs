//! Station platforms from the map's `gmod_track_platform` entities, projected onto the main line.

use crate::renderer::SCALE;
use crate::track::Path3;
use glam::Vec3;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct Platform {
    pub station: u32,
    pub index: u32,
    /// Distance along the main line of the two ends (start < end), metres.
    pub from: f32,
    pub to: f32,
    /// Side of the direction of travel the platform is on (doors open that way).
    pub side: Side,
    pub last: bool,
}

fn origin(e: &HashMap<String, String>) -> Option<Vec3> {
    let c: Vec<f32> = e.get("origin")?.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (c.len() == 3).then(|| Vec3::new(c[0], c[1], c[2]) * SCALE)
}

/// Platforms within reach of the main line, ordered along it.
pub fn load(ents: &[HashMap<String, String>], main: &Path3) -> Vec<Platform> {
    let named: HashMap<&str, Vec3> =
        ents.iter().filter_map(|e| Some((e.get("targetname")?.as_str(), origin(e)?))).collect();
    let mut out: Vec<Platform> = ents
        .iter()
        .filter(|e| e.get("classname").is_some_and(|c| c == "gmod_track_platform"))
        .filter_map(|e| {
            let a = *named.get(e.get("PlatformStart")?.as_str())?;
            let b = *named.get(e.get("PlatformEnd")?.as_str())?;
            let (sa, la) = main.project(a);
            let (sb, lb) = main.project(b);
            // both ends must land on the same pass of the line (it loops back near some stations)
            if la.max(lb) > 12.0 || !(40.0..250.0).contains(&(sa - sb).abs()) {
                return None;
            }
            let mid = (a + b) * 0.5;
            let (sm, _) = main.project(mid);
            let at = main.at(sm);
            let side = if at.dir.cross(mid - at.pos).z > 0.0 { Side::Left } else { Side::Right };
            Some(Platform {
                station: e.get("StationIndex")?.parse().ok()?,
                index: e.get("PlatformIndex").and_then(|v| v.parse().ok()).unwrap_or(1),
                from: sa.min(sb),
                to: sa.max(sb),
                side,
                last: e.get("PlatformLast").is_some_and(|v| v == "yes"),
            })
        })
        .collect();
    out.sort_by(|a, b| a.from.total_cmp(&b.from));
    out
}

/// Station names (Russian) from the map's `Metrostroi.StationConfigurations` table, found by scanning the Lua text.
pub fn names(lua_file: &std::path::Path) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(lua_file) else { return out };
    let mut cur: Option<u32> = None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix('[') {
            if let Some((n, tail)) = rest.split_once(']') {
                if tail.trim_start().starts_with('=') {
                    cur = n.trim().parse().ok();
                }
            }
        } else if let (Some(id), Some(rest)) = (cur, l.strip_prefix("names")) {
            if let Some(q) = rest.split('"').nth(1) {
                let mut c = q.chars();
                let cap: String = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
                out.insert(id, cap);
                cur = None;
            }
        }
    }
    out
}

/// The platform the train is on or the next one ahead: (platform, metres to its start (<= 0 when alongside)).
pub fn ahead(platforms: &[Platform], s: f32) -> Option<(&Platform, f32)> {
    platforms.iter().filter(|p| p.to > s).min_by(|a, b| a.from.total_cmp(&b.from)).map(|p| (p, p.from - s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::Track;
    use std::path::Path;

    #[test]
    fn station_names_are_read_from_the_map_lua() {
        let f = Path::new(r"E:\metrostroi2pz\src\scripts\lua\metrostroi\maps\crossline.lua");
        if !f.exists() {
            return;
        }
        let n = names(f);
        assert_eq!(n.get(&110).map(String::as_str), Some("Международная"));
        assert!(n.len() > 5, "{n:?}");
    }

    #[test]
    fn crossline_c4_has_platforms_along_the_main_line() {
        let dir = Path::new(r"E:\metrostroi2pz\game\content\maps\crossline_c4");
        if !dir.join("map.bsp").exists() {
            return;
        }
        let t = Track::load(&dir.join("track.lua")).unwrap();
        let ents = formats::bsp::entities(&std::fs::read(dir.join("map.bsp")).unwrap()).unwrap();
        let ps = load(&ents, t.main());
        for p in &ps {
            println!("station {} platform {}: {:.0}..{:.0} m, {:?}", p.station, p.index, p.from, p.to, p.side);
        }
        assert!(!ps.is_empty(), "no platforms on the main line");
        assert!(ps.iter().all(|p| p.to - p.from > 20.0 && p.to - p.from < 400.0), "implausible platform lengths");
    }
}
