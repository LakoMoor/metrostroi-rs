//! Walkable floor taken from the map's triangles: a downward ray query on a coarse grid.

use glam::Vec3;
use std::collections::HashMap;

const CELL: f32 = 4.0;

#[derive(Default)]
pub struct Ground {
    cells: HashMap<(i32, i32), Vec<[Vec3; 3]>>,
    /// Steep triangles (walls, pillars) for horizontal collision.
    walls: HashMap<(i32, i32), Vec<[Vec3; 3]>>,
}

static PENDING: std::sync::Mutex<Option<Ground>> = std::sync::Mutex::new(None);

/// The scene loader builds the grid; the app picks it up once the scene exists.
pub fn store(g: Ground) {
    *PENDING.lock().unwrap() = Some(g);
}

pub fn take() -> Option<Ground> {
    PENDING.lock().unwrap().take()
}

fn cell(x: f32) -> i32 {
    (x / CELL).floor() as i32
}

impl Ground {
    /// Upward-facing triangles of `meshes` (Source units scaled by `scale`).
    pub fn build(meshes: &[formats::mdl::Mesh], scale: f32) -> Ground {
        let mut g = Ground::default();
        for m in meshes {
            for t in m.indices.chunks_exact(3) {
                let p = |i: u32| Vec3::from(m.vertices[i as usize].pos) * scale;
                let tri = [p(t[0]), p(t[1]), p(t[2])];
                let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
                let (mn, mx) = (tri[0].min(tri[1]).min(tri[2]), tri[0].max(tri[1]).max(tri[2]));
                if n.z.abs() < 0.5 * n.length() {
                    // walls and steep slopes
                    for cx in cell(mn.x)..=cell(mx.x) {
                        for cy in cell(mn.y)..=cell(mx.y) {
                            g.walls.entry((cx, cy)).or_default().push(tri);
                        }
                    }
                    continue;
                }
                for cx in cell(mn.x)..=cell(mx.x) {
                    for cy in cell(mn.y)..=cell(mx.y) {
                        g.cells.entry((cx, cy)).or_default().push(tri);
                    }
                }
            }
        }
        g
    }

    /// True if a body of `radius` standing at `feet` (map space) touches a wall between knee and head height.
    pub fn blocked(&self, feet: Vec3, radius: f32) -> bool {
        let r = radius + 0.0;
        for cx in cell(feet.x - r)..=cell(feet.x + r) {
            for cy in cell(feet.y - r)..=cell(feet.y + r) {
                let Some(tris) = self.walls.get(&(cx, cy)) else { continue };
                for t in tris {
                    // skip triangles wholly above the head or below the knee
                    if t[0].z.min(t[1].z).min(t[2].z) > feet.z + 1.7 || t[0].z.max(t[1].z).max(t[2].z) < feet.z + 0.45 {
                        continue;
                    }
                    for h in [0.5, 1.0, 1.5] {
                        let p = feet + Vec3::Z * h;
                        if (closest_on_triangle(p, t[0], t[1], t[2]) - p).length() < r {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Highest floor at (x, y) that is not above `limit`.
    pub fn height(&self, x: f32, y: f32, limit: f32) -> Option<f32> {
        let tris = self.cells.get(&(cell(x), cell(y)))?;
        let mut best: Option<f32> = None;
        for t in tris {
            let (a, b, c) = (t[0], t[1], t[2]);
            let d = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
            if d.abs() < 1e-9 {
                continue;
            }
            let u = ((b.y - c.y) * (x - c.x) + (c.x - b.x) * (y - c.y)) / d;
            let v = ((c.y - a.y) * (x - c.x) + (a.x - c.x) * (y - c.y)) / d;
            if u < 0.0 || v < 0.0 || u + v > 1.0 {
                continue;
            }
            let z = u * a.z + v * b.z + (1.0 - u - v) * c.z;
            if z <= limit && best.is_none_or(|b| z > b) {
                best = Some(z);
            }
        }
        best
    }
}

/// Closest point of triangle abc to p (Ericson, Real-Time Collision Detection).
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_wall_blocks_a_walker_but_not_one_beside_it() {
        use super::*;
        // a vertical wall in the plane x = 5, spanning y 0..10, z 0..3
        let quad = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| {
            let v = |p: Vec3| formats::mdl::Vertex { pos: p.to_array(), normal: [1.0, 0.0, 0.0], uv: [0.0, 0.0], joints: [0; 3], weights: [1.0, 0.0, 0.0], nbones: 0 };
            formats::mdl::Mesh { material: String::new(), vertices: vec![v(a), v(b), v(c), v(d)], indices: vec![0, 1, 2, 0, 2, 3] }
        };
        let wall = quad(Vec3::new(5.0, 0.0, 0.0), Vec3::new(5.0, 10.0, 0.0), Vec3::new(5.0, 10.0, 3.0), Vec3::new(5.0, 0.0, 3.0));
        let g = Ground::build(&[wall], 1.0);
        assert!(g.blocked(Vec3::new(4.9, 5.0, 0.0), 0.3));
        assert!(!g.blocked(Vec3::new(4.0, 5.0, 0.0), 0.3));
        assert!(!g.blocked(Vec3::new(4.9, 5.0, 4.0), 0.3), "above the wall");
    }
}

#[cfg(test)]
mod floor_tests {
    use super::*;

    #[test]
    fn finds_the_highest_floor_below_the_limit() {
        let mk = |z: f32| formats::mdl::Mesh {
            material: String::new(),
            vertices: [[0.0, 0.0, z], [10.0, 0.0, z], [0.0, 10.0, z]]
                .into_iter()
                .map(|p| formats::mdl::Vertex { pos: p, normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0], joints: [0; 3], weights: [1.0, 0.0, 0.0], nbones: 0 })
                .collect(),
            indices: vec![0, 1, 2],
        };
        let g = Ground::build(&[mk(0.0), mk(3.0)], 1.0);
        assert_eq!(g.height(2.0, 2.0, 5.0), Some(3.0));
        assert_eq!(g.height(2.0, 2.0, 1.0), Some(0.0));
        assert_eq!(g.height(20.0, 20.0, 5.0), None);
    }
}
