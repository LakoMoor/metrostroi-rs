//! A generated tunnel and stations along a track path, for maps that have no Source level (the Moscow metro
//! routes built from OpenStreetMap). The world is cut into short chunks, each with its own origin, so single
//! precision stays exact over tens of kilometres and only the chunks near the train are drawn.

use crate::track::Path3;
use formats::mdl::{Mesh, Vertex};
use glam::Vec3;

/// Length of one chunk along the track, metres.
pub const CHUNK: f32 = 100.0;
/// Distance between cross-sections, metres.
const STEP: f32 = 4.0;
/// Track gauge: rail centres are this far apart, metres.
const GAUGE: f32 = 1.52;
/// Height of a platform edge above the rail head, metres (the 81-717 floor is about 1.3 m up).
const PLATFORM_Z: f32 = 1.3;
const SLAB_Z: f32 = -0.35;

#[derive(Clone, Copy, Debug)]
pub struct Station {
    /// Hall start and end along the path, metres.
    pub from: f32,
    pub to: f32,
    /// Platform on the left of the direction of travel.
    pub left: bool,
}

pub struct Chunk {
    /// Map-space origin (metres) the vertices are relative to.
    pub origin: Vec3,
    /// Vertices in Source units relative to `origin`.
    pub meshes: Vec<Mesh>,
    /// Brightness of the chunk: stations are lit, the tunnel is dim.
    pub light: f32,
}

const M_FLOOR: &str = "models/metrostroi_tunnels/tunnelfloor_001";
const M_WALL: &str = "models/metrostroi_tunnels/tunnelwall_001";
const M_RAIL: &str = "models/metrostroi_tunnels/railroad_001";
const M_PLATFORM: &str = "metro/metromarble016";
const M_HALL_WALL: &str = "metro/metromarble010";
const M_EDGE: &str = "metro/metroconcrete001";
const M_CEIL: &str = "plaster/wall_white01";

/// One extruded profile: lateral/height points (left of travel positive), the material, which side the
/// normals face (towards `toward`, or away from it) and the texture repeat along the profile.
struct Profile {
    pts: Vec<(f32, f32)>,
    closed: bool,
    material: &'static str,
    toward: (f32, f32),
    away: bool,
    v_scale: f32,
}

struct Frame {
    pos: Vec3,
    left: Vec3,
    s: f32,
}

fn frames(path: &Path3) -> Vec<Frame> {
    let n = (path.length() / STEP).ceil() as usize + 1;
    (0..n)
        .map(|i| {
            let s = (i as f32 * STEP).min(path.length());
            let a = path.at(s);
            let d = Vec3::new(a.dir.x, a.dir.y, 0.0).normalize_or_zero();
            Frame { pos: a.pos, left: Vec3::new(-d.y, d.x, 0.0), s }
        })
        .collect()
}

fn ring(radius: f32, centre_z: f32, n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|k| {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            (radius * a.cos(), centre_z + radius * a.sin())
        })
        .collect()
}

fn rail(lat: f32) -> Profile {
    Profile {
        pts: vec![(lat - 0.035, -0.15), (lat - 0.035, 0.0), (lat + 0.035, 0.0), (lat + 0.035, -0.15)],
        closed: false,
        material: M_RAIL,
        toward: (lat, -0.07),
        away: true,
        v_scale: 1.0,
    }
}

