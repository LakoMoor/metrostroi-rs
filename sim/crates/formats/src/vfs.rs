//! A read-only virtual file system over several GMA archives (later mounts win).

use crate::gma::Gma;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Default)]
pub struct Vfs {
    mounts: Vec<Gma>,
    index: HashMap<String, (usize, usize)>,
    /// Files mounted from memory (e.g. a BSP's embedded pakfile); they win over archives.
    loose: HashMap<String, Vec<u8>>,
}

fn norm(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
}

impl Vfs {
    pub fn mount(&mut self, path: &Path) -> Result<()> {
        let gma = Gma::open(path)?;
        let m = self.mounts.len();
        for (i, e) in gma.entries.iter().enumerate() {
            self.index.insert(e.path.clone(), (m, i));
        }
        self.mounts.push(gma);
        Ok(())
    }

    pub fn mount_files(&mut self, files: Vec<(String, Vec<u8>)>) {
        for (p, d) in files {
            self.loose.insert(norm(&p), d);
        }
    }

    pub fn read(&self, path: &str) -> Option<&[u8]> {
        if let Some(d) = self.loose.get(&norm(path)) {
            return Some(d);
        }
        let &(m, i) = self.index.get(&norm(path))?;
        let g = &self.mounts[m];
        Some(g.read(&g.entries[i]))
    }

    pub fn exists(&self, path: &str) -> bool {
        self.loose.contains_key(&norm(path)) || self.index.contains_key(&norm(path))
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.index.keys().chain(self.loose.keys()).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.index.len() + self.loose.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}
