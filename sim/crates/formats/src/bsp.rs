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

/// All face lightmaps packed into one RGBA8 atlas (linear light / 2, so values up to 2.0 fit).
pub struct Lightmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// World geometry grouped by material name (lowercase, no extension).
pub fn load(data: &[u8]) -> Result<Vec<Mesh>> {
    Ok(load_lit(data)?.0)
}

/// World geometry plus the packed lightmaps (`uv2` of every vertex addresses the atlas).
pub fn load_lit(data: &[u8]) -> Result<(Vec<Mesh>, Option<Lightmap>)> {
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
    let lighting = b.lump(8);
    let dispinfo = b.lump(26);
    let dispverts = b.lump(33);

    let name_of = |td: usize| -> String {
        let id = i32_at(texdata, td * 32 + 12) as usize;
        let off = i32_at(tex_table, id * 4) as usize;
        let end = tex_strings[off..].iter().position(|&c| c == 0).map_or(tex_strings.len(), |e| off + e);
        String::from_utf8_lossy(&tex_strings[off..end]).to_ascii_lowercase().replace('\\', "/")
    };
    let vert = |i: usize| [f32_at(verts, i * 12), f32_at(verts, i * 12 + 4), f32_at(verts, i * 12 + 8)];

    let mut meshes: Vec<Mesh> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    // atlas: shelf packing, 2048 wide; the first 4x4 block is white for faces without a lightmap
    const AW: u32 = 2048;
    let mut blocks: Vec<(u32, u32, u32, u32, usize)> = Vec::new(); // x, y, w, h, byte offset in the lighting lump
    let (mut sx, mut sy, mut shelf) = (4u32, 0u32, 4u32);
    for f in 0..faces.len() / 56 {
        let fo = f * 56;
        let (plane, side) = (u16_at(faces, fo) as usize, faces[fo + 2]);
        let (first, n) = (i32_at(faces, fo + 4) as usize, i16_at(faces, fo + 8) as usize);
        let ti = i16_at(faces, fo + 10);
        let disp = i16_at(faces, fo + 12);
        if std::env::var_os("MS_BSPLOG").is_some() && (ti < 0 || n < 3 || disp >= 0) {
            eprintln!("FACE ti {ti} n {n} disp {disp} dispinfo_len {}", dispinfo.len() / 176);
        }
        if ti < 0 || n < 3 || (disp >= 0 && (n != 4 || (disp as usize + 1) * 176 > dispinfo.len())) {
            continue;
        }
        let ti = ti as usize;
        if i32_at(texinfo, ti * 72 + 64) & SURF_SKIP_MASK != 0 {
            if std::env::var_os("MS_BSPLOG").is_some() {
                eprintln!("SKIPFLAG {} flags {:x}", name_of(i32_at(texinfo, ti * 72 + 68) as usize), i32_at(texinfo, ti * 72 + 64));
            }
            continue;
        }
        let td = i32_at(texinfo, ti * 72 + 68) as usize;
        let name = name_of(td);
        if name.starts_with("tools/") {
            if std::env::var_os("MS_BSPLOG").is_some() {
                eprintln!("TOOLS {name}");
            }
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
        // lightmap block of this face
        let (lw, lh) = ((i32_at(faces, fo + 36) + 1).max(1) as u32, (i32_at(faces, fo + 40) + 1).max(1) as u32);
        let lofs = i32_at(faces, fo + 20);
        let lit = lofs >= 0 && faces[fo + 16] != 255 && lofs as usize + (lw * lh * 4) as usize <= lighting.len() && lw < 512 && lh < 512;
        let (mut bx, mut by) = (1u32, 1u32);
        if lit {
            let (pw, ph) = (lw + 2, lh + 2);
            if sx + pw > AW {
                sx = 0;
                sy += shelf;
                shelf = 0;
            }
            (bx, by) = (sx + 1, sy + 1);
            blocks.push((bx, by, lw, lh, lofs as usize));
            sx += pw;
            shelf = shelf.max(ph);
        }
        let (lmin_s, lmin_t) = (i32_at(faces, fo + 28) as f32, i32_at(faces, fo + 32) as f32);
        let lv = |k: usize| [f32_at(texinfo, ti * 72 + 32 + k * 16), f32_at(texinfo, ti * 72 + 32 + k * 16 + 4), f32_at(texinfo, ti * 72 + 32 + k * 16 + 8), f32_at(texinfo, ti * 72 + 32 + k * 16 + 12)];
        let (ls, lt) = (lv(0), lv(1));
        let poly: Vec<[f32; 3]> = (0..n)
            .map(|k| {
                let se = i32_at(surfedges, (first + k) * 4);
                let e = se.unsigned_abs() as usize;
                vert(u16_at(edges, e * 4 + if se < 0 { 2 } else { 0 }) as usize)
            })
            .collect();
        // one vertex: final position, flat position (texture/lightmap coordinates come from it), lightmap luxel override
        let mut mk = |p: [f32; 3], flat: [f32; 3], lm: Option<(f32, f32)>| {
            let dot = |v: [f32; 4]| flat[0] * v[0] + flat[1] * v[1] + flat[2] * v[2] + v[3];
            let uv2 = if lit {
                let (s, t) = lm.unwrap_or((dot(ls) - lmin_s, dot(lt) - lmin_t));
                let (s, t) = (s.clamp(0.0, (lw - 1) as f32), t.clamp(0.0, (lh - 1) as f32));
                [bx as f32 + s + 0.5, by as f32 + t + 0.5]
            } else {
                [1.5, 1.5]
            };
            m.vertices.push(Vertex { pos: p, normal: nrm, uv: [dot(sv) / tw, dot(tv) / th], uv2, nbones: 0, ..Default::default() });
        };
        if disp >= 0 {
            let di = disp as usize * 176;
            let start = [f32_at(dispinfo, di), f32_at(dispinfo, di + 4), f32_at(dispinfo, di + 8)];
            let vstart = i32_at(dispinfo, di + 12).max(0) as usize;
            let power = i32_at(dispinfo, di + 20).clamp(2, 4) as u32;
            let g = (1usize << power) + 1;
            if (vstart + g * g) * 20 > dispverts.len() {
                continue;
            }
            // rotate the quad so that corner 0 is the displacement's start point
            let d2 = |a: [f32; 3]| (a[0] - start[0]).powi(2) + (a[1] - start[1]).powi(2) + (a[2] - start[2]).powi(2);
            let c0 = (0..4).min_by(|&a, &b| d2(poly[a]).total_cmp(&d2(poly[b]))).unwrap_or(0);
            let q = |k: usize| poly[(c0 + k) % 4];
            let lerp = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
            for row in 0..g {
                for col in 0..g {
                    let (u, v) = (col as f32 / (g - 1) as f32, row as f32 / (g - 1) as f32);
                    let flat = lerp(lerp(q(0), q(1), u), lerp(q(3), q(2), u), v);
                    let o = (vstart + row * g + col) * 20;
                    let dist = f32_at(dispverts, o + 12);
                    let p = [flat[0] + f32_at(dispverts, o) * dist, flat[1] + f32_at(dispverts, o + 4) * dist, flat[2] + f32_at(dispverts, o + 8) * dist];
                    mk(p, flat, Some((u * (lw - 1) as f32, v * (lh - 1) as f32)));
                }
            }
            for row in 0..g as u32 - 1 {
                for col in 0..g as u32 - 1 {
                    let i = base + row * g as u32 + col;
                    let g = g as u32;
                    m.indices.extend_from_slice(&[i, i + 1, i + g + 1, i, i + g + 1, i + g]);
                }
            }
            continue;
        }
        for &p in &poly {
            mk(p, p, None);
        }
        for k in 1..n as u32 - 1 {
            m.indices.extend_from_slice(&[base, base + k, base + k + 1]);
        }
    }
    if blocks.is_empty() {
        for m in &mut meshes {
            for v in &mut m.vertices {
                v.uv2 = [0.5, 0.5];
            }
        }
        return Ok((meshes, None));
    }
    let ah = (sy + shelf).next_power_of_two().max(8);
    let mut rgba = vec![0u8; (AW * ah * 4) as usize];
    for y in 0..4 {
        for x in 0..4 {
            let o = ((y * AW + x) * 4) as usize;
            rgba[o..o + 4].copy_from_slice(&[128, 128, 128, 255]); // 1.0 after the x2 decode
        }
    }
    for &(bx, by, w, h, off) in &blocks {
        for y in 0..h {
            for x in 0..w {
                let s = off + ((y * w + x) * 4) as usize;
                let e = (lighting[s + 3] as i8) as i32;
                let k = 2f32.powi(e) / 255.0 / 2.0;
                let o = (((by + y) * AW + bx + x) * 4) as usize;
                for c in 0..3 {
                    rgba[o + c] = (lighting[s + c] as f32 * k * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                rgba[o + 3] = 255;
            }
        }
    }
    let (fw, fh) = (AW as f32, ah as f32);
    for m in &mut meshes {
        for v in &mut m.vertices {
            v.uv2 = [v.uv2[0] / fw, v.uv2[1] / fh];
        }
    }
    Ok((meshes, Some(Lightmap { width: AW, height: ah, rgba })))
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
    fn crossline_c4_static_props() {
        let Ok(data) = std::fs::read(r"E:\metrostroi2pz\game\content\maps\crossline_c4\map.bsp") else { return };
        let p = super::static_props(&data).unwrap();
        let mut names: Vec<&str> = p.iter().map(|x| x.model.as_str()).collect();
        names.sort();
        names.dedup();
        println!("{} static props, {} distinct models, e.g. {:?}", p.len(), names.len(), &names[..names.len().min(8)]);
        assert!(!p.is_empty());
    }

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

/// A static prop placed by the map compiler (game lump `sprp`).
#[derive(Debug, Clone)]
pub struct StaticProp {
    /// Model path as in the map, e.g. `models/props_c17/lamp.mdl`.
    pub model: String,
    pub origin: [f32; 3],
    /// Source angles: pitch, yaw, roll (degrees).
    pub angles: [f32; 3],
    pub skin: i32,
}

/// Static props from the game lump directory (lump 35).
pub fn static_props(data: &[u8]) -> Result<Vec<StaticProp>> {
    ensure!(data.len() > 1036 && &data[0..4] == b"VBSP", "not a VBSP file");
    let dir = Bsp(data).lump(35);
    let n = i32_at(dir, 0).max(0) as usize;
    for i in 0..n {
        let o = 4 + i * 16;
        if &dir[o..o + 4] != b"prps" && &dir[o..o + 4] != b"sprp" {
            continue;
        }
        // dgamelump_t: id(4) flags(2) version(2) fileofs(4) filelen(4)
        let (ofs, lump_len) = (i32_at(dir, o + 8) as usize, i32_at(dir, o + 12) as usize);
        if dir[o + 4] & 1 != 0 || ofs + lump_len > data.len() {
            return Ok(Vec::new()); // compressed or truncated: skip
        }
        let g = &data[ofs..ofs + lump_len];
        let dict_n = i32_at(g, 0).max(0) as usize;
        let mut names = Vec::with_capacity(dict_n);
        for k in 0..dict_n {
            let s = &g[4 + k * 128..4 + (k + 1) * 128];
            let end = s.iter().position(|&c| c == 0).unwrap_or(128);
            names.push(String::from_utf8_lossy(&s[..end]).to_string());
        }
        let mut p = 4 + dict_n * 128;
        let leaves = i32_at(g, p).max(0) as usize;
        p += 4 + leaves * 2;
        let count = i32_at(g, p).max(0) as usize;
        p += 4;
        if count == 0 {
            return Ok(Vec::new());
        }
        // entry size depends on the lump version; the entries fill the rest of the lump
        let stride = lump_len.saturating_sub(p) / count;
        ensure!(stride >= 56, "unexpected static prop stride {stride}");
        let mut out = Vec::with_capacity(count);
        for k in 0..count {
            let e = p + k * stride;
            let idx = u16_at(g, e + 24) as usize;
            let Some(model) = names.get(idx) else { continue };
            out.push(StaticProp {
                model: model.clone(),
                origin: [f32_at(g, e), f32_at(g, e + 4), f32_at(g, e + 8)],
                angles: [f32_at(g, e + 12), f32_at(g, e + 16), f32_at(g, e + 20)],
                skin: i32_at(g, e + 32),
            });
        }
        return Ok(out);
    }
    Ok(Vec::new())
}
