# Metrostroi-rs — goal and roadmap

## Goal

A standalone metro simulator in Rust + wgpu (the openOMSI approach) that runs **Metrostroi's original content and logic**
read straight from the Steam Workshop files — no conversion step:

- the 81-717 cab looks and behaves like in Metrostroi, **every one of the 353 buttons works 1:1**;
- the train is driven by the original Lua systems (Lua 5.1 + a GMod compatibility shim), with hot paths moved to Rust later;
- you can drive it on a track: physics, signals, stations, sound.

Out of scope for now: Project Zomboid and Minecraft ports (paused), other train types, multiplayer.

## Done

| Area | State |
|---|---|
| Formats | GMA (+legacy LZMA .bin), VFS over 9 workshop packs, MDL/VVD/VTX, VTF (all 1225 decode), VMT |
| Renderer | wgpu, opaque/alpha-test/blend, per-object tint, CPU skinning with pose parameters |
| Skeletons | bones, pose params, sequence-0 blend (frame 0) — 71 animated props parse |
| Window | winit, orbit + cab camera, click picking on all `ButtonMap` panels (353/353 self-test) |
| Lua host | mlua 5.1 + `gmod.lua`/`entity.lua`; 171/172 system files load; 81-717 MVM entity spawns (366 systems) |
| Cab animation | original `cl_init.lua` runs on mock client models → 492 props, switches/knobs/needles driven by Lua state |
| Interaction | click → real `ButtonEvent` → systems react; lamp alpha applied as brightness |

## Roadmap

### M1 — Cab you can operate (current)
- [x] Black surfaces: cab textures are genuinely dark (lit by cab lights in-game); skin textures from the Lua applied, exposure raised
- [x] Live window: hover shows button + tooltip, click reaches the Lua (`R_RadioToggle = 1`); [x] VB + all A* breakers pressed through ButtonEvent: switches latch and the panel lamp set (LUDS, red lamps, AVU…) comes on; the 717's Electric has no `Aux80V` output (only set with `PowerSupply`), so lights depend on other switches
- [x] Hover tooltip (window title); [x] keyboard: all keys go through the entity's own `OnKeyEvent`/`KeyMap` (camera on F2/F3); needs the real start-up (VB, `KVWrenchKV`, `KVReverserUp`) before KV keys act
- [x] Lamp glow: props with "lamp"/"light" in the name are drawn self-lit (tint.a = 2 in the shader), brightness follows the Lua value; [ ] additive halo sprites
- [ ] Doors, seats/masks/ARS variants chosen by the Lua (not defaults)
- [ ] Proper cab camera (head position, look limits) and a first-person feel

