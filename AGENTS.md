# AGENTS.md — Metrostroi-rs (for Codex, Claude Code and other coding agents)

A standalone metro simulator in **Rust + wgpu** that runs **Metrostroi's original content and Lua logic** read
straight from the user's own Steam Workshop files (no conversion). The user works autonomously with agents and speaks
Russian: answer in Russian, keep reports short, say plainly what was verified and what was not.

## Layout
```
sim/                      cargo workspace
  crates/formats          GMA/VFS, MDL/VVD/VTX, VTF/VMT, BSP (+entities, pakfile), skeleton
  crates/lua              mlua 5.1 host + gmod.lua / entity.lua GMod shim
  crates/viewer           the game (bin `metrostroi-rs`): app.rs window/input/camera/HUD, sim.rs Lua<->Rust glue,
                          dynamics.rs, track.rs, station.rs, signals.rs, ground.rs, audio.rs, ui.rs, menu.rs ...
game/                     runtime data: config/game.json, content/{trains,maps}, assets, docs, lua/ (Metrostroi scripts)
tools/                    helper scripts (make_font.py, make_tooltips.py, make_share.py, build_game.bat ...)
ROADMAP.md  PLAN.md       what is done / next
```
`game/lua` is the Metrostroi Lua tree (`entities/`, `metrostroi/systems/` ...). `game/config/game.json` holds
`workshop` (Steam Workshop dir of GMod, appid 4000), `packs` (Workshop item ids, mounted in order, later wins),
`lua_root` (relative to `game/`), `train`, `map`, `obstacles` (standing trains, metres along the main line).
**Use forward slashes in JSON** (backslashes silently fall back to defaults).

## Build / run / test
```
cd sim
cargo build --release -p viewer            # -> sim/target/release/metrostroi-rs.exe
cargo test  --release -p viewer            # 18 tests; those needing map/Lua files skip if the files are absent
target/release/metrostroi-rs.exe --cab --at 300          # options: --nomenu --block <m> --noworld --shot out.png --world <m>
```
Kill a running exe before rebuilding (`taskkill //IM metrostroi-rs.exe //F`). Rust deps: wgpu 30, winit 0.30,
glam **0.30 (don't upgrade)**, mlua 0.10 (lua51, vendored), rodio 0.20, png 0.18.

Keys: F2 orbit · F3 cab · F4 stand up/sit · WASD walk (open a door first: KDL/KDP buttons) · F5 scripted start-up ·
F6 reset ARS autostop · arrows/PgUp/PgDn/Home move the head · Esc menu.

## Rules that bit before
- **Editing:** do not push Rust/WGSL/JSON with quotes or backslashes through shell heredocs (mangled, "unexpected EOF").
  Use the editor tools, or write a small `.py` patch file and run it.
- Units: Source units × **0.01905** m (`renderer::SCALE`) everywhere; Source axes (x forward, y left, z up).
- 717 start-up: breakers/VAH start *closed* (check `Value` before toggling); brake valve needs the three DriverValve
  disconnect toggles + PAD_1; traction needs T1 → 0 → X fast with VozvratRP held in KV 0. `Sim::quick_start` does it.
- ARS is a Rust stand-in (`Sim::enforce_ars`, `track::aspect`); the real BARS chain is unsolved (EPKC/RPB stay 0).
- The Lua tables for signals/plates are read by running the entity's `shared.lua` under stubs (`signals.rs`).
- GUI testing on Windows: `um win` from the universal-modder plugin (`python -m um win drive|shot ...`), window client
  size 1600x900. Look at `--scale 0.5` screenshots.
- Not redistributable: Workshop models/textures/sounds. They are mounted from Steam at run time, never copied.

## Status and next steps
See ROADMAP.md. Done: formats, renderer, Lua host, cab controls (353/353), dynamics, track+signs+ARS, BSP world,
signals/plates, block occupancy (standing trains), stations/platforms, doors, sound, menu/HUD, tooltips, walking incl.
collisions. Next: moving AI trains and switches (needs path connectivity), lightmaps/static props/sky, announcements and
station names, real BARS chain, multi-car consists, a second train type.

Try another cab: `metrostroi-rs.exe --train gmod_subway_81-717_lvz --cab` (any `entities/gmod_subway_*` that the Lua host can spawn).
