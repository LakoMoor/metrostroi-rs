"""Turn the downloaded OpenStreetMap route relations (tools/fetch_moscow_osm.py) into Metrostroi-rs maps.

For every route relation (one direction of one line) this writes game/content/maps/moscow/<id>/ with
  track.lua      one path in Metrostroi's format (strings "[x y z]" in Source units, 0.01905 m each)
  stations.json  [{"name", "name_en", "s", "side"}]  s = metres along the path of the stop, side = platform side
and game/content/maps/moscow/lines.json, the list the game offers in its line menu.
Geometry (c) OpenStreetMap contributors, ODbL. Distances are measured along the real track geometry.
"""
import json
import math
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "tmp" / "osm"
OUT = ROOT / "game" / "content" / "maps" / "moscow"
LAT0, LON0 = 55.75, 37.62
M_LAT = 111_132.0
M_LON = 111_320.0 * math.cos(math.radians(LAT0))
UNIT = 0.01905  # metres per Source unit
STEP = 8.0  # resampling distance, metres
PLATFORM_HALF = 78.0

STOP_ROLES = {"stop", "stop_entry_only", "stop_exit_only"}


def xy(lat, lon):
    return ((lon - LON0) * M_LON, (lat - LAT0) * M_LAT)


def dist(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def chain(ways):
    """Join way polylines (lists of (x, y)) end to end, flipping where needed. Returns one polyline."""
    pts = list(ways[0])
    rest = ways[1:]
    # if the second way attaches to our start instead of our end, flip the first
    if rest:
        a0, a1 = pts[0], pts[-1]
        b0, b1 = rest[0][0], rest[0][-1]
        if min(dist(a0, b0), dist(a0, b1)) < min(dist(a1, b0), dist(a1, b1)):
            pts.reverse()
    for w in rest:
        if dist(pts[-1], w[-1]) < dist(pts[-1], w[0]):
            w = w[::-1]
        if dist(pts[-1], w[0]) > 400.0:
            continue  # a stray way (depot spur, mapping error): skip rather than jump
        pts.extend(w[1:] if dist(pts[-1], w[0]) < 1.0 else w)
    return pts


def resample(pts, step):
    out = [pts[0]]
    acc = 0.0
    for a, b in zip(pts, pts[1:]):
        seg = dist(a, b)
        if seg < 1e-6:
            continue
        d = step - acc
        while d <= seg:
            t = d / seg
            out.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
            d += step
        acc = seg - (d - step)
    if dist(out[-1], pts[-1]) > 1.0:
        out.append(pts[-1])
    return out


def project(path, cum, p):
    """Nearest point of the polyline to p: (s along, lateral distance, signed side: +1 left of travel, -1 right)."""
    best = (1e18, 0.0, 1)
    for i in range(len(path) - 1):
        a, b = path[i], path[i + 1]
        dx, dy = b[0] - a[0], b[1] - a[1]
        l2 = dx * dx + dy * dy
        t = 0.0 if l2 == 0 else max(0.0, min(1.0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2))
        q = (a[0] + dx * t, a[1] + dy * t)
        d = dist(p, q)
        if d < best[0]:
            cross = dx * (p[1] - a[1]) - dy * (p[0] - a[0])
            best = (d, cum[i] + math.sqrt(l2) * t, 1 if cross < 0 else -1)
    # cross > 0 means the point is to the left of the direction of travel
    return best[1], best[0], best[2]


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    lines = []
    for f in sorted(SRC.glob("*.json")):
        if f.name.endswith("_nodes.json"):
            continue
        rid = f.stem
        rel = json.loads(f.read_text(encoding="utf-8"))["elements"][0]
        tags = rel.get("tags", {})
        nodes_file = SRC / f"{rid}_nodes.json"
        names = {}
        if nodes_file.exists():
            for n in json.loads(nodes_file.read_text(encoding="utf-8"))["elements"]:
                t = n.get("tags", {})
                names[n["id"]] = (t.get("name", ""), t.get("name:en", ""))
        ways = []
        stops = []
        platforms = []
        for m in rel["members"]:
            role = m.get("role", "")
            if m["type"] == "way" and m.get("geometry"):
                g = [xy(p["lat"], p["lon"]) for p in m["geometry"]]
                if role.startswith("platform"):
                    platforms.append(g[len(g) // 2])
                elif role in ("", "forward", "backward"):
                    ways.append(g)
            elif m["type"] == "node":
                if role in STOP_ROLES:
                    stops.append((m["ref"], xy(m["lat"], m["lon"])))
                elif role.startswith("platform"):
                    platforms.append(xy(m["lat"], m["lon"]))
        if not ways or not stops:
            print(f"{rid}: no track or stops, skipped")
            continue
        path = resample(chain(ways), STEP)
        cum = [0.0]
        for a, b in zip(path, path[1:]):
            cum.append(cum[-1] + dist(a, b))
        closed = dist(path[0], path[-1]) < 60.0
        st = []
        last_s = -1e9
        for ref, p in stops:
            name, en = names.get(ref, ("", ""))
            s, off, side = project(path, cum, p)
            if off > 300.0 or not name:
                continue
            side = -1  # right of travel unless a platform member says otherwise
            # platform side: the nearest platform member if there is one close by, else the stop node itself
            side_pt = min(platforms, key=lambda q: dist(q, p), default=None)
            if side_pt and dist(side_pt, p) < 80.0:
                _, _, side = project(path, cum, side_pt)
            if abs(s - last_s) < 150.0 and st and st[-1]["name"] == name:
                continue
            st.append({"name": name, "name_en": en, "s": round(s, 1), "side": "left" if side > 0 else "right"})
            last_s = s
        st.sort(key=lambda x: x["s"])
        if len(st) < 2:
            print(f"{rid}: fewer than two stations, skipped")
            continue
        folder = OUT / rid
        folder.mkdir(exist_ok=True)
        nodes = [f"[{x / UNIT:.1f} {y / UNIT:.1f} {-1000:.1f}]" for x, y in path]
        (folder / "track.lua").write_text(json.dumps([nodes], ensure_ascii=False, indent=0), encoding="utf-8")
        (folder / "stations.json").write_text(json.dumps(st, ensure_ascii=False, indent=1), encoding="utf-8")
        full = tags.get("name", rid)
        lines.append(
            {
                "id": rid,
                "ref": tags.get("ref", ""),
                "name": full,
                "colour": tags.get("colour", "#888888"),
                "from": tags.get("from", st[0]["name"]),
                "to": tags.get("to", st[-1]["name"]),
                "length_km": round(cum[-1] / 1000.0, 2),
                "stations": len(st),
                "loop": closed,
            }
        )
        print(f"{rid}: {full}  {cum[-1] / 1000:.1f} km, {len(st)} stations{'  (loop)' if closed else ''}")
    lines.sort(key=lambda l: (l["ref"].zfill(3), l["name"]))
    (OUT / "lines.json").write_text(json.dumps(lines, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"{len(lines)} routes written to {OUT}")


if __name__ == "__main__":
    main()
