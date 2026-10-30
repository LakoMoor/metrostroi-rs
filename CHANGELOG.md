# Changelog

## 0.1.1
- Wheel slip: traction and braking are limited by wheel-rail adhesion; a settings toggle makes the rails wet and the HUD warns.
- Coupling tension per coupling and the strike speed of the slack take-up in the trip statistics.
- Quick start switches the compressor on (the air no longer runs out after ~5 min); duplicate first stations removed from 4 Moscow lines.

## 0.1.0 — first release
- Original 81-717 cab and Metrostroi Lua logic run directly from your Steam Workshop content (all 353 cab buttons).
- Physics: the bogey force law of the original with a calibrated 1-D model (see ROADMAP.md, "Physics status"), slopes,
  rail voltage sag, coupled multi-car consists with shared train wires (`--cars N`).
- Failure system (F7 trigger, F8 auto, F9 reset), announcer, sounds for every switch.
- Maps: Crossline C4 (from Workshop) and **all 34 Moscow metro routes** generated from OpenStreetMap (line menu in the
  main menu), with stations, block signals and a stand-in vertical profile.
- Walking through the car and outside controls (end cocks, main switch, ...) within arm's reach.
- Graphics quality: standard / high (4x MSAA) / ultra (HDR, bloom, ACES, SSAO, render-scale slider).
- Trip statistics screen with a driving score (stop accuracy, autostops, door violations).
- Key remapping through `keys` in game.json; curve resistance; overruns past the platform end are scored.
- Pause menu: save and load the cab (toggle switches, controller, reverser) to `game/config/cab.json`.
- Own asset package (`assets.mpak`, built locally with `--pack-assets`), friendly start-up errors, Steam Workshop
  auto-detection.
