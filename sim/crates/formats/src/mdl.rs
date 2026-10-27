//! Source engine static models: `.mdl` (v44-49) + `.vvd` + `.dx90.vtx` (LOD 0).
//!
//! Geometry stays in Source space (Z up, inches); the renderer converts.

use anyhow::{bail, ensure, Result};

#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Lightmap atlas coordinates (map faces only).
    pub uv2: [f32; 2],
    /// Up to three bone influences (`weights` sum to 1); `nbones` of them are used.
    pub joints: [u8; 3],
    pub weights: [f32; 3],
    pub nbones: u8,
}

#[derive(Debug)]
pub struct Mesh {
    /// Texture name from the model's texture table (skin family 0 applied).
    pub material: String,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug)]
pub struct SubModel {
    pub meshes: Vec<Mesh>,
}

#[derive(Debug)]
pub struct BodyPart {
    pub name: String,
    pub models: Vec<SubModel>,
}

#[derive(Debug)]
pub struct Model {
    pub version: i32,
    /// Material search directories (`cdtexture`), with trailing slash.
    pub cd_textures: Vec<String>,
    pub body_parts: Vec<BodyPart>,
    pub skeleton: Option<crate::skeleton::Skeleton>,
}

struct Rd<'a>(&'a [u8]);

impl<'a> Rd<'a> {
    fn i32(&self, o: usize) -> Result<i32> {
        ensure!(o + 4 <= self.0.len(), "read past end at {o}");
        Ok(i32::from_le_bytes(self.0[o..o + 4].try_into().unwrap()))
    }
    fn u16(&self, o: usize) -> Result<u16> {
        ensure!(o + 2 <= self.0.len(), "read past end at {o}");
        Ok(u16::from_le_bytes(self.0[o..o + 2].try_into().unwrap()))
    }
    fn i16(&self, o: usize) -> Result<i16> {
        Ok(self.u16(o)? as i16)
    }
    fn f32(&self, o: usize) -> Result<f32> {
        ensure!(o + 4 <= self.0.len(), "read past end at {o}");
        Ok(f32::from_le_bytes(self.0[o..o + 4].try_into().unwrap()))
    }
    fn cstr(&self, o: usize) -> Result<String> {
        ensure!(o < self.0.len(), "string offset out of range");
        let n = self.0[o..].iter().position(|&c| c == 0).unwrap_or(self.0.len() - o);
        Ok(String::from_utf8_lossy(&self.0[o..o + n]).into_owned())
    }
}

fn load_vvd(b: &[u8]) -> Result<Vec<Vertex>> {
    let r = Rd(b);
    ensure!(b.len() >= 64 && &b[..4] == b"IDSV", "bad VVD header");
    ensure!(r.i32(4)? == 4, "unsupported VVD version");
    let lod0 = r.i32(16)? as usize;
    let nfix = r.i32(48)? as usize;
    let fixoff = r.i32(52)? as usize;
    let vstart = r.i32(56)? as usize;
    let tstart = r.i32(60)? as usize;
    let count = if nfix > 0 { (tstart - vstart) / 48 } else { lod0 };
    let mut raw = Vec::with_capacity(count);
    for i in 0..count {
        let b0 = vstart + i * 48;
        let o = b0 + 16;
        raw.push(Vertex {
            weights: [r.f32(b0)?, r.f32(b0 + 4)?, r.f32(b0 + 8)?],
            joints: [r.0[b0 + 12], r.0[b0 + 13], r.0[b0 + 14]],
            nbones: r.0[b0 + 15],
            pos: [r.f32(o)?, r.f32(o + 4)?, r.f32(o + 8)?],
            normal: [r.f32(o + 12)?, r.f32(o + 16)?, r.f32(o + 20)?],
            uv: [r.f32(o + 24)?, r.f32(o + 28)?],
            uv2: [0.0, 0.0],
        });
    }
    if nfix == 0 {
        return Ok(raw);
    }
    let mut out = Vec::new();
    for i in 0..nfix {
        let o = fixoff + i * 12;
        let (lod, src, n) = (r.i32(o)?, r.i32(o + 4)? as usize, r.i32(o + 8)? as usize);
        if lod >= 0 {
            ensure!(src + n <= raw.len(), "VVD fixup out of range");
            out.extend_from_slice(&raw[src..src + n]);
        }
    }
    Ok(out)
}

