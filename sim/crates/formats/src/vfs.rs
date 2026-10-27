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
    /// Valve packages (game content); they lose to addons. Index: path -> (package, preload+data joined when needed).
    vpks: Vec<crate::vpk::Vpk>,
    vpk_index: HashMap<String, usize>,
    /// VPK files that have preload bytes and archive data: joined once at mount time.
    vpk_joined: HashMap<String, Vec<u8>>,
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

    /// Mount `<name>_dir.vpk` (HL2, CSS, GMod content). Earlier mounts win.
    pub fn mount_vpk(&mut self, dir_file: &Path) -> Result<()> {
        let vpk = crate::vpk::Vpk::open(dir_file)?;
        let m = self.vpks.len();
        for (p, e) in &vpk.entries {
            if !self.vpk_index.contains_key(p) {
                self.vpk_index.insert(p.clone(), m);
                if !e.preload.is_empty() && e.length > 0 {
                    if let Some(d) = vpk.data(e) {
                        let mut j = e.preload.clone();
                        j.extend_from_slice(d);
                        self.vpk_joined.insert(p.clone(), j);
                    }
                }
            }
        }
        self.vpks.push(vpk);
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
        let key = norm(path);
        if let Some(&(m, i)) = self.index.get(&key) {
            let g = &self.mounts[m];
            return Some(g.read(&g.entries[i]));
        }
        if let Some(j) = self.vpk_joined.get(&key) {
            return Some(j);
        }
        let v = &self.vpks[*self.vpk_index.get(&key)?];
        let e = v.entries.get(&key)?;
        if e.length == 0 {
            return Some(&e.preload);
        }
        v.data(e)
    }

    pub fn exists(&self, path: &str) -> bool {
        self.loose.contains_key(&norm(path)) || self.index.contains_key(&norm(path)) || self.vpk_index.contains_key(&norm(path))
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
