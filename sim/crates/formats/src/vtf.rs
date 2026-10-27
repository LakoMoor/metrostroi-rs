//! Valve Texture Format (`.vtf`) decoder: 2D textures, all mip levels, frame 0, to RGBA8.

use anyhow::{bail, ensure, Result};

pub struct Vtf {
    pub width: u32,
    pub height: u32,
    /// RGBA8 pixels, mip 0 (full size) first.
    pub mips: Vec<Vec<u8>>,
    pub has_alpha: bool,
}

fn le16(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes([b[o], b[o + 1]]) as usize
}
fn le32(b: &[u8], o: usize) -> usize {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize
}

fn mip_size(fmt: i32, w: usize, h: usize) -> Result<usize> {
    let blocks = |bw: usize| ((w + 3) / 4).max(1) * ((h + 3) / 4).max(1) * bw;
    Ok(match fmt {
        0 | 1 | 11 | 12 | 16 => w * h * 4,
        2 | 3 | 9 | 10 => w * h * 3,
        4 | 17 | 6 | 18 | 19 | 21 | 22 => w * h * 2,
        5 | 8 => w * h,
        13 | 20 => blocks(8),
        14 | 15 => blocks(16),
        _ => bail!("unsupported VTF image format {fmt}"),
    })
}

fn rgb565(v: u16) -> [u8; 3] {
    let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
    [((r * 255 + 15) / 31) as u8, ((g * 255 + 31) / 63) as u8, ((b * 255 + 15) / 31) as u8]
}

/// `always_four`: DXT3/5 colour blocks never use the 3-colour mode.
fn dxt_color_block(b: &[u8], one_bit_alpha: bool, always_four: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || always_four {
        for i in 0..3 {
            pal[2][i] = ((2 * p0[i] as u16 + p1[i] as u16) / 3) as u8;
            pal[3][i] = ((p0[i] as u16 + 2 * p1[i] as u16) / 3) as u8;
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for i in 0..3 {
            pal[2][i] = ((p0[i] as u16 + p1[i] as u16) / 2) as u8;
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, if one_bit_alpha { 0 } else { 255 }];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let mut out = [[0u8; 4]; 16];
    for (i, px) in out.iter_mut().enumerate() {
        *px = pal[((bits >> (2 * i)) & 3) as usize];
    }
    out
}

fn decode_dxt(fmt: i32, src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let bw = (w + 3) / 4;
    let bh = (h + 3) / 4;
    let stride = if fmt == 13 || fmt == 20 { 8 } else { 16 };
    for by in 0..bh {
        for bx in 0..bw {
            let blk = &src[(by * bw + bx) * stride..][..stride];
            let colors = match fmt {
                13 => dxt_color_block(blk, false, false),
                20 => dxt_color_block(blk, true, false),
                _ => dxt_color_block(&blk[8..], false, true),
            };
            let mut alpha = [255u8; 16];
            match fmt {
                14 => {
                    for i in 0..16 {
                        let a = (blk[i / 2] >> (4 * (i % 2))) & 15;
                        alpha[i] = a * 17;
                    }
                }
                15 => {
                    let (a0, a1) = (blk[0] as u16, blk[1] as u16);
                    let mut pal = [0u8; 8];
                    pal[0] = a0 as u8;
                    pal[1] = a1 as u8;
                    if a0 > a1 {
                        for i in 1..7 {
                            pal[i + 1] = (((7 - i as u16) * a0 + i as u16 * a1) / 7) as u8;
                        }
                    } else {
                        for i in 1..5 {
                            pal[i + 1] = (((5 - i as u16) * a0 + i as u16 * a1) / 5) as u8;
                        }
                        pal[6] = 0;
                        pal[7] = 255;
                    }
                    let mut bits = 0u64;
                    for i in 0..6 {
                        bits |= (blk[2 + i] as u64) << (8 * i);
                    }
                    for i in 0..16 {
                        alpha[i] = pal[((bits >> (3 * i)) & 7) as usize];
                    }
                }
                _ => {}
            }
            for py in 0..4 {
                for px in 0..4 {
                    let (x, y) = (bx * 4 + px, by * 4 + py);
                    if x < w && y < h {
                        let c = colors[py * 4 + px];
                        let o = (y * w + x) * 4;
                        let a = if fmt == 13 || fmt == 20 { c[3] } else { alpha[py * 4 + px] };
                        out[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], a]);
                    }
                }
            }
        }
    }
    out
}

