//! Game folder discovery and `config/game.json`.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub workshop: String,
    pub packs: Vec<String>,
    pub lua_root: String,
    pub train: String,
    pub controls: String,
    /// Map folder under `game/` holding `track.lua`; empty = no track.
    pub map: String,
    /// Trains standing on the main line (metres along it): they hold the signals behind them at red.
    pub obstacles: Vec<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            workshop: r"E:\Games\steamapps\workshop\content\4000".into(),
            packs: ["1123150860", "1563918075", "1122422169", "302370120", "674649096", "3018420076", "3340000198", "1812142201", "1095094174", "1095098251", "1095100683", "1095105863", "1095109617", "1095111608", "1973160540", "261801217"]
                .map(String::from)
                .to_vec(),
            lua_root: r"E:\metrostroi2pz\src\scripts\lua".into(),
            train: "gmod_subway_81-717_mvm".into(),
            controls: "content/trains/81-717_mvm/controls.json".into(),
            map: "content/maps/crossline_c4".into(),
            obstacles: Vec::new(),
        }
    }
}

/// The `game/` folder: `$METROSTROI_GAME`, else the nearest `game/config/game.json` above the cwd or the exe.
pub fn game_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("METROSTROI_GAME") {
        return Some(PathBuf::from(p));
    }
    let starts = [std::env::current_dir().ok(), std::env::current_exe().ok()];
    for start in starts.into_iter().flatten() {
        for dir in start.ancestors() {
            if dir.join("game/config/game.json").exists() {
                return Some(dir.join("game"));
            }
        }
    }
    None
}

impl Config {
    pub fn load() -> (Config, Option<PathBuf>) {
        let dir = game_dir();
        let cfg = dir
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.join("config/game.json")).ok())
            .and_then(|s| match serde_json::from_str(&s) {
                Ok(c) => Some(c),
                Err(e) => {
                    log::warn!("config/game.json: {e}; using defaults");
                    None
                }
            })
            .unwrap_or_default();
        (cfg, dir)
    }

    /// Resolve a path from the config relative to the game folder.
    pub fn resolve(&self, game: Option<&Path>, rel: &str) -> PathBuf {
        match game {
            Some(g) if Path::new(rel).is_relative() => g.join(rel),
            _ => PathBuf::from(rel),
        }
    }
}
