"""Source MDL (v48/49) + VVD + VTX (LOD0) -> OBJ/MTL, Y-up, metres.

Usage: convert_models.py <content_root> <png_root> <out_root> [pack_root ...]
Every bodypart model becomes its own OBJ object (bp<i>_m<j>) so bodygroup
alternatives stay selectable later.
"""
import os
import re
import struct
import sys
from pathlib import Path

SCALE = 0.0254  # Source inch -> metre
BASETEX = re.compile(r'"?\$basetexture"?\s+"?([^"\r\n]+)"?', re.I)


def cstr(b, o):
    return b[o:b.index(b"\0", o)].decode("latin-1")


def load_vvd(path):
    b = path.read_bytes()
    lod0 = struct.unpack_from("<i", b, 16)[0]
    nfix, fixoff, vstart, tstart = struct.unpack_from("<4i", b, 48)
    count = (tstart - vstart) // 48 if nfix else lod0
    raw = [struct.unpack_from("<16x3f3f2f", b, vstart + i * 48) for i in range(count)]
    if not nfix:
        return raw
    out = []
    for i in range(nfix):
        lod, src, n = struct.unpack_from("<3i", b, fixoff + i * 12)
        if lod >= 0:
            out.extend(raw[src:src + n])
    return out


def parse_vtx(b, mdl_ver):
    """-> {(bodypart, model): [(mesh_idx, [(vertex_origMeshVertID list), tri indices])]}"""
    sg_size = 25 if mdl_ver < 49 else 33
    nbp, bpoff = struct.unpack_from("<2i", b, 28)
    res = {}
    for bp in range(nbp):
        bph = bpoff + bp * 8
        nm, mo = struct.unpack_from("<2i", b, bph)
        for m in range(nm):
            mh = bph + mo + m * 8
            nl, lo = struct.unpack_from("<2i", b, mh)
            if nl < 1:
                continue
            lh = mh + lo  # LOD 0
            nmesh, meshoff = struct.unpack_from("<2i", b, lh)
            meshes = []
            for k in range(nmesh):
                msh = lh + meshoff + k * 9
                nsg, sgo = struct.unpack_from("<2i", b, msh)
                tris = []
                for g in range(nsg):
                    sg = msh + sgo + g * sg_size
                    nv, vo, ni, io, ns, so = struct.unpack_from("<6i", b, sg)
                    verts = [struct.unpack_from("<H", b, sg + vo + i * 9 + 4)[0] for i in range(nv)]
                    idx = struct.unpack_from(f"<{ni}H", b, sg + io)
                    for s in range(ns):
                        st = sg + so + s * 27
                        sni, sio = struct.unpack_from("<2i", b, st)
                        sflags = b[st + 18]
                        seg = idx[sio:sio + sni]
                        if sflags & 1:
                            tri = [(seg[i], seg[i + 1], seg[i + 2]) for i in range(0, len(seg) - 2, 3)]
                        else:
                            tri = []
                            for i in range(len(seg) - 2):
                                t = (seg[i], seg[i + 1], seg[i + 2]) if i % 2 == 0 else (seg[i + 1], seg[i], seg[i + 2])
                                if len(set(t)) == 3:
                                    tri.append(t)
                        tris += [tuple(verts[j] for j in t) for t in tri]
                meshes.append(tris)
            res[(bp, m)] = meshes
    return res


def vmt_index(roots):
    idx = {}
    for r in roots:
        for f in (r / "materials").rglob("*.vmt"):
            idx[f.relative_to(r / "materials").with_suffix("").as_posix().lower()] = f
    return idx


def material_png(name, cds, vmts, png_root):
    for cd in cds + [""]:
        key = (cd + name).replace("\\", "/").lower().lstrip("/")
        v = vmts.get(key)
        if v:
            m = BASETEX.search(v.read_text("latin-1"))
            if m:
                tex = m.group(1).replace("\\", "/").lower()
                p = png_root / "materials" / (tex + ".png")
                return p if p.exists() else None
    return None


