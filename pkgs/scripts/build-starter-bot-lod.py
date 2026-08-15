import bpy
import sys

source, target = sys.argv[sys.argv.index('--') + 1:]
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=source)

def ratio_for(face_count):
    if face_count >= 100_000:
        return 0.08
    if face_count >= 30_000:
        return 0.16
    if face_count >= 8_000:
        return 0.35
    return 1.0

scene_meshes = [item for item in bpy.context.scene.objects if item.type == 'MESH']
processed_meshes = set()
for obj in scene_meshes:
    source_mesh = obj.data
    if source_mesh.name in processed_meshes:
        continue
    processed_meshes.add(source_mesh.name)
    ratio = ratio_for(len(source_mesh.polygons))
    if ratio >= 1.0:
        continue

    instances = [item for item in scene_meshes if item.data == source_mesh]
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    modifier = obj.modifiers.new('runtime_lod', 'DECIMATE')
    modifier.decimate_type = 'COLLAPSE'
    modifier.ratio = ratio
    modifier.use_collapse_triangulate = True
    modifier.delimit = {'NORMAL', 'MATERIAL', 'UV', 'SEAM', 'SHARP'}
    bpy.ops.object.modifier_apply(modifier=modifier.name)
    for instance in instances:
        instance.data = obj.data
    obj.select_set(False)

bpy.ops.export_scene.gltf(
    filepath=target,
    export_format='GLB',
    export_materials='EXPORT',
    export_cameras=False,
    export_lights=False,
    export_yup=True,
)
