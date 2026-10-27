"""blender -b -P make_test_fbx.py -- <out.fbx> : a 1x1x2 m box named 'test', exported like the real models."""
import sys
from pathlib import Path

import bpy

out = Path(sys.argv[sys.argv.index("--") + 1])
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.mesh.primitive_cube_add(size=1.0)
o = bpy.context.active_object
o.name = "test"
o.scale = (1.0, 2.0, 1.0)  # Y is up in the exported file; length along Z after export conversion
bpy.ops.object.transform_apply(scale=True)
out.parent.mkdir(parents=True, exist_ok=True)
bpy.ops.export_scene.fbx(filepath=str(out), axis_forward="-Z", axis_up="Y", global_scale=1.0,
                         apply_unit_scale=True, path_mode="STRIP", add_leaf_bones=False)
print("test fbx written", out)