/// Triangles (as indices into the mesh's own vertex list `origMeshVertID`) per
/// `[bodypart][model][mesh]`, LOD 0.
fn load_vtx(b: &[u8], mdl_version: i32) -> Result<Vec<Vec<Vec<Vec<[u16; 3]>>>>> {
    let r = Rd(b);
    ensure!(r.i32(0)? == 7, "unsupported VTX version");
    let sg_size = if mdl_version < 49 { 25 } else { 33 };
    let nbp = r.i32(28)? as usize;
    let bpoff = r.i32(32)? as usize;
    let mut res = Vec::new();
    for bp in 0..nbp {
        let bph = bpoff + bp * 8;
        let (nm, mo) = (r.i32(bph)? as usize, r.i32(bph + 4)? as usize);
        let mut models = Vec::new();
        for m in 0..nm {
            let mh = bph + mo + m * 8;
            let (nl, lo) = (r.i32(mh)?, r.i32(mh + 4)? as usize);
            let mut meshes = Vec::new();
            if nl >= 1 {
                let lh = mh + lo;
                let (nmesh, meshoff) = (r.i32(lh)? as usize, r.i32(lh + 4)? as usize);
                for k in 0..nmesh {
                    let msh = lh + meshoff + k * 9;
                    let (nsg, sgo) = (r.i32(msh)? as usize, r.i32(msh + 4)? as usize);
                    let mut tris: Vec<[u16; 3]> = Vec::new();
                    for g in 0..nsg {
                        let sg = msh + sgo + g * sg_size;
                        let nv = r.i32(sg)? as usize;
                        let vo = r.i32(sg + 4)? as usize;
                        let ni = r.i32(sg + 8)? as usize;
                        let io = r.i32(sg + 12)? as usize;
                        let ns = r.i32(sg + 16)? as usize;
                        let so = r.i32(sg + 20)? as usize;
                        let mut verts = Vec::with_capacity(nv);
                        for i in 0..nv {
                            verts.push(r.u16(sg + vo + i * 9 + 4)?);
                        }
                        let mut idx = Vec::with_capacity(ni);
                        for i in 0..ni {
                            idx.push(r.u16(sg + io + i * 2)? as usize);
                        }
                        for s in 0..ns {
                            let st = sg + so + s * 27;
                            let sni = r.i32(st)? as usize;
                            let sio = r.i32(st + 4)? as usize;
                            let flags = *b.get(st + 18).unwrap_or(&0);
                            ensure!(sio + sni <= idx.len(), "strip out of range");
                            let seg = &idx[sio..sio + sni];
                            let mut push = |a: usize, b2: usize, c: usize| -> Result<()> {
                                ensure!(a < verts.len() && b2 < verts.len() && c < verts.len(), "VTX index out of range");
                                tris.push([verts[a], verts[b2], verts[c]]);
                                Ok(())
                            };
                            if flags & 1 != 0 {
                                for t in seg.chunks_exact(3) {
                                    push(t[0], t[1], t[2])?;
                                }
                            } else {
                                for i in 0..seg.len().saturating_sub(2) {
                                    let t = if i % 2 == 0 {
                                        [seg[i], seg[i + 1], seg[i + 2]]
                                    } else {
                                        [seg[i + 1], seg[i], seg[i + 2]]
                                    };
                                    if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
                                        push(t[0], t[1], t[2])?;
                                    }
                                }
                            }
                        }
                    }
                    meshes.push(tris);
                }
            }
            models.push(meshes);
        }
        res.push(models);
    }
    Ok(res)
}

