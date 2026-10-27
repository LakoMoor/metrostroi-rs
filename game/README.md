# Metrostroi-rs — game folder

Runtime data for the simulator (`sim/` holds the Rust source). Nothing here is copied from the Workshop:
original models, textures and Lua are read from the Steam Workshop folder named in `config/game.json`.

| Folder | Contents |
|---|---|
| `config/` | `game.json` — Workshop path, packs, Lua root, default train, window, audio, key overrides |
| `assets/` | our own files: `shaders/`, `ui/`, `fonts/`, `icons/` |
| `content/trains/<id>/` | per-train data: `train.json` (entity, body model, cab eye), `controls.json` (button panels) |
| `content/maps/` | track and station data (Phase 3) |
| `content/signs/` | signalling / ARS data (Phase 3) |
| `content/sounds/` | sound tables and our own audio (Phase 5) |
| `mods/` | drop-in extra trains/maps, same layout as `content/` |
| `saves/` | saved switch states and sessions |
| `logs/` | run logs |
| `screenshots/` | F12 / `--shot` output |
| `docs/` | design notes |

Run from the repo: `cargo run --release -p viewer` (finds this folder automatically; override with `METROSTROI_GAME`).

## Executable
`Metrostroi-rs.exe` (repo root, built by `tools/build_game.bat`) has the icon from `assets/icons/icon.ico` embedded; `Play.bat` starts it in the cab at 300 m. The window and taskbar icon come from `icon.png`, compiled in from `sim/crates/viewer/`.
