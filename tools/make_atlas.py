"""Merge all materials of an OBJ into one atlas texture (PZ vehicle models take one texture).

Usage: make_atlas.py <in.obj> <out_dir> [name]
Writes <out_dir>/<name>.obj/.mtl and <out_dir>/<name>.png. UVs outside 0..1 are clamped
(tiling is lost); the count is reported.
"""
import math
import re
import sys
from pathlib import Path

from PIL import Image

CELL = 1024


def main(src, out_dir, name=None):
    src, out_dir = Path(src), Path(out_dir)
    name = name or src.stem
    mats, cur = {}, None
    for l in src.with_suffix(".mtl").read_text().splitlines():
        if l.startswith("newmtl "):
            cur = l.split(None, 1)[1]
            mats[cur] = None
        elif l.startswith("map_Kd ") and cur:
            mats[cur] = (src.parent / l.split(None, 1)[1]).resolve()
    names = list(mats)
    g = max(1, math.ceil(math.sqrt(len(names))))
    atlas = Image.new("RGBA", (g * CELL, g * CELL), (128, 128, 128, 255))
    slot = {}
    for i, m in enumerate(names):
        c, r = i % g, i // g
        slot[m] = (c, r)
        p = mats[m]
        if p and p.exists():
            atlas.paste(Image.open(p).convert("RGBA").resize((CELL, CELL), Image.LANCZOS), (c * CELL, r * CELL))

    lines, pending, clamped = [], [], 0
    for l in src.read_text().splitlines():
        if l.startswith("mtllib"):
            l = f"mtllib {name}.mtl"
        elif l.startswith("vt "):
            pending.append(len(lines))
        elif l.startswith("usemtl "):
            c, r = slot[l.split(None, 1)[1]]
            for i in pending:
                u, v = map(float, lines[i].split()[1:3])
                if not (-1e-3 <= u <= 1 + 1e-3 and -1e-3 <= v <= 1 + 1e-3):
                    clamped += 1
                u, v = min(max(u, 0.0), 1.0), min(max(v, 0.0), 1.0)
                lines[i] = f"vt {(c + u) / g:.6f} {(g - r - 1 + v) / g:.6f}"
            pending = []
            l = "usemtl pz_atlas"
        lines.append(l)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / f"{name}.obj").write_text("\n".join(lines) + "\n")
    (out_dir / f"{name}.mtl").write_text(f"newmtl pz_atlas\nKd 1 1 1\nmap_Kd {name}.png\n")
    atlas.save(out_dir / f"{name}.png")
    print(f"{name}: {len(names)} materials -> {g*CELL}px atlas, clamped uvs: {clamped}")


if __name__ == "__main__":
    main(*sys.argv[1:4])
