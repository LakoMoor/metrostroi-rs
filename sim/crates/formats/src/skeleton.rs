//! Source model skeleton: bones, pose parameters and the blend of sequence 0's animations.
//!
//! Only frame 0 of each animation is decoded: the cab's switches and gauge needles are
//! "pose parameter" models, i.e. a few single-pose animations blended by one parameter.

use anyhow::{ensure, Result};
use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub pos: Vec3,
    pub quat: Quat,
    pub rot: Vec3,
    pub pos_scale: Vec3,
    pub rot_scale: Vec3,
    pub pose_to_bone: Mat4,
}

#[derive(Clone, Debug)]
pub struct PoseParam {
    pub name: String,
    pub start: f32,
    pub end: f32,
}

#[derive(Clone, Copy, Debug)]
struct Xform {
    pos: Vec3,
    quat: Quat,
}

#[derive(Clone, Debug)]
struct Blend {
    param: i32,
    start: f32,
    end: f32,
    /// Frame-0 transform of every bone, one entry per blend point.
    poses: Vec<Vec<Xform>>,
}

#[derive(Clone, Debug)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
    pub pose_params: Vec<PoseParam>,
    blend: Option<Blend>,
}

struct Rd<'a>(&'a [u8]);

impl Rd<'_> {
    fn i32(&self, o: usize) -> Result<i32> {
        ensure!(o + 4 <= self.0.len(), "read past end at {o}");
        Ok(i32::from_le_bytes(self.0[o..o + 4].try_into().unwrap()))
    }
    fn i16(&self, o: usize) -> Result<i16> {
        ensure!(o + 2 <= self.0.len(), "read past end at {o}");
        Ok(i16::from_le_bytes(self.0[o..o + 2].try_into().unwrap()))
    }
    fn u16(&self, o: usize) -> Result<u16> {
        Ok(self.i16(o)? as u16)
    }
    fn u8(&self, o: usize) -> Result<u8> {
        ensure!(o < self.0.len(), "read past end at {o}");
        Ok(self.0[o])
    }
    fn f32(&self, o: usize) -> Result<f32> {
        Ok(f32::from_bits(self.i32(o)? as u32))
    }
    fn u64(&self, o: usize) -> Result<u64> {
        ensure!(o + 8 <= self.0.len(), "read past end at {o}");
        Ok(u64::from_le_bytes(self.0[o..o + 8].try_into().unwrap()))
    }
    fn vec3(&self, o: usize) -> Result<Vec3> {
        Ok(Vec3::new(self.f32(o)?, self.f32(o + 4)?, self.f32(o + 8)?))
    }
    fn cstr(&self, o: usize) -> String {
        let n = self.0.get(o..).and_then(|s| s.iter().position(|&c| c == 0)).unwrap_or(0);
        String::from_utf8_lossy(&self.0[o..o + n]).into_owned()
    }
}

fn half(h: u16) -> f32 {
    let (s, e, m) = ((h >> 15) & 1, (h >> 10) & 31, (h & 1023) as f32);
    let v = match e {
        0 => m * 2f32.powi(-24),
        31 => f32::INFINITY,
        _ => (1.0 + m / 1024.0) * 2f32.powi(e as i32 - 15),
    };
    if s == 1 {
        -v
    } else {
        v
    }
}

fn quat48(r: &Rd, o: usize) -> Result<Quat> {
    let (x, y, z) = (r.u16(o)?, r.u16(o + 2)?, r.u16(o + 4)?);
    let fx = (x as f32 - 32768.0) / 32768.0;
    let fy = (y as f32 - 32768.0) / 32768.0;
    let fz = ((z & 0x7fff) as f32 - 16384.0) / 16384.0;
    let mut w = (1.0 - fx * fx - fy * fy - fz * fz).max(0.0).sqrt();
    if z & 0x8000 != 0 {
        w = -w;
    }
    Ok(Quat::from_xyzw(fx, fy, fz, w))
}