/// Sweep `p` along frames `a..b` (inclusive) of `fr`, appending to `mesh`; positions are relative to `origin`.
fn sweep(mesh: &mut Mesh, fr: &[Frame], origin: Vec3, z0: f32, p: &Profile) {
    let m = p.pts.len();
    let segs = if p.closed { m } else { m - 1 };
    let at = |f: &Frame, (lat, z): (f32, f32)| f.pos + f.left * lat + Vec3::Z * (z0 + z) - origin;
    for w in fr.windows(2) {
        let (fa, fb) = (&w[0], &w[1]);
        let base = mesh.vertices.len() as u32;
        let mut acc = 0.0;
        for k in 0..segs {
            let (p0, p1) = (p.pts[k], p.pts[(k + 1) % m]);
            let len = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt().max(1e-4);
            // normal of the edge in the (lateral, height) plane, pointing towards/away from the reference point
            let mut n2 = (-(p1.1 - p0.1) / len, (p1.0 - p0.0) / len);
            let mid = ((p0.0 + p1.0) * 0.5, (p0.1 + p1.1) * 0.5);
            let to = (p.toward.0 - mid.0, p.toward.1 - mid.1);
            let facing = n2.0 * to.0 + n2.1 * to.1;
            if (facing > 0.0) == p.away {
                n2 = (-n2.0, -n2.1);
            }
            let normal = fa.left * n2.0 + Vec3::Z * n2.1;
            let (ua, ub) = (fa.s * 0.5, fb.s * 0.5);
            let (va, vb) = (acc * p.v_scale * 0.5, (acc + len) * p.v_scale * 0.5);
            let quad = [(fa, p0, ua, va), (fa, p1, ua, vb), (fb, p0, ub, va), (fb, p1, ub, vb)];
            let i0 = mesh.vertices.len() as u32;
            for (f, pt, u, v) in quad {
                let pos = at(f, pt) / crate::renderer::SCALE;
                mesh.vertices.push(Vertex { pos: pos.to_array(), normal: normal.to_array(), uv: [u, v], ..Default::default() });
            }
            // front faces are counter-clockwise seen from the side the normal points to
            for tri in [[0u32, 1, 2], [1, 3, 2]] {
                let v = |j: u32| Vec3::from(mesh.vertices[(i0 + tri[j as usize]) as usize].pos);
                let c = (v(1) - v(0)).cross(v(2) - v(0));
                if c.dot(normal) < 0.0 {
                    mesh.indices.extend([i0 + tri[0], i0 + tri[2], i0 + tri[1]]);
                } else {
                    mesh.indices.extend([i0 + tri[0], i0 + tri[1], i0 + tri[2]]);
                }
            }
            acc += len;
        }
        let _ = base;
    }
}

fn mesh_for<'a>(meshes: &'a mut Vec<Mesh>, material: &str) -> &'a mut Mesh {
    if let Some(i) = meshes.iter().position(|m| m.material == material) {
        return &mut meshes[i];
    }
    meshes.push(Mesh { material: material.to_string(), vertices: Vec::new(), indices: Vec::new() });
    meshes.last_mut().unwrap()
}

fn tunnel_profiles() -> Vec<Profile> {
    vec![
        Profile { pts: ring(2.75, 1.7, 18), closed: true, material: M_WALL, toward: (0.0, 1.7), away: false, v_scale: 1.0 },
        Profile { pts: vec![(-2.2, SLAB_Z), (2.2, SLAB_Z)], closed: false, material: M_FLOOR, toward: (0.0, 2.0), away: false, v_scale: 1.0 },
        rail(-GAUGE / 2.0),
        rail(GAUGE / 2.0),
    ]
}

/// The hall of a station with the platform on the side `sg` (+1 left, -1 right).
fn hall_profiles(sg: f32) -> Vec<Profile> {
    let (track_wall, far_wall, ceil) = (-sg * 5.5, sg * 12.0, 6.2);
    let inside = (0.0, 3.0);
    vec![
        // platform top and its edge
        Profile { pts: vec![(sg * 1.75, PLATFORM_Z), (far_wall, PLATFORM_Z)], closed: false, material: M_PLATFORM, toward: inside, away: false, v_scale: 1.0 },
        Profile { pts: vec![(sg * 1.75, SLAB_Z), (sg * 1.75, PLATFORM_Z)], closed: false, material: M_EDGE, toward: (0.0, 0.5), away: false, v_scale: 1.0 },
        // track bed
        Profile { pts: vec![(track_wall, SLAB_Z), (sg * 1.75, SLAB_Z)], closed: false, material: M_FLOOR, toward: inside, away: false, v_scale: 1.0 },
        // walls and ceiling
        Profile { pts: vec![(track_wall, SLAB_Z), (track_wall, ceil)], closed: false, material: M_HALL_WALL, toward: inside, away: false, v_scale: 1.0 },
        Profile { pts: vec![(far_wall, PLATFORM_Z), (far_wall, ceil)], closed: false, material: M_HALL_WALL, toward: inside, away: false, v_scale: 1.0 },
        Profile { pts: vec![(track_wall, ceil), (far_wall, ceil)], closed: false, material: M_CEIL, toward: inside, away: false, v_scale: 1.0 },
        rail(-GAUGE / 2.0),
        rail(GAUGE / 2.0),
    ]
}

