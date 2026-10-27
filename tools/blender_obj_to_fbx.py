"""Run: blender -b -P blender_obj_to_fbx.py -- <obj_root> <fbx_root>
Imports every OBJ and exports an FBX (Y-up, metres, textures referenced by path).
"""
import sys
from pathlib import Path

import bpy

obj_root, fbx_root = (Path(p) for p in sys.argv[sys.argv.index("--") + 1:][:2])
ok = bad = 0
for obj in sorted(obj_root.rglob("*.obj")):
    out = fbx_root / obj.relative_to(obj_root).with_suffix(".fbx")
    if out.exists():
        continue
    out.parent.mkdir(parents=True, exist_ok=True)
    try:
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.wm.obj_import(filepath=str(obj), forward_axis="NEGATIVE_Z", up_axis="Y")
        bpy.ops.export_scene.fbx(
            filepath=str(out),
            axis_forward="-Z",
            axis_up="Y",
            global_scale=1.0,
            apply_unit_scale=True,
            path_mode="STRIP",
            add_leaf_bones=False,
        )
        ok += 1
    except Exception as e:
        bad += 1
        print("FAIL", obj, e)
print(f"fbx ok={ok} failed={bad}")
