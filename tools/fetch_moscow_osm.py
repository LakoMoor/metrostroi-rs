"""Download the geometry of every Moscow Metro route relation from OpenStreetMap (Overpass API).

Output: tmp/osm/<relation id>.json (one Overpass `out geom` answer each). Data (c) OpenStreetMap contributors, ODbL.
Re-run to resume: relations that are already on disk are skipped.
"""
import json
import pathlib
import sys
import time
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "tmp" / "osm"
OUT.mkdir(parents=True, exist_ok=True)

MIRRORS = [
    "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
    "https://overpass-api.de/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
    "https://overpass.private.coffee/api/interpreter",
]
UA = "metrostroi-rs/0.1 (https://github.com/LakoMoor/metrostroi-rs)"

IDS = [305810, 1475755, 1532874, 309623, 13525793, 13525792, 3873066, 1580607, 10011654, 10011655, 18020152, 18020151,
       21362056, 21362057, 2657084, 2657085, 1472298, 302149, 302148, 1463285, 302147, 1463286, 1462011, 300607,
       300618, 1514369, 1516303, 309620, 1526350, 326491, 6855052, 6855053, 301486, 1570093]


def fetch(rid: int, kind: str = "geom") -> bytes | None:
    q = f"[out:json][timeout:120];relation({rid});out geom;" if kind == "geom" else f"[out:json][timeout:120];relation({rid});node(r);out;"
    body = urllib.parse.urlencode({"data": q}).encode()
    for attempt in range(6):
        for url in MIRRORS:
            try:
                req = urllib.request.Request(url, data=body, headers={"User-Agent": UA})
                with urllib.request.urlopen(req, timeout=150) as r:
                    data = r.read()
                if data.lstrip().startswith(b"{") and b'"elements"' in data:
                    return data
            except Exception as e:  # noqa: BLE001
                print(f"  {rid} {url.split('/')[2]}: {e}", flush=True)
        time.sleep(5 * (attempt + 1))
    return None


def main() -> int:
    missing = 0
    for rid in IDS:
        kinds = (("geom", f"{rid}.json"), ("nodes", f"{rid}_nodes.json"))
        if len(sys.argv) > 1:  # `nodes` or `geom`: only that kind
            kinds = tuple(k for k in kinds if k[0] == sys.argv[1])
        for kind, name in kinds:
            f = OUT / name
            if f.exists() and f.stat().st_size > 500:
                continue
            data = fetch(rid, kind)
            if data is None:
                print(f"{name}: FAILED", flush=True)
                missing += 1
                continue
            f.write_bytes(data)
            print(f"{name}: {len(data) // 1024} KB", flush=True)
    print("missing:", missing)
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