pub fn load(mdl: &[u8], vvd: &[u8], vtx: &[u8]) -> Result<Model> {
    let r = Rd(mdl);
    ensure!(mdl.len() > 240 && &mdl[..4] == b"IDST", "bad MDL header");
    let version = r.i32(4)?;
    if !(44..=49).contains(&version) {
        bail!("unsupported MDL version {version}");
    }
    let ntex = r.i32(204)? as usize;
    let texoff = r.i32(208)? as usize;
    let ncd = r.i32(212)? as usize;
    let cdoff = r.i32(216)? as usize;
    let nref = r.i32(220)? as usize;
    let skinoff = r.i32(228)? as usize;
    let nbp = r.i32(232)? as usize;
    let bpoff = r.i32(236)? as usize;

    let mut textures = Vec::with_capacity(ntex);
    for i in 0..ntex {
        let base = texoff + i * 64;
        textures.push(r.cstr(base + r.i32(base)? as usize)?);
    }
    let mut cd_textures = Vec::with_capacity(ncd);
    for i in 0..ncd {
        let mut s = r.cstr(r.i32(cdoff + i * 4)? as usize)?.replace('\\', "/");
        s = s.trim_start_matches('/').to_string();
        if !s.is_empty() && !s.ends_with('/') {
            s.push('/');
        }
        cd_textures.push(s);
    }
    let mut skin0 = Vec::with_capacity(nref);
    for i in 0..nref {
        skin0.push(r.i16(skinoff + i * 2)? as usize);
    }

    let verts = load_vvd(vvd)?;
    let strips = load_vtx(vtx, version)?;

    let mut body_parts = Vec::with_capacity(nbp);
    for bp in 0..nbp {
        let bph = bpoff + bp * 16;
        let name = r.cstr(bph + r.i32(bph)? as usize)?;
        let nmodels = r.i32(bph + 4)? as usize;
        let modelidx = r.i32(bph + 12)? as usize;
        let mut models = Vec::with_capacity(nmodels);
        for m in 0..nmodels {
            let mo = bph + modelidx + m * 148;
            let nmesh = r.i32(mo + 72)? as usize;
            let meshoff = r.i32(mo + 76)? as usize;
            let nverts = r.i32(mo + 80)?;
            let vertidx = r.i32(mo + 84)? as usize;
            let mut meshes = Vec::new();
            let tris_per_mesh = strips.get(bp).and_then(|x| x.get(m));
            if let (Some(tris_per_mesh), true) = (tris_per_mesh, nverts > 0) {
                let mstart = vertidx / 48;
                for k in 0..nmesh.min(tris_per_mesh.len()) {
                    let tris = &tris_per_mesh[k];
                    if tris.is_empty() {
                        continue;
                    }
                    let mh = mo + meshoff + k * 116;
                    let matidx = r.i32(mh)? as usize;
                    let voff = r.i32(mh + 12)? as usize;
                    let ti = skin0.get(matidx).copied().unwrap_or(matidx);
                    let material = textures.get(ti).cloned().unwrap_or_default();

                    let mut remap = std::collections::HashMap::<u16, u32>::new();
                    let mut vertices = Vec::new();
                    let mut indices = Vec::with_capacity(tris.len() * 3);
                    for t in tris {
                        // Source winds clockwise; reverse for counter-clockwise front faces.
                        for &v in &[t[2], t[1], t[0]] {
                            let ix = *remap.entry(v).or_insert_with(|| {
                                let gv = mstart + voff + v as usize;
                                vertices.push(verts.get(gv).copied().unwrap_or_default());
                                (vertices.len() - 1) as u32
                            });
                            indices.push(ix);
                        }
                    }
                    meshes.push(Mesh { material, vertices, indices });
                }
            }
            models.push(SubModel { meshes });
        }
        body_parts.push(BodyPart { name, models });
    }
    let skeleton = crate::skeleton::Skeleton::parse(mdl).unwrap_or_else(|e| {
        eprintln!("skeleton: {e}");
        None
    });
    Ok(Model { version, cd_textures, body_parts, skeleton })
}