fn quat64(r: &Rd, o: usize) -> Result<Quat> {
    let v = r.u64(o)?;
    let c = |n: u64| (n as f32 - 1048576.0) / 1048576.5;
    let (x, y, z) = (c(v & 0x1f_ffff), c((v >> 21) & 0x1f_ffff), c((v >> 42) & 0x1f_ffff));
    let mut w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    if v >> 63 != 0 {
        w = -w;
    }
    Ok(Quat::from_xyzw(x, y, z, w))
}

/// Source's `AngleQuaternion(RadianEuler)`: x = roll, y = pitch, z = yaw.
fn euler_quat(e: Vec3) -> Quat {
    let (sr, cr) = (e.x * 0.5).sin_cos();
    let (sp, cp) = (e.y * 0.5).sin_cos();
    let (sy, cy) = (e.z * 0.5).sin_cos();
    Quat::from_xyzw(
        sr * cp * cy - cr * sp * sy,
        cr * sp * cy + sr * cp * sy,
        cr * cp * sy - sr * sp * cy,
        cr * cp * cy + sr * sp * sy,
    )
}

/// One RLE track (`mstudioanimvalue_t`) sampled at `frame`.
fn rle(r: &Rd, base: usize, frame: usize) -> Result<f32> {
    let mut p = base;
    let mut k = frame;
    loop {
        let (valid, total) = (r.u8(p)? as usize, r.u8(p + 1)? as usize);
        ensure!(total > 0, "bad animation RLE");
        if total > k {
            let idx = if valid > k { k + 1 } else { valid };
            return Ok(r.i16(p + idx * 2)? as f32);
        }
        k -= total;
        p += (valid + 1) * 2;
    }
}

fn decode_anim(r: &Rd, desc: usize, bones: &[Bone]) -> Result<Vec<Xform>> {
    let mut out: Vec<Xform> = bones.iter().map(|b| Xform { pos: b.pos, quat: b.quat }).collect();
    let animindex = r.i32(desc + 56)? as usize;
    if animindex == 0 {
        return Ok(out);
    }
    let mut p = desc + animindex;
    loop {
        let (bone, flags, next) = (r.u8(p)? as usize, r.u8(p + 1)?, r.i16(p + 2)?);
        if bone == 255 {
            break;
        }
        ensure!(bone < bones.len(), "animation bone {bone} out of range");
        let b = &bones[bone];
        let mut d = p + 4;
        let (mut quat, mut pos) = (b.quat, b.pos);
        if flags & 0x08 != 0 {
            // ANIMROT: three RLE offsets
            let mut e = Vec3::ZERO;
            for a in 0..3 {
                let off = r.i16(d + a * 2)?;
                let v = if off != 0 { rle(r, d + off as usize, 0)? } else { 0.0 };
                e[a] = b.rot[a] + v * b.rot_scale[a];
            }
            quat = euler_quat(e);
            d += 6;
        } else if flags & 0x02 != 0 {
            quat = quat48(r, d)?;
            d += 6;
        } else if flags & 0x20 != 0 {
            quat = quat64(r, d)?;
            d += 8;
        }
        if flags & 0x04 != 0 {
            let mut v = Vec3::ZERO;
            for a in 0..3 {
                let off = r.i16(d + a * 2)?;
                let x = if off != 0 { rle(r, d + off as usize, 0)? } else { 0.0 };
                v[a] = b.pos[a] + x * b.pos_scale[a];
            }
            pos = v;
        } else if flags & 0x01 != 0 {
            pos = Vec3::new(half(r.u16(d)?), half(r.u16(d + 2)?), half(r.u16(d + 4)?));
        }
        out[bone] = Xform { pos, quat };
        if next == 0 {
            break;
        }
        p = (p as isize + next as isize) as usize;
    }
    Ok(out)
}

