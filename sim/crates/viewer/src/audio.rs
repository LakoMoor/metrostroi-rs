//! Cab sound: looping states and one-shots raised by the train's Lua, decoded from the mounted GMAs.

use crate::sim::{LoopSound, OnceSound};
use formats::vfs::Vfs;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

struct Bytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub struct Audio {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    vfs: Arc<Mutex<Vfs>>,
    cache: HashMap<String, Option<Arc<Vec<u8>>>>,
    loops: HashMap<String, Sink>,
    once: Vec<Sink>,
    seed: u32,
    /// one-shots raised while the train settles at start-up (doors, relays) stay silent
    quiet_until: std::time::Instant,
}

impl Audio {
    pub fn new(vfs: Arc<Mutex<Vfs>>) -> Option<Audio> {
        let (_stream, handle) = OutputStream::try_default().ok()?;
        Some(Audio { _stream, handle, vfs, cache: HashMap::new(), loops: HashMap::new(), once: Vec::new(), seed: 12345, quiet_until: std::time::Instant::now() + std::time::Duration::from_millis(2500) })
    }

    fn load(&mut self, file: &str) -> Option<Arc<Vec<u8>>> {
        if let Some(c) = self.cache.get(file) {
            return c.clone();
        }
        let vfs = self.vfs.lock().unwrap();
        let data = ["sound/", ""].iter().find_map(|pre| vfs.read(&format!("{pre}{file}"))).map(|d| Arc::new(d.to_vec()));
        drop(vfs);
        if data.is_none() {
            log::warn!("missing sound {file}");
        }
        self.cache.insert(file.to_string(), data.clone());
        data
    }

    fn source(&mut self, file: &str) -> Option<Decoder<Cursor<Bytes>>> {
        let data = self.load(file)?;
        match Decoder::new(Cursor::new(Bytes(data))) {
            Ok(d) => Some(d),
            Err(e) => {
                log::warn!("cannot decode {file}: {e}");
                self.cache.insert(file.to_string(), None);
                None
            }
        }
    }

    fn rand(&mut self) -> usize {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed >> 8) as usize
    }

    /// Apply this frame's loop states and start the one-shots; `master` is 0..1.
    pub fn update(&mut self, loops: &[LoopSound], once: &[OnceSound], master: f32) {
        for l in loops {
            let vol = (l.volume * master).clamp(0.0, 2.0);
            if vol < 0.002 {
                if let Some(s) = self.loops.get(&l.id) {
                    s.set_volume(0.0);
                }
                continue;
            }
            if !self.loops.contains_key(&l.id) {
                // start / loop / end triples: the middle one is the loop
                let file = if l.files.len() >= 3 { &l.files[1] } else { &l.files[0] };
                let (Some(src), Ok(sink)) = (self.source(file), Sink::try_new(&self.handle)) else { continue };
                log::info!("sound loop {} = {file}", l.id);
                sink.append(src.repeat_infinite());
                self.loops.insert(l.id.clone(), sink);
            }
            let s = &self.loops[&l.id];
            s.set_volume(vol);
            s.set_speed(l.pitch.clamp(0.2, 3.0));
        }
        self.once.retain(|s| !s.empty());
        if std::time::Instant::now() < self.quiet_until {
            return;
        }
        for o in once {
            if o.files.is_empty() {
                continue;
            }
            let file = o.files[self.rand() % o.files.len()].clone();
            let (Some(src), Ok(sink)) = (self.source(&file), Sink::try_new(&self.handle)) else { continue };
            log::info!("sound once {file}");
            sink.set_volume((o.volume * master).clamp(0.0, 2.0));
            sink.set_speed(o.pitch.clamp(0.2, 3.0));
            sink.append(src);
            self.once.push(sink);
        }
    }

    /// A fresh train: mute its start-up noises for a moment.
    pub fn restart(&mut self) {
        self.quiet_until = std::time::Instant::now() + std::time::Duration::from_millis(2500);
        self.silence();
    }

    pub fn silence(&mut self) {
        for s in self.loops.values() {
            s.set_volume(0.0);
        }
    }
}
