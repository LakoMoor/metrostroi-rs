# Setting up on another PC (Windows)

1. **Install** Rust (stable, MSVC toolchain: https://rustup.rs) and the Visual Studio Build Tools (C++ workload).
2. **Steam:** log in and subscribe to the Metrostroi content (GMod Workshop). Needed Workshop item ids (they are listed in
   `game/config/game.json`): 1812142201, 1095094174, 1095098251, 1095100683, 1095105863, 1095109617, 1095111608,
   1973160540, 261801217. Subscribe in Steam, let it download, or use SteamCMD for appid 4000. Track and signs data are in
   `game/content/maps`; the map geometry `map.bsp` is not (third-party). Extract it from item 3340000198:
   `cd sim && cargo run --release -p formats --example extract -- <path to that item's .gma> crossline_c4.bsp ../game/content/maps/crossline_c4/map.bsp`
   (use `--example findall` to list entries). Without it the game still runs, just without the world.
3. **Config:** edit `game/config/game.json` → `"workshop"` = the folder that contains the item-id folders, e.g.
   `C:/Program Files (x86)/Steam/steamapps/workshop/content/4000`. Forward slashes only. `lua_root` is `lua`
   (relative to `game/`, already included).
4. **Build:** `cd sim && cargo build --release -p viewer` (first build ~5–10 min). The exe is
   `sim/target/release/metrostroi-rs.exe`; `Metrostroi-rs.exe` in this folder is a prebuilt copy (Windows x64).
5. **Run:** `Play.bat` or `sim\target\release\metrostroi-rs.exe --cab --at 300` (needs a GPU with Vulkan/DX12).
6. **Tests:** `cd sim && cargo test --release -p viewer`.
7. **AI agents:** open this folder in Codex or Claude Code; they read `AGENTS.md` / `CLAUDE.md`.

Optional: Python 3 for `tools/*.py` (`make_font.py` needs Pillow; `make_tooltips.py` regenerates the Russian tooltips from
`game/lua/metrostroi_data/languages`).
