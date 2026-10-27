"""Builds E:\\metrostroi2pz\\share: everything needed to continue building on another PC.

Copies the Rust workspace (no target/), the game folder (own data + Metrostroi Lua scripts under game/lua),
small helper scripts and the docs. Steam Workshop packs are NOT copied: they come from the other PC's Steam.
"""
import json
import shutil
from pathlib import Path

ROOT = Path(r"E:\metrostroi2pz")
OUT = ROOT / "share" / "metrostroi-rs"


def copy_tree(src, dst, skip=()):
    for p in src.rglob("*"):
        rel = p.relative_to(src)
        if any(part in skip for part in rel.parts):
            continue
        t = dst / rel
        if p.is_dir():
            t.mkdir(parents=True, exist_ok=True)
        else:
            t.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(p, t)


if OUT.exists():
    shutil.rmtree(OUT)
OUT.mkdir(parents=True)

copy_tree(ROOT / "sim", OUT / "sim", skip=("target",))
copy_tree(ROOT / "game", OUT / "game", skip=("screenshots", "logs"))
copy_tree(ROOT / "src" / "scripts" / "lua", OUT / "game" / "lua")
if (ROOT / "site").exists():
    copy_tree(ROOT / "site", OUT / "docs")
(OUT / "tools").mkdir()
for f in (ROOT / "tools").iterdir():
    if f.is_file() and f.suffix in (".py", ".bat", ".ps1") and not f.name.startswith("patch_"):
        shutil.copy2(f, OUT / "tools" / f.name)
for name in ("ROADMAP.md", "PLAN.md", "Play.bat"):
    shutil.copy2(ROOT / name, OUT / name)
exe = ROOT / "Metrostroi-rs.exe"
if exe.exists():
    shutil.copy2(exe, OUT / "Metrostroi-rs.exe")

cfg = OUT / "game" / "config" / "game.json"
d = json.loads(cfg.read_text(encoding="utf-8"))
d["lua_root"] = "lua"
d["workshop"] = "C:/Program Files (x86)/Steam/steamapps/workshop/content/4000"
cfg.write_text(json.dumps(d, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

for f in ("README.md", "AGENTS.md", "CLAUDE.md", "SETUP.md"):
    src = ROOT / "share_docs" / f
    if src.exists():
        shutil.copy2(src, OUT / f)

GITIGNORE = "\n".join(["sim/target/", "*.exe", "game/content/maps/*/map.bsp", "game/screenshots/", "game/logs/", ""])
(OUT / ".gitignore").write_text(GITIGNORE, encoding="utf-8")
size = sum(p.stat().st_size for p in OUT.rglob("*") if p.is_file())
print(f"{OUT}: {size / 1e6:.0f} MB")
