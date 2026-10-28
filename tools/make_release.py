"""Builds dist/Metrostroi-rs-<version>-win64.zip from the share tree (run tools/make_share.py first).

The zip holds the exe, Play.bat, the game folder (own data + Metrostroi Lua, no Workshop assets), and the docs.
"""
import re
import zipfile
from pathlib import Path

ROOT = Path(r"E:\metrostroi2pz")
SHARE = ROOT / "share" / "metrostroi-rs"
version = re.search(r'^version = "([^"]+)"', (ROOT / "sim" / "Cargo.toml").read_text(encoding="utf-8"), re.M).group(1)
out = ROOT / "dist" / f"Metrostroi-rs-{version}-win64.zip"
out.parent.mkdir(exist_ok=True)

files = [SHARE / n for n in ("Metrostroi-rs.exe", "Play.bat", "README.md", "SETUP.md", "ROADMAP.md", "LICENSE", "CHANGELOG.md")]
files += [p for p in (SHARE / "game").rglob("*") if p.is_file() and p.suffix not in (".mpak", ".bsp")]
files = [f for f in files if f.exists()]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for f in files:
        z.write(f, Path(f"Metrostroi-rs-{version}") / f.relative_to(SHARE))
print(f"{out}: {out.stat().st_size / 1e6:.1f} MB, {len(files)} files")
