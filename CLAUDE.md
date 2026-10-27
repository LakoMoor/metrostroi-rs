# CLAUDE.md

Read **AGENTS.md** first — it has the layout, build/test commands and the gotchas. It is the single source of truth
for all agents; do not duplicate it here.

Claude Code specifics:
- The user writes Russian and wants autonomous progress ("делай", "продолжай портировать"): pick the next item from
  ROADMAP.md / PLAN.md, do it end to end (code, test, screenshot, docs), then report briefly.
- Edit Rust with the Edit/Write tools; for multi-site patches write a `tools/patch_*.py` file and run it.
- Verify visually with `python -m um win shot` (see AGENTS.md) and read the downscaled PNG.
- Update ROADMAP.md and rebuild + copy `Metrostroi-rs.exe` to the repo root after features.
- Never publish Workshop assets (models, textures, sounds) anywhere; screenshots of the running game are fine.