fn to_rgba(fmt: i32, src: &[u8], w: usize, h: usize) -> Result<Vec<u8>> {
    let n = w * h;
    let mut out = vec![255u8; n * 4];
    match fmt {
        13 | 14 | 15 | 20 => return Ok(decode_dxt(fmt, src, w, h)),
        0 => out.copy_from_slice(&src[..n * 4]),
        1 => (0..n).for_each(|i| {
            let s = &src[i * 4..];
            out[i * 4..i * 4 + 4].copy_from_slice(&[s[3], s[2], s[1], s[0]]);
        }),
        11 => (0..n).for_each(|i| {
            let s = &src[i * 4..];
            out[i * 4..i * 4 + 4].copy_from_slice(&[s[1], s[2], s[3], s[0]]);
        }),
        12 => (0..n).for_each(|i| {
            let s = &src[i * 4..];
            out[i * 4..i * 4 + 4].copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }),
        16 => (0..n).for_each(|i| {
            let s = &src[i * 4..];
            out[i * 4..i * 4 + 4].copy_from_slice(&[s[2], s[1], s[0], 255]);
        }),
        2 | 9 => (0..n).for_each(|i| out[i * 4..i * 4 + 3].copy_from_slice(&src[i * 3..i * 3 + 3])),
        3 | 10 => (0..n).for_each(|i| {
            let s = &src[i * 3..];
            out[i * 4..i * 4 + 3].copy_from_slice(&[s[2], s[1], s[0]]);
        }),
        4 | 17 => (0..n).for_each(|i| {
            let v = u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]);
            let c = if fmt == 4 { rgb565(v) } else { let c = rgb565(v); [c[2], c[1], c[0]] };
            out[i * 4..i * 4 + 3].copy_from_slice(&c);
        }),
        5 => (0..n).for_each(|i| out[i * 4..i * 4 + 3].fill(src[i])),
        8 => (0..n).for_each(|i| {
            out[i * 4..i * 4 + 3].fill(255);
            out[i * 4 + 3] = src[i];
        }),
        6 => (0..n).for_each(|i| {
            out[i * 4..i * 4 + 3].fill(src[i * 2]);
            out[i * 4 + 3] = src[i * 2 + 1];
        }),
        _ => bail!("unsupported VTF image format {fmt}"),
    }
    Ok(out)
}

pub fn decode(b: &[u8]) -> Result<Vtf> {
    ensure!(b.len() > 64 && &b[..4] == b"VTF\0", "not a VTF");
    let minor = le32(b, 8);
    let header = le32(b, 12);
    let (w, h) = (le16(b, 16), le16(b, 18));
    let flags = le32(b, 20);
    let frames = le16(b, 24).max(1);
    let fmt = le32(b, 52) as i32;
    let mip_count = b[56] as usize;
    let low_fmt = le32(b, 57) as i32;
    let (lw, lh) = (b[61] as usize, b[62] as usize);
    ensure!(w > 0 && h > 0 && mip_count > 0, "empty VTF");
    let faces = if flags & 0x4000 != 0 { if minor >= 5 { 7 } else { 6 } } else { 1 };
    let depth = if minor >= 2 { le16(b, 63).max(1) } else { 1 };

    let mut off = header;
    if low_fmt >= 0 && lw > 0 && lh > 0 {
        off += mip_size(low_fmt, lw, lh)?;
    }
    let dims = |m: usize| ((w >> m).max(1), (h >> m).max(1));
    // Data is stored smallest mip first; each mip holds frames x faces x depth images.
    let mut mip_off = vec![0usize; mip_count];
    let mut cur = off;
    for m in (0..mip_count).rev() {
        let (mw, mh) = dims(m);
        let sz = mip_size(fmt, mw, mh)?;
        mip_off[m] = cur;
        cur += sz * frames * faces * depth;
    }
    ensure!(cur <= b.len(), "VTF data truncated");

    let mut mips = Vec::with_capacity(mip_count);
    for m in 0..mip_count {
        let (mw, mh) = dims(m);
        let sz = mip_size(fmt, mw, mh)?;
        mips.push(to_rgba(fmt, &b[mip_off[m]..mip_off[m] + sz], mw, mh)?);
    }
    let has_alpha = mips[0].chunks_exact(4).any(|p| p[3] != 255);
    Ok(Vtf { width: w as u32, height: h as u32, mips, has_alpha })
}