def convert(mdl, vmts, png_root, out_file):
    b = mdl.read_bytes()
    ver = struct.unpack_from("<i", b, 4)[0]
    vvd, vtx = mdl.with_suffix(".vvd"), mdl.with_suffix(".dx90.vtx")
    if not vtx.exists():
        vtx = mdl.with_suffix(".dx80.vtx")
    if not (vvd.exists() and vtx.exists()):
        raise FileNotFoundError("missing vvd/vtx")
    ntex, texoff, ncd, cdoff, nref, nfam, skinoff, nbp, bpoff = struct.unpack_from("<9i", b, 204)
    texs = [cstr(b, texoff + i * 64 + struct.unpack_from("<i", b, texoff + i * 64)[0]) for i in range(ntex)]
    cds = []
    for i in range(ncd):
        o = struct.unpack_from("<i", b, cdoff + i * 4)[0]
        cds.append(cstr(b, o).replace("\\", "/").lstrip("/"))
    cds = [c if c.endswith("/") or not c else c + "/" for c in cds]
    skin0 = struct.unpack_from(f"<{nref}h", b, skinoff) if nref else ()
    verts = load_vvd(vvd)
    strips = parse_vtx(vtx.read_bytes(), ver)

    mats = {}
    lines = [f"mtllib {out_file.stem}.mtl"]
    vbase = 1
    for bp in range(nbp):
        bph = bpoff + bp * 16
        nmodels, _base, modelidx = struct.unpack_from("<3i", b, bph + 4)[0:3]
        for m in range(nmodels):
            mo = bph + modelidx + m * 148
            nmesh, meshoff, nverts, vertidx = struct.unpack_from("<4i", b, mo + 72)
            meshes = strips.get((bp, m))
            if not meshes or nverts == 0:
                continue
            mstart = vertidx // 48
            lines.append(f"o bp{bp}_m{m}")
            for k in range(min(nmesh, len(meshes))):
                mh = mo + meshoff + k * 116
                matidx, _, _, voff = struct.unpack_from("<4i", b, mh)
                ti = skin0[matidx] if matidx < len(skin0) else matidx
                mname = re.sub(r"\W", "_", texs[ti]) if ti < len(texs) else f"mat{matidx}"
                if mname not in mats:
                    mats[mname] = material_png(texs[ti], cds, vmts, png_root) if ti < len(texs) else None
                tris = meshes[k]
                if not tris:
                    continue
                used = sorted({v for t in tris for v in t})
                remap = {v: vbase + i for i, v in enumerate(used)}
                for v in used:
                    p = verts[mstart + voff + v][0:3]
                    lines.append(f"v {p[0]*SCALE:.6f} {p[2]*SCALE:.6f} {-p[1]*SCALE:.6f}")
                for v in used:
                    n, uv = verts[mstart + voff + v][3:6], verts[mstart + voff + v][6:8]
                    lines.append(f"vn {n[0]:.5f} {n[2]:.5f} {-n[1]:.5f}")
                for v in used:
                    uv = verts[mstart + voff + v][6:8]
                    lines.append(f"vt {uv[0]:.6f} {1 - uv[1]:.6f}")
                lines.append(f"usemtl {mname}")
                for a, c, d in tris:  # reverse winding
                    ia, ic, id_ = remap[a], remap[c], remap[d]
                    lines.append(f"f {id_}/{id_}/{id_} {ic}/{ic}/{ic} {ia}/{ia}/{ia}")
                vbase += len(used)

    out_file.parent.mkdir(parents=True, exist_ok=True)
    out_file.write_text("\n".join(lines) + "\n")
    mtl = []
    for name, png in mats.items():
        mtl.append(f"newmtl {name}\nKd 1 1 1")
        if png:
            mtl.append("map_Kd " + Path(os.path.relpath(png, out_file.parent)).as_posix())
    out_file.with_suffix(".mtl").write_text("\n".join(mtl) + "\n")


def main(content, png_root, out_root):
    content, png_root, out_root = Path(content), Path(png_root), Path(out_root)
    packs = [p for p in content.iterdir() if p.is_dir()]
    vmts = vmt_index(packs)
    ok = bad = 0
    for pack in packs:
        for mdl in (pack / "models").rglob("*.mdl"):
            out = out_root / mdl.relative_to(pack).with_suffix(".obj")
            try:
                convert(mdl, vmts, png_root, out)
                ok += 1
            except Exception as e:
                bad += 1
                print("FAIL", mdl.relative_to(pack), type(e).__name__, e)
    print(f"models ok={ok} failed={bad}")


if __name__ == "__main__":
    main(*sys.argv[1:4])
