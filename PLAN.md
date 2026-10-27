# Plan to a playable simulator

## What "playable" means (acceptance)

A player starts `viewer`, sits in the 81-717 cab on a real Metrostroi track and can:

1. Turn the train on with the real procedure (battery, breakers, KV key/reverser) and every cab button/switch responds.
2. Drive from station to station: traction, brakes, speed shown on the gauges, ARS/ALS limits and signals respected.
3. Open/close doors at a platform.
4. See and hear a believable world: tunnel and station geometry, lighting, motor/brake/door/relay sounds.
5. Run at 60 fps on the RTX 2080 test machine.

Anything beyond that (more trains, multiplayer, schedules, passengers) is after v1.

## Principles
- Reuse Metrostroi's Lua (systems, entity, client animation) rather than rewriting it; Rust owns world, physics and rendering.
- Every phase ends with something verifiable (headless screenshot, unit test, or logged behaviour) before the next starts.
- Drive the shim by discovery: run real code, add the missing GMod API when it fails, keep a regression example per phase.

## Phase 0 — Stabilise what exists (≈1–2 sessions)
Deliverable: the current cab is trustworthy.
- Fix black surfaces (find the materials/normals responsible; compare with Source behaviour).
- Live-window test of click → ButtonEvent → pose change; add hover highlight + tooltip.
- Regression examples: `lua` crate `tick` example asserts VB toggles battery path; `viewer --shot` golden image for the cab.
- Done when: pressing VB then the A* breakers lights panel lamps and the 80 V bus in the window.

## Phase 1 — Operable cab (≈3–4 sessions)
Deliverable: full cab procedure works without a track.
- Keyboard mapping from the entity's `KeyMap` (press/release events, modifiers).
- Mouse interaction parity: hold buttons, drag knobs/controllers, scroll on multi-position switches.
- Lamp glow (self-illumination / additive sprites), panel displays (ASNP/IGLA text, digit displays from `ClientDraw`).
- Variants chosen by the Lua (seats, masks, ARS block, doors) — remove any remaining defaults.
- Door leaves animated (multi-frame sampling in `skeleton.rs`).
- Done when: the standard start-up checklist can be followed using only the mouse/keyboard.

## Phase 2 — Train dynamics (≈4–6 sessions)  ← biggest unknown
Status: 1-D dynamics + feedback done and tested; rail model, gradients and wheel slip still to do.
Deliverable: the train moves, driven by the Lua's electrics/pneumatics.
- Study `gmod_train_bogey` (torque, brake force, friction, slip) and how `Speed`, `Acceleration`, motor current feed back into systems.
- Rust 1-D longitudinal physics along a track distance coordinate: mass, gradient, curve resistance, traction and brake forces from the bogey outputs.
- Replace the forced 750 V with a rail model (contact rail present everywhere at first, later sectioned).
- Feed Speed/odometer/acceleration back into `T.FrontBogey/RearBogey` fields the Lua reads each tick.
- Done when: from standstill, KV in X1/X2/X3 accelerates the car, gauges follow, T1/T2 and the brake valve stop it, emergency brake works.

## Phase 3 — Track and signalling (≈4–6 sessions)
Deliverable: a real route to drive.
- Find where track data lives (`metrostroi_data/track_*`, map GMAs/BSP entity lump, track editor output) and write a loader; pick one small map to start (candidates in the workshop: crossline, b46_lite, loopline).
- Track spline: position/heading/gradient by distance; switches can wait (fixed route first).
- Signals/ARS: port `sv_railnetwork`/signs data to supply `ALSCoil` frequencies, speed limits, signal states to the train.
- Stations: stop markers, platform side for doors.
- Done when: the train follows the map's track, receives ARS codes, and an overspeed triggers the autostop.

