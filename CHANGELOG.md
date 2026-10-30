# Changelog

## 0.1.0 — first release
- Original 81-717 cab and Metrostroi Lua logic run directly from your Steam Workshop content (all 353 cab buttons).
- Wheel slip: traction and braking forces are limited by the wheel-rail adhesion; a settings toggle makes the rails wet
  (adhesion drops to a third) and the HUD shows a wheel-slip warning.
- Coupling slack: the strike speed of the cars crossing the free play when the tension reverses is shown in the trip statistics.
- Trip statistics also show the peak coupling tension of the consist (computed from the per-car force difference).
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
