# Changelog

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
- Own asset package (`assets.mpak`, built locally with `--pack-assets`), friendly start-up errors, Steam Workshop
  auto-detection.