impl Skeleton {
    /// `None` when the model has no bones.
    pub fn parse(mdl: &[u8]) -> Result<Option<Self>> {
        let r = Rd(mdl);
        let nb = r.i32(156)? as usize;
        if nb == 0 {
            return Ok(None);
        }
        let boff = r.i32(160)? as usize;
        let mut bones = Vec::with_capacity(nb);
        for i in 0..nb {
            let o = boff + i * 216;
            let name = r.cstr((o as i32 + r.i32(o)?) as usize);
            let m = |k: usize| -> Result<f32> { r.f32(o + 96 + k * 4) };
            let mut c = [0f32; 12];
            for (k, v) in c.iter_mut().enumerate() {
                *v = m(k)?;
            }
            // row-major 3x4 -> column-major Mat4
            let pose_to_bone = Mat4::from_cols_array(&[
                c[0], c[4], c[8], 0.0, c[1], c[5], c[9], 0.0, c[2], c[6], c[10], 0.0, c[3], c[7], c[11], 1.0,
            ]);
            bones.push(Bone {
                name,
                parent: r.i32(o + 4)?,
                pos: r.vec3(o + 32)?,
                quat: Quat::from_xyzw(r.f32(o + 44)?, r.f32(o + 48)?, r.f32(o + 52)?, r.f32(o + 56)?),
                rot: r.vec3(o + 60)?,
                pos_scale: r.vec3(o + 72)?,
                rot_scale: r.vec3(o + 84)?,
                pose_to_bone,
            });
        }

        let (npp, ppoff) = (r.i32(300)? as usize, r.i32(304)? as usize);
        let mut pose_params = Vec::with_capacity(npp);
        for i in 0..npp {
            let o = ppoff + i * 20;
            pose_params.push(PoseParam {
                name: r.cstr((o as i32 + r.i32(o)?) as usize),
                start: r.f32(o + 8)?,
                end: r.f32(o + 12)?,
            });
        }

        let (nseq, seqoff) = (r.i32(188)? as usize, r.i32(192)? as usize);
        let animoff = r.i32(184)? as usize;
        let mut blend = None;
        if nseq > 0 {
            let s = seqoff;
            let numblends = (r.i32(s + 56)? as usize).max(1);
            let aii = s + r.i32(s + 60)? as usize;
            let mut poses = Vec::with_capacity(numblends);
            for k in 0..numblends {
                let ai = r.i16(aii + k * 2)? as usize;
                poses.push(decode_anim(&r, animoff + ai * 100, &bones)?);
            }
            blend = Some(Blend { param: r.i32(s + 76)?, start: r.f32(s + 84)?, end: r.f32(s + 92)?, poses });
        }
        Ok(Some(Skeleton { bones, pose_params, blend }))
    }

    pub fn param_index(&self, name: &str) -> Option<usize> {
        self.pose_params.iter().position(|p| p.name == name)
    }

    /// Skinning matrices (model space -> posed model space) for the given raw pose-parameter values.
    pub fn skin_matrices(&self, params: &[f32]) -> Vec<Mat4> {
        let xf: Vec<Xform> = match &self.blend {
            None => self.bones.iter().map(|b| Xform { pos: b.pos, quat: b.quat }).collect(),
            Some(bl) => {
                let n = bl.poses.len();
                let (i, f) = if n > 1 && bl.param >= 0 {
                    let v = params.get(bl.param as usize).copied().unwrap_or(0.0);
                    let span = bl.end - bl.start;
                    let t = if span.abs() > 1e-6 { ((v - bl.start) / span).clamp(0.0, 1.0) } else { 0.0 };
                    let x = t * (n - 1) as f32;
                    let i = (x.floor() as usize).min(n - 2);
                    (i, x - i as f32)
                } else {
                    (0, 0.0)
                };
                let (a, b) = (&bl.poses[i], &bl.poses[(i + 1).min(n - 1)]);
                a.iter().zip(b).map(|(a, b)| Xform { pos: a.pos.lerp(b.pos, f), quat: a.quat.slerp(b.quat, f) }).collect()
            }
        };
        let mut world: Vec<Mat4> = Vec::with_capacity(xf.len());
        for (i, x) in xf.iter().enumerate() {
            let local = Mat4::from_rotation_translation(x.quat, x.pos);
            let m = match self.bones[i].parent {
                p if p >= 0 && (p as usize) < i => world[p as usize] * local,
                _ => local,
            };
            world.push(m);
        }
        world.iter().zip(&self.bones).map(|(w, b)| *w * b.pose_to_bone).collect()
    }
}
