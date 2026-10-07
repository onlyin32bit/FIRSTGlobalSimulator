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
processed_objects = set()

MECHANISM_NAMES = ('climbwheel', 'intakeroller', 'outtakeroller', 'transferroller')

def is_mechanism_object(obj):
    current = obj
    while current is not None:
        key = ''.join(ch for ch in current.name.lower() if ch.isalnum())
        if any(name in key for name in MECHANISM_NAMES):
            return True
        current = current.parent
    return False

for obj in scene_meshes:
    if obj.name in processed_objects:
        continue
    source_mesh = obj.data
    instances = [item for item in scene_meshes if item.data == source_mesh]
    processed_objects.update(item.name for item in instances)
    # These pieces animate independently at runtime. Preserve their exact shape
    # (especially the grooved climbing wheel contact profile).
    if any(is_mechanism_object(item) for item in instances):
        continue
    ratio = ratio_for(len(source_mesh.polygons))
    if ratio >= 1.0:
        continue

    # Blender 5 refuses to apply a modifier to multi-user mesh data. Make one
    # working copy, decimate it once, then reconnect every original instance to
    # the resulting shared LOD mesh.
    obj.data = source_mesh.copy()
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
