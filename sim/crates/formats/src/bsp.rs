//! Source maps: `.bsp` v19/v20 world geometry (no displacements, no lightmaps yet).
//!
//! Output reuses [`crate::mdl::Mesh`]: one mesh per material, Source space (Z up, inches).

use crate::mdl::{Mesh, Vertex};
use anyhow::{ensure, Result};
use std::collections::HashMap;

struct Bsp<'a>(&'a [u8]);

impl<'a> Bsp<'a> {
    fn lump(&self, i: usize) -> &'a [u8] {
        let o = 8 + i * 16;
        let off = i32::from_le_bytes(self.0[o..o + 4].try_into().unwrap()) as usize;
        let len = i32::from_le_bytes(self.0[o + 4..o + 8].try_into().unwrap()) as usize;
        &self.0[off..off + len]
    }
}

fn i32_at(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
fn i16_at(b: &[u8], o: usize) -> i16 {
    u16_at(b, o) as i16
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

const SURF_SKIP_MASK: i32 = 0x2 | 0x4 | 0x40 | 0x80 | 0x100 | 0x200; // sky2d, sky, trigger, nodraw, hint, skip

/// World geometry grouped by material name (lowercase, no extension).
pub fn load(data: &[u8]) -> Result<Vec<Mesh>> {
    ensure!(data.len() > 1036 && &data[0..4] == b"VBSP", "not a VBSP file");
    let ver = i32_at(data, 4);
    ensure!(ver == 19 || ver == 20, "unsupported BSP version {ver}");
    let b = Bsp(data);
    let planes = b.lump(1);
    let texdata = b.lump(2);
    let verts = b.lump(3);
    let texinfo = b.lump(6);
    let faces = b.lump(7);
    let edges = b.lump(12);
    let surfedges = b.lump(13);
    let tex_table = b.lump(44);
    let tex_strings = b.lump(43);

    let name_of = |td: usize| -> String {
        let id = i32_at(texdata, td * 32 + 12) as usize;
        let off = i32_at(tex_table, id * 4) as usize;
        let end = tex_strings[off..].iter().position(|&c| c == 0).map_or(tex_strings.len(), |e| off + e);
        String::from_utf8_lossy(&tex_strings[off..end]).to_ascii_lowercase().replace('\\', "/")
    };
    let vert = |i: usize| [f32_at(verts, i * 12), f32_at(verts, i * 12 + 4), f32_at(verts, i * 12 + 8)];

    let mut meshes: Vec<Mesh> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for f in 0..faces.len() / 56 {
        let fo = f * 56;
        let (plane, side) = (u16_at(faces, fo) as usize, faces[fo + 2]);
        let (first, n) = (i32_at(faces, fo + 4) as usize, i16_at(faces, fo + 8) as usize);
        let ti = i16_at(faces, fo + 10);
        let disp = i16_at(faces, fo + 12);
        if disp >= 0 || ti < 0 || n < 3 {
            continue;
        }
        let ti = ti as usize;
        if i32_at(texinfo, ti * 72 + 64) & SURF_SKIP_MASK != 0 {
            continue;
        }
        let td = i32_at(texinfo, ti * 72 + 68) as usize;
        let name = name_of(td);
        if name.starts_with("tools/") {
            continue;
        }
        let (tw, th) = (i32_at(texdata, td * 32 + 16).max(1) as f32, i32_at(texdata, td * 32 + 20).max(1) as f32);
        let mut nrm = [f32_at(planes, plane * 20), f32_at(planes, plane * 20 + 4), f32_at(planes, plane * 20 + 8)];
        if side != 0 {
            nrm = nrm.map(|c| -c);
        }
        let ts = |k: usize| [f32_at(texinfo, ti * 72 + k * 16), f32_at(texinfo, ti * 72 + k * 16 + 4), f32_at(texinfo, ti * 72 + k * 16 + 8), f32_at(texinfo, ti * 72 + k * 16 + 12)];
        let (sv, tv) = (ts(0), ts(1));

        let idx = *by_name.entry(name.clone()).or_insert_with(|| {
            meshes.push(Mesh { material: name.clone(), vertices: Vec::new(), indices: Vec::new() });
            meshes.len() - 1
        });
        let m = &mut meshes[idx];
        let base = m.vertices.len() as u32;
        for k in 0..n {
            let se = i32_at(surfedges, (first + k) * 4);
            let e = se.unsigned_abs() as usize;
            let vi = u16_at(edges, e * 4 + if se < 0 { 2 } else { 0 }) as usize;
            let p = vert(vi);
            let dot = |v: [f32; 4]| p[0] * v[0] + p[1] * v[1] + p[2] * v[2] + v[3];
            m.vertices.push(Vertex { pos: p, normal: nrm, uv: [dot(sv) / tw, dot(tv) / th], nbones: 0, ..Default::default() });
        }
        for k in 1..n as u32 - 1 {
            m.indices.extend_from_slice(&[base, base + k, base + k + 1]);
        }
    }
    Ok(meshes)
}

/// Files embedded in the map (lump 40 is a zip): `materials/...`, `models/...` and so on.
pub fn pakfile(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    use std::io::Read;
    ensure!(data.len() > 1036 && &data[0..4] == b"VBSP", "not a VBSP file");
    let pak = Bsp(data).lump(40);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(pak))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
        out.push((f.name().to_string(), buf));
    }
    Ok(out)
}

/// The map's entity lump (lump 0): one key/value map per `{ ... }` block.
pub fn entities(data: &[u8]) -> Result<Vec<std::collections::HashMap<String, String>>> {
    ensure!(data.len() > 1036 && &data[0..4] == b"VBSP", "not a VBSP file");
    let text = String::from_utf8_lossy(Bsp(data).lump(0));
    let mut out = Vec::new();
    let mut cur: Option<std::collections::HashMap<String, String>> = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "{" {
            cur = Some(Default::default());
        } else if line == "}" {
            out.extend(cur.take());
        } else if let (Some(c), true) = (cur.as_mut(), line.starts_with('"')) {
            let mut parts = line.split('"').filter(|p| !p.trim().is_empty());
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                c.insert(k.to_string(), v.to_string());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn crossline_c4_world_geometry() {
        let p = std::path::Path::new(&std::env::var("TEMP").unwrap_or_default()).join("c4.bsp");
        let Ok(data) = std::fs::read(&p) else { return };
        let m = super::load(&data).unwrap();
        let tris: usize = m.iter().map(|x| x.indices.len() / 3).sum();
        println!("{} materials, {tris} triangles", m.len());
        assert!(m.len() > 20 && tris > 10_000);
        let files = super::pakfile(&data).unwrap();
        println!("pakfile: {} files", files.len());
        assert!(files.iter().any(|f| f.0.ends_with(".vtf")));
    }
}
