"""mp3/wav -> ogg via bundled ffmpeg. Usage: convert_sounds.py <content_root> <out_root>"""
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import imageio_ffmpeg

FFMPEG = imageio_ffmpeg.get_ffmpeg_exe()


def conv(args):
    f, out = args
    if out.exists():
        return 0
    out.parent.mkdir(parents=True, exist_ok=True)
    r = subprocess.run(
        [FFMPEG, "-v", "error", "-y", "-i", str(f), "-c:a", "libvorbis", "-q:a", "5", str(out)],
        capture_output=True,
    )
    if r.returncode:
        print("FAIL", f, r.stderr.decode(errors="replace").strip()[:200])
        return 1
    return 0


def main(src, dst):
    src, dst = Path(src), Path(dst)
    jobs = [
        (f, dst / f.relative_to(src).with_suffix(".ogg"))
        for ext in ("*.mp3", "*.wav")
        for f in src.rglob(ext)
    ]
    with ThreadPoolExecutor(8) as ex:
        bad = sum(ex.map(conv, jobs))
    print(f"sounds total={len(jobs)} failed={bad}")


if __name__ == "__main__":
    main(*sys.argv[1:3])
