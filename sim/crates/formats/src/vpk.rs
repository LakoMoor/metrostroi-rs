//! Valve package (`*_dir.vpk` + numbered archives), versions 1 and 2. Only the directory is parsed; file data is mapped.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;

pub struct Entry {
    pub archive: u16,
    pub offset: u32,
    pub length: u32,
    /// Bytes stored in the directory itself, before the archive data.
    pub preload: Vec<u8>,
}

pub struct Vpk {
    /// Mapped archives by index; `dir` holds the embedded data of entries with archive 0x7fff.
    archives: HashMap<u16, memmap2::Mmap>,
    dir: memmap2::Mmap,
    data_start: usize,
    pub entries: HashMap<String, Entry>,
}

fn cstr(b: &[u8], o: &mut usize) -> Result<String> {
    let end = b[*o..].iter().position(|&c| c == 0).context("unterminated string in vpk tree")? + *o;
    let s = String::from_utf8_lossy(&b[*o..end]).into_owned();
    *o = end + 1;
    Ok(s)
}

impl Vpk {
    /// `dir_file` is the path of `<name>_dir.vpk`.
    pub fn open(dir_file: &Path) -> Result<Vpk> {
        let f = std::fs::File::open(dir_file).with_context(|| dir_file.display().to_string())?;
        let dir = unsafe { memmap2::Mmap::map(&f)? };
        let u32_at = |o: usize| u32::from_le_bytes(dir[o..o + 4].try_into().unwrap());
        if dir.len() < 12 || u32_at(0) != 0x55aa1234 {
            bail!("{} is not a VPK", dir_file.display());
        }
        let (header, tree) = match u32_at(4) {
            1 => (12usize, u32_at(8) as usize),
            2 => (28usize, u32_at(8) as usize),
            v => bail!("unsupported VPK version {v}"),
        };
        let mut o = header;
        let end = header + tree;
        let mut entries = HashMap::new();
        loop {
            let ext = cstr(&dir, &mut o)?;
            if ext.is_empty() {
                break;
            }
            loop {
                let path = cstr(&dir, &mut o)?;
                if path.is_empty() {
                    break;
                }
                loop {
                    let name = cstr(&dir, &mut o)?;
                    if name.is_empty() {
                        break;
                    }
                    let preload_len = u16::from_le_bytes(dir[o + 4..o + 6].try_into().unwrap()) as usize;
                    let archive = u16::from_le_bytes(dir[o + 6..o + 8].try_into().unwrap());
                    let offset = u32::from_le_bytes(dir[o + 8..o + 12].try_into().unwrap());
                    let length = u32::from_le_bytes(dir[o + 12..o + 16].try_into().unwrap());
                    o += 18;
                    let preload = dir[o..o + preload_len].to_vec();
                    o += preload_len;
                    let full = if path == " " { format!("{name}.{ext}") } else { format!("{path}/{name}.{ext}") };
                    entries.insert(full.to_ascii_lowercase(), Entry { archive, offset, length, preload });
                }
            }
        }
        // numbered archives sit next to the dir file: <name>_000.vpk
        let stem = dir_file.file_name().and_then(|n| n.to_str()).unwrap_or("").trim_end_matches("_dir.vpk").to_string();
        let mut archives = HashMap::new();
        let mut used: Vec<u16> = entries.values().map(|e| e.archive).filter(|&a| a != 0x7fff).collect();
        used.sort_unstable();
        used.dedup();
        for a in used {
            let p = dir_file.with_file_name(format!("{stem}_{a:03}.vpk"));
            if let Ok(f) = std::fs::File::open(&p) {
                archives.insert(a, unsafe { memmap2::Mmap::map(&f)? });
            }
        }
        Ok(Vpk { archives, dir, data_start: end, entries })
    }

    /// The archive part of an entry (without preload bytes), if it is available.
    pub fn data(&self, e: &Entry) -> Option<&[u8]> {
        let (o, l) = (e.offset as usize, e.length as usize);
        let src: (&[u8], usize) = if e.archive == 0x7fff { (&self.dir, self.data_start + o) } else { (self.archives.get(&e.archive)?, o) };
        src.0.get(src.1..src.1 + l)
    }
}