/// Build the whole line. `z_offset` lowers the rail head against the track nodes (so the car's wheels sit on it).
pub fn build(path: &Path3, stations: &[Station], z_offset: f32) -> Vec<Chunk> {
    let fr = frames(path);
    let in_hall = |s: f32| stations.iter().find(|st| s >= st.from && s <= st.to);
    let per = (CHUNK / STEP) as usize;
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < fr.len() {
        let j = (i + per).min(fr.len() - 1);
        let slice = &fr[i..=j];
        let origin = slice[slice.len() / 2].pos;
        let mut meshes = Vec::new();
        let mut hall = false;
        // consecutive frames belong to a run of the same kind; build each run with its profiles
        let mut a = 0;
        while a + 1 < slice.len() {
            let kind = in_hall(slice[a].s).filter(|st| slice[a + 1].s <= st.to).copied();
            let mut b = a + 1;
            while b + 1 < slice.len() && in_hall(slice[b].s).filter(|st| slice[b + 1].s <= st.to).map(|s| s.left) == kind.map(|s| s.left) {
                b += 1;
            }
            let run = &slice[a..=b];
            match kind {
                Some(st) => {
                    hall = true;
                    for p in hall_profiles(if st.left { 1.0 } else { -1.0 }) {
                        sweep(mesh_for(&mut meshes, p.material), run, origin, z_offset, &p);
                    }
                }
                None => {
                    for p in tunnel_profiles() {
                        sweep(mesh_for(&mut meshes, p.material), run, origin, z_offset, &p);
                    }
                }
            }
            a = b;
        }
        meshes.retain(|m| !m.indices.is_empty());
        out.push(Chunk { origin, meshes, light: if hall { 1.0 } else { 0.55 } });
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight(len: f32) -> Path3 {
        let pts: Vec<Vec3> = (0..=(len / 8.0) as usize).map(|i| Vec3::new(i as f32 * 8.0, 0.0, -19.0)).collect();
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            cum.push(cum.last().unwrap() + w[0].distance(w[1]));
        }
        Path3 { pts, cum }
    }

    #[test]
    fn builds_chunks_with_tunnel_and_hall() {
        let path = straight(600.0);
        let chunks = build(&path, &[Station { from: 250.0, to: 410.0, left: false }], -0.62);
        assert!(chunks.len() >= 5, "{} chunks", chunks.len());
        assert!(chunks.iter().any(|c| c.light > 0.9), "a station chunk is lit");
        assert!(chunks.iter().any(|c| c.light < 0.6), "tunnel chunks are dim");
        // vertices stay close to their chunk origin
        for c in &chunks {
            for m in &c.meshes {
                assert!(m.indices.len() % 3 == 0);
                for v in &m.vertices {
                    let p = Vec3::from(v.pos) * crate::renderer::SCALE;
                    assert!(p.length() < 80.0, "vertex {p:?} far from chunk origin");
                }
            }
        }
    }

    #[test]
    fn triangles_face_the_inside_of_the_tube() {
        let path = straight(80.0);
        let chunks = build(&path, &[], 0.0);
        let m = chunks[0].meshes.iter().find(|m| m.material == M_WALL).unwrap();
        let mut inward = 0;
        for t in m.indices.chunks(3) {
            let v: Vec<Vec3> = t.iter().map(|&i| Vec3::from(m.vertices[i as usize].pos)).collect();
            let n = Vec3::from(m.vertices[t[0] as usize].normal);
            let c = (v[1] - v[0]).cross(v[2] - v[0]);
            if c.dot(n) > 0.0 {
                inward += 1;
            }
        }
        assert_eq!(inward, m.indices.len() / 3);
    }
}
