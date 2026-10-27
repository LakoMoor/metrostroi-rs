import sys, bpy
bpy.ops.wm.read_factory_settings(use_empty=True)
for f in sys.argv[sys.argv.index("--")+1:]:
    bpy.ops.import_scene.fbx(filepath=f)
    print("== FBX", f.split("\\")[-1])
    for o in bpy.data.objects:
        extra = ""
        if o.type == "ARMATURE": extra = [b.name for b in o.data.bones][:8]
        if o.type == "MESH": extra = f"verts={len(o.data.vertices)} vgroups={[g.name for g in o.vertex_groups][:4]} mods={[m.type for m in o.modifiers]} parent={o.parent.name if o.parent else None} scale={tuple(round(s,3) for s in o.scale)} rot={tuple(round(r,2) for r in o.rotation_euler)}"
        print("  ", o.type, o.name, extra)
    bpy.ops.wm.read_factory_settings(use_empty=True)