## Phase 4 — World rendering (≈4–6 sessions)
Deliverable: a believable environment.
- Source BSP loader for the chosen map (geometry, textures, displacement maybe skipped), or a track-side mesh generator if BSP is too costly.
- Lighting: lightmaps from BSP or baked approximations; headlights and cab light as dynamic lights.
- Camera attached to the moving car; frustum culling, LOD, texture streaming for performance.
- Done when: the route is recognisable and ≥60 fps in the cab.

## Phase 5 — Sound (≈3–4 sessions)
Deliverable: audio feedback.
- Audio backend (kira or rodio) with 3-D positions and loops.
- Read the entity's `SoundNames/SoundPositions`, the client `Sounds` logic (motor pitch from speed/current, compressor, relays, doors, announcements).
- Done when: motors spool with speed, relays click on switch events, doors chime.

## Phase 6 — Playable release (≈2–3 sessions)
- Start menu (map/train choice), pause, reset train, save/restore of switch state.
- Settings: resolution, vsync, key rebinding, volume.
- Packaging: single exe + config pointing at the Workshop folder; README with setup.
- Done when: a fresh machine with Metrostroi from the Workshop runs the sim without editing code.

## Dependency order
```
P0 → P1 ─┐
P0 → P2 ─┼→ P3 → P4 ─→ P6
P1 → P5 ─┘
```
P1 and P2 can overlap; P5 can start once P2 produces speed/current values.

## Key risks
| Risk | Mitigation |
|---|---|
| Bogey/physics code is tied to Source VPhysics | Reimplement the 1-D model in Rust from its formulas; validate against known Metrostroi numbers (acceleration, braking distance) |
| Track data format undocumented or map-bound | Phase 3 starts with a spike on one map; fallback is a hand-made straight/curved test track |
| BSP rendering is a large job | Fallback: render only track-side geometry and tunnel meshes; BSP loader later |
| Lua shim gaps keep appearing | Per-phase regression example; stub-by-discovery; never silently swallow errors |
| Source lighting looks different | Accept approximation; tune ambient/lights by eye against reference screenshots |
| Performance of Lua systems per tick | Profile in P2; move the hottest (electric/pneumatic) to Rust only if frame time requires |

## Immediate next steps
1. Black-surface bug (Phase 0).
2. Live-window click test + hover tooltip.
3. Spike: locate track data for one map (starts Phase 3 knowledge early, de-risks the plan).

## References (openOMSI, AGPL-3.0 — read-only inspiration)
Cloned for reading from https://github.com/openOMSI-org/openOMSI. **Licence is AGPL-3.0: copying its code would make this project AGPL; we only borrow ideas/architecture and write our own code unless that is decided otherwise.**
- `docs/ARCHITECTURE.md` — layering (formats → simulation → presentation), threading, tile streaming: model for Phase 4.
- `docs/RAIL.md`, `omsi-sim::rail` — metro/tram/train classes, third rail (`contact_shoe`), track info (tunnel, speed limit): ideas for Phase 2–3 track model.
- `crates/omsi-audio` (cpal mixer, loops/one-shots, volume curves, 3-D): reference for Phase 5.
- `docs/SUSPENSION.md`, `omsi-render` (wgpu materials, lights, shadows): reference for lighting and skinning.
- `omsi-script` / `omsi-sim::host`: how a vehicle script VM drives animations — compare with our Lua-driven cab.

## Plan: stations and platforms (current)
1. [x] `formats::bsp::entities` — parse the entity lump (lump 0) into key/value maps.
2. [x] `station.rs` — platforms from `gmod_track_platform` (StationIndex, PlatformIndex, start/end from the named `info_target`s), projected onto the main line (s range, side of travel).
3. [x] Sim/HUD — distance to the next platform, "стоп-метка", which side the doors open on, station number.
4. [x] Doors: HUD warnings (moving / wrong side) and stop-accuracy report
5. [x] Test on crossline_c4 (platforms found and projected), live screenshot, docs/ROADMAP, rebuild exe.
