"""VTF -> PNG. Usage: convert_textures.py <content_root> <out_root>"""
import sys
from pathlib import Path

from srctools.vtf import VTF


def main(src, dst):
    src, dst = Path(src), Path(dst)
    ok = bad = 0
    for f in src.rglob("*.vtf"):
        out = dst / f.relative_to(src).with_suffix(".png")
        if out.exists():
            continue
        try:
            with open(f, "rb") as fh:
                img = VTF.read(fh).get().to_PIL()
            out.parent.mkdir(parents=True, exist_ok=True)
            img.save(out)
            ok += 1
        except Exception as e:
            bad += 1
            print("FAIL", f, e)
    print(f"textures ok={ok} failed={bad}")


if __name__ == "__main__":
    main(*sys.argv[1:3])
