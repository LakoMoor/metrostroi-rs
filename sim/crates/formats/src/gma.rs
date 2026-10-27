//! Garry's Mod addon archives (`.gma`, and Workshop `*_legacy.bin` = LZMA-compressed GMA).

use anyhow::{bail, Context, Result};
use std::path::Path;

enum Data {
    Mmap(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl Data {
    fn bytes(&self) -> &[u8] {
        match self {
            Data::Mmap(m) => m,
            Data::Owned(v) => v,
        }
    }
}

pub struct Entry {
    /// Lower-case, forward-slash path inside the archive.
    pub path: String,
    pub size: usize,
    offset: usize,
}

pub struct Gma {
    data: Data,
    pub title: String,
    pub entries: Vec<Entry>,
}

struct Cursor<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.o + n > self.b.len() {
            bail!("unexpected end of GMA header");
        }
        let s = &self.b[self.o..self.o + n];
        self.o += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn cstr(&mut self) -> Result<String> {
        let rest = &self.b[self.o..];
        let n = rest.iter().position(|&c| c == 0).context("unterminated string")?;
        let s = String::from_utf8_lossy(&rest[..n]).into_owned();
        self.o += n + 1;
        Ok(s)
    }
}

impl Gma {
    pub fn open(path: &Path) -> Result<Gma> {
        let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        // SAFETY: the archive is only read; a concurrent writer would be a user error.
        let map = unsafe { memmap2::Mmap::map(&file)? };
        let data = if map.starts_with(b"GMAD") {
            Data::Mmap(map)
        } else {
            let mut out = Vec::new();
            lzma_rs::lzma_decompress(&mut std::io::Cursor::new(&map[..]), &mut out)
                .map_err(|e| anyhow::anyhow!("lzma: {e:?}"))?;
            if !out.starts_with(b"GMAD") {
                bail!("{} is not a GMA archive", path.display());
            }
            Data::Owned(out)
        };
        Self::parse(data)
    }

    fn parse(data: Data) -> Result<Gma> {
        let b = data.bytes();
        let mut c = Cursor { b, o: 4 };
        let version = c.u8()?;
        c.take(8 + 8)?; // steamid, timestamp
        if version > 1 {
            while !c.cstr()?.is_empty() {} // required-content list
        }
        let title = c.cstr()?;
        let _desc = c.cstr()?;
        let _author = c.cstr()?;
        c.u32()?; // addon version

        let mut raw = Vec::new();
        while c.u32()? != 0 {
            let name = c.cstr()?;
            let size = c.i64()?;
            c.u32()?; // crc
            raw.push((name.replace('\\', "/").to_ascii_lowercase(), size as usize));
        }
        let mut offset = c.o;
        let mut entries = Vec::with_capacity(raw.len());
        for (path, size) in raw {
            entries.push(Entry { path, size, offset });
            offset += size;
        }
        if offset > b.len() {
            bail!("GMA truncated: needs {offset} bytes, file has {}", b.len());
        }
        Ok(Gma { data, title, entries })
    }

    pub fn read(&self, e: &Entry) -> &[u8] {
        &self.data.bytes()[e.offset..e.offset + e.size]
    }
}
