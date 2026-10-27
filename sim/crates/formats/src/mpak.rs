//! Metrostroi-rs asset package (`.mpak`): one memory-mapped file holding the game's assets under their
//! virtual paths, in the spirit of Source's VPK. Layout (little endian):
//!
//! ```text
//! "MPAK" u32 version  u32 count  u64 directory_offset
//! ... file data, each file 16-byte aligned ...
//! directory: count x { u16 path_len, path (utf-8, lower case, '/'), u64 offset, u64 size }
//! ```

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

const MAGIC: &[u8; 4] = b"MPAK";
const VERSION: u32 = 1;
const HEADER: usize = 20;

pub struct Mpak {
    map: memmap2::Mmap,
    pub entries: HashMap<String, (usize, usize)>,
}

impl Mpak {
    pub fn open(path: &Path) -> Result<Mpak> {
        let file = std::fs::File::open(path).with_context(|| path.display().to_string())?;
        let map = unsafe { memmap2::Mmap::map(&file)? };
        if map.len() < HEADER || &map[..4] != MAGIC {
            bail!("{}: not an mpak", path.display());
        }
        let u32_at = |o: usize| u32::from_le_bytes(map[o..o + 4].try_into().unwrap());
        let u64_at = |o: usize| u64::from_le_bytes(map[o..o + 8].try_into().unwrap()) as usize;
        if u32_at(4) != VERSION {
            bail!("{}: mpak version {}", path.display(), u32_at(4));
        }
        let (count, mut at) = (u32_at(8) as usize, u64_at(12));
        let mut entries = HashMap::with_capacity(count);
        for _ in 0..count {
            let len = u16::from_le_bytes(map.get(at..at + 2).context("truncated directory")?.try_into().unwrap()) as usize;
            let name = std::str::from_utf8(map.get(at + 2..at + 2 + len).context("truncated directory")?)?.to_string();
            at += 2 + len;
            let (off, size) = (u64_at(at), u64_at(at + 8));
            at += 16;
            if off + size > map.len() {
                bail!("{}: entry {name} runs past the end", path.display());
            }
            entries.insert(name, (off, size));
        }
        Ok(Mpak { map, entries })
    }

    pub fn read(&self, path: &str) -> Option<&[u8]> {
        let &(o, s) = self.entries.get(path)?;
        Some(&self.map[o..o + s])
    }
}

/// Write `files` (virtual path, bytes) as a package. Paths are normalised; a repeated path keeps the last data.
pub fn write(out: &Path, files: &[(String, Vec<u8>)]) -> Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).with_context(|| out.display().to_string())?);
    f.write_all(MAGIC)?;
    f.write_all(&VERSION.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?; // count, patched below
    f.write_all(&0u64.to_le_bytes())?; // directory offset, patched below
    let mut dir: Vec<(String, u64, u64)> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut pos = HEADER as u64;
    for (p, d) in files {
        let name = p.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase();
        if let Some(&i) = seen.get(&name) {
            dir[i].0.clear(); // superseded
        }
        let pad = (16 - pos % 16) % 16;
        f.write_all(&vec![0u8; pad as usize])?;
        pos += pad;
        f.write_all(d)?;
        seen.insert(name.clone(), dir.len());
        dir.push((name, pos, d.len() as u64));
        pos += d.len() as u64;
    }
    dir.retain(|e| !e.0.is_empty());
    for (name, off, size) in &dir {
        f.write_all(&(name.len() as u16).to_le_bytes())?;
        f.write_all(name.as_bytes())?;
        f.write_all(&off.to_le_bytes())?;
        f.write_all(&size.to_le_bytes())?;
    }
    f.flush()?;
    let mut file = f.into_inner()?;
    file.seek(SeekFrom::Start(8))?;
    file.write_all(&(dir.len() as u32).to_le_bytes())?;
    file.write_all(&pos.to_le_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let path = std::env::temp_dir().join("mpak_round_trip.mpak");
        let files = vec![
            ("Models\\A.mdl".to_string(), vec![1u8, 2, 3]),
            ("materials/b.vtf".to_string(), vec![9u8; 1000]),
            ("models/a.mdl".to_string(), vec![7u8, 7]),
        ];
        write(&path, &files).unwrap();
        let p = Mpak::open(&path).unwrap();
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.read("models/a.mdl"), Some(&[7u8, 7][..]));
        assert_eq!(p.read("materials/b.vtf").unwrap().len(), 1000);
        assert!(p.read("nope").is_none());
        let _ = std::fs::remove_file(path);
    }
}