### M2 — Drive it
- [x] Rail voltage is now a Rust-controlled input (`Sim::set_rail_voltage`, 0 = dead section), gradient supported (`set_grade`); [ ] sectioned rail from track data, wheel slip
- [x] Traction/brake physics: `viewer/src/dynamics.rs` integrates the bogey fields the Lua writes (motor force x power, brake cylinder) and feeds Speed/SpeedSign/Acceleration back; test `drive_and_stop` runs the real start-up (VB, wrench, reverser, brake valve, T1->0->X3) and gets ~1.06 m/s^2 at 36 km/h, T2 brakes to a stop
- [x] Track: `track_<map>.lua` is plain JSON of node positions (Source units x 0.01905 m) inside the scripts GMA 1095130789; `track.rs` loads it, `game/content/maps/b46_lite/track.lua` extracted (`formats --example extract`), gradient from the main line feeds the dynamics, `Sim::world_pose` gives position/heading. Maps (BSPs) for ~35 maps are in other Workshop GMAs
- [x] `signs_<map>.lua` loaded (`track::load_signs`): b46_lite has 488 signals + 35 switches, 416 within 8 m of the main line (the parallel track counts too); entries carry Pos/Angles/Settings bitfield only
- [x] ARS: crossline_c4 (`game/content/maps/crossline_c4`, full v1.2 signs with Routes/ARSCodes, array-form track) is now the default map; `track::next_signal` finds the signal ahead, its free-line ARS code (last digit of `ARSCodes`) feeds a stand-in ARS section (`Metrostroi.GetARSJoint`) so the train's own `ALSCoil` produces F1..F5 (test `als_coil_gets_code_from_next_signal`)
- [ ] Real occupancy/route logic (red signals, switches), autostop; (was: spawn `gmod_track_signal` entities in the Lua host from these entries (ARS codes/speed limits come from their routes), switches between paths, stations (`auto_*`, `pa_*`)
- [ ] Signals and ARS/ALS inputs, speed limits

### M3 — World
- [x] BSP world (first cut): `formats::bsp` parses VBSP v19/20 brush geometry (crossline_c4: 136 materials, 48k triangles, displacements skipped) and the embedded pakfile zip (3202 files, mounted via `Vfs::mount_files`); `viewer --shot f.png --world <s>` renders it from the main line at s metres (`map.bsp` is copied next to the track data). Missing HL2/CSS textures get a muted stand-in colour by name
- [ ] Lightmaps (lump 8), displacements, static props (lump 35 game lump), sky, embedded-pak textures only cover part of the map; (scale settled: 0.01905 m/unit everywhere — a 717 body is 18.1 m at that scale, 24 m at 0.0254)
- [x] Live window: the map is loaded next to the car (`--noworld` to skip, `--at <m>` start position); the car stays at the origin and the map moves with `Sim::world_pose`, so the cab sees the world through the windows
- [x] Driving live: F5 runs the scripted start-up (6x speed, ends with KV in 0 after T1); then press 3 within ~1 s (RVT delay) to go X3. Title bar shows speed, distance and next signal with its ARS code. Verified: 75 km/h at 734 m, world scrolling past the windows
- [x] ARS stand-in (`Sim::enforce_ars`): the 717's own BARS needs a long chain of cab switches (ARS/ALS/EPK, vigilance PB, RC1... EPK stayed de-energised, W1=0 even with them on), so speed is checked against the ALS code (8/7/6/4 = 80/70/60/40 km/h, +3 tolerance) and the emergency valve is tripped via `Pneumatic:Autostop`; F6 resets it. Test `ars_stops_an_overspeeding_train`
- [ ] Make the real BARS chain work (find why EPKC/RPB stay 0) and drop the stand-in
- [x] Cold and dark: `game/docs/COLD_AND_DARK.md`; test `cold_and_dark_manual_start` drives from the spawn state with cab controls only (battery, valve cocks, wrench, reverser, VUD+VAD, RP reset by a held button, T1->0->X3 -> 43 km/h). Mouse buttons are now held while pressed (`Sim::button`)
- [x] Doors: props start at pose 0 (they were drawn in their open bind pose), poses default to 0 for re-created models and are cached per object; passenger doors follow `DoorL1..4/DoorR1..4` from the Lua pneumatics. Keys: `A` open left, `D` open right (hold), `V` close (VUD1). Verified live: right doors open on D, close on V (~6 s). The map is only drawn in the cab view
- [x] Cab/rear/compartment doors (`CabinDoor`, `PassengerDoor`, `RearDoor`) are ordinary ButtonMap buttons: clickable with Russian tags, animated by the Lua, sounds via PlayOnce (test `cab_doors_swing`); [x] station names in the HUD (parsed from `Metrostroi.StationConfigurations`); [ ] announcements, door-side lamps
- [ ] Tunnel/station models, lighting, skybox
- [ ] Passenger doors and platforms; multi-car consists (coupling, train wires)
- [x] Second train type: `--train gmod_subway_81-717_lvz` loads the LVZ cab (own panels, props and body model from `ENT.Model`) with no 717-specific code; other families (720/722/em508) untested

### M4 — Sound and polish
- [ ] Sound: workshop WAV/MP3/OGG, sound positions and loops from the entity tables
- [ ] Performance: move hot systems from Lua to Rust, GPU skinning
- [ ] Settings, key rebinding, save/restore of switch state

## Risks and notes
- Lua shim is built by discovery: each new system may need more GMod stubs.
- Only frame 0 of animations is decoded; multi-frame animations (doors etc.) will need full sampling.
- Source lighting (lightmaps/ambient cubes) is approximated, so cab brightness will differ from the game.
- Legal: content is read from the user's own Workshop install; nothing is redistributed.

## Game folder
`game/` holds config (`config/game.json`), our assets, per-train data (`content/trains/<id>`), maps, signs, sounds, mods, saves, logs, screenshots; see `game/README.md`. The viewer finds it automatically.

## Menu (done)
Main / pause (Esc) / settings (FOV, mouse, HUD) / help screens drawn by `ui.rs` from a font atlas
(`tools/make_font.py`); settings persist in `game/config/settings.json`; HUD shows speed, next signal, ARS limit
and autostop banner. `--nomenu` skips the main menu.

## Sound + head movement (done)
`audio.rs` (rodio): the client Lua's `SetSoundState`/`PlayOnce` are captured by `__sounds()` in sim.rs and played from the
mounted GMAs (loops with volume/pitch, random one-shots; start-up noises muted for 2.5 s). Volume = settings.volume.
Cab view: arrows move the head (fwd/back/left/right), PgUp/PgDn up/down, Home resets; `!`-lamps are no longer clickable.

## Trackside signals (done)
`signals.rs`: poles, heads and lenses of every `gmod_track_signal` on the main line, laid out from the entity's own
`TrafficLightModels` tables (read by running `shared.lua` under stubs), placed at the sign positions, shown within 450 m in
the cab view. Lamp colour = free-line ARS code (8/7 green, 4-6 yellow, else red) via `Object.tint`. Still missing: real
occupancy (red when a block is taken), route indicators/letters, switches, lamp glow.

## Block occupancy (done, first cut)
Trains standing on the main line (`obstacles` in `game.json` or `--block <m>`) hold signals: `track::aspect` picks the
`ARSCodes` digit by the number of signals between a signal and the nearest obstacle (none = the guarding signal is red,
code 0). Lights, ALS code and HUD follow it; running past a red signal trips the ARS autostop. Not yet drawn: the
standing train itself; no moving AI trains, switches or routes.
- Standing trains from `obstacles` are now drawn (717 body, tail at the obstacle, visible within 600 m); verified with `--block 560 --at 470`.

## Stations (done, first cut)
`formats::bsp::entities` reads the entity lump; `station.rs` turns `gmod_track_platform` + their `info_target` ends into
platforms on the main line (crossline_c4: 11 platforms, ~117 m each, door side). HUD shows the next station/platform,
distance and door side. Missing: station names/announcements, stop-mark scoring, door-side interlock, platform models.
- Doors/stops (HUD only): open doors are reported by side; warnings for doors open while moving or on the side away from the platform; stopping on a platform reports metres short of / past its end for 10 s. No hard interlock. Next: announcements, station names, door-side lamp logic.
- Track plates (`gmod_track_signs`, speed limits etc.): models/offsets from `ENT.SignModels` (read like the signal tables), 221 plates placed on crossline_c4 next to the 258 lights. Placement not yet checked by eye close-up.

## On foot + Metrostroi-style tooltips (done)
F4 stands up from the driver's seat; WASD/Shift walk in car space (floor, side walls, end walls); through an open
passenger door (KDL/KDP) the player steps out onto the map ground (`ground.rs`, floor taken from BSP triangles, 0.5 m
step limit, drop-offs block). F4 near the seat sits back down. Cab buttons stay clickable on foot. Hovering a button
shows its Russian description (`tooltips_ru.json` generated from Metrostroi's language files by `tools/make_tooltips.py`).
Missing: collision with the train body from outside beyond the door box, other cars, climbing ladders/stairs.

## Collisions, share folder, site
Walking collides with map walls (`Ground::blocked`, triangle-vs-capsule samples at knee/waist/chest, axis sliding) and the
car shell. `share/metrostroi-rs` (built by `tools/make_share.py`) holds everything to rebuild on another PC plus
`AGENTS.md`/`CLAUDE.md`/`SETUP.md`. `site/` is a static GitHub Pages site (no Workshop assets, only screenshots).

- Moving lead train: `--lead <m>[,<m>...]` spawns one or more 717s running ahead at 12 m/s (43 km/h); it is an occupied block for the signals, so lights behind it go red/yellow and the ARS stand-in reacts. Reset (menu) restarts it. Next: several AI trains, switches/path connectivity.
- Consist: `--cars <n>` (1..8) draws n-1 more cars coupled behind the driven one, 19.2 m apart, following the main line (visual only for now: same body model, no walking through, no train-wire coupling to the cab).
- Failures: F7 random failure, F8 reset, F9 ageing on/off (FailSim). Rail voltage sags by traction current x 0.03 ohm. `MS_CAR_HEIGHT` overrides the car height above the track node (default 1.6 m).

- World materials/light: base-game HL2/CSS/GMod textures are read from the Garry's Mod install's VPKs (`formats::vpk`, mounted after the addons); map texture packs 3340000198 + 3018420076 are mounted; unknown addon textures get a neutral grey. BSP lightmaps (lump 8, face luxels packed into a 2048-wide atlas, second UV set `uv2`) light the map (object tint alpha 3 selects it in the shader; `--nolightmap` disables). Still open: displacements, static props, skybox, other addons' textures (`mus/`, `se_concrete/`).
- Static props (game lump `sprp`, `bsp::static_props`): 3208 props / 183 models on crossline_c4 (rails, sleepers, station architecture) placed and streamed within 300 m of the train (`--noprops` disables). Skins and fade distances ignored. MDL v44-49 load (HL2 props), 2860 of 3208 placed.
- Displacements (lump 26/33, power 2..4) are meshed (only 20 on crossline_c4); all 3208 static props now resolve thanks to the Garrygrad/Loopline packs. `MS_BSPLOG=1`/`MS_MATLOG=1` print face/material diagnostics.
- Map faces are drawn double-sided (`bsp::double_sided`, render copy only; collision keeps the original triangles): face winding from the BSP edge order does not agree with the plane for many faces. `MS_NOCULL=1` turns culling off for everything.
- Skybox: `skyname` from worldspawn, six `skybox/<name>{ft,bk,lf,rt,up,dn}` faces on a cube pinned to the far plane (tint alpha 4), centred on the train. Car height above the rail is now 1.6 m (bogey origin is 80-84 units below the car origin in the Lua), was 1.9.
- Main line is now chained: the longest path plus the path that starts within 30 m of its end (the return track), 23 km on crossline_c4, so the run does not dead-end at 11.6 km. Switches/side branches (depots, crossovers) are still not followed; signs and platforms project onto the chained line.
- Lighting: static props take the map's leaf ambient cube (lumps 52/56, `StaticProp::ambient`, tint alpha 5 in the shader) so they match the lightmapped brushwork; lightmap and prop brightness use the same x4..x7 decode. Open: additive lamp halos, dynamic car lighting from the tunnel, HDR lightmaps/tonemapping.
- Car lighting follows the surroundings: the car body and cab props are scaled by the mean ambient of static props within 25 m of the train (0.55..1.15, eased); lamps stay self-lit. Shadows: the map's static shadows are baked in the lightmaps; no dynamic shadow maps yet (car/props cast none).

- Announcer: the Lua `net` library now loops server messages back to its receivers, so UPO/ASNP announcements reach the audio queue as whole-file one-shots (files must exist in the mounted packs). crossline_c4's BSP has no `gmod_track_switch` entities, so there are no switches to route over on that map; side branches stay unused.
