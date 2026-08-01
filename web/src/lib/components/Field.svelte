<script lang="ts">
  import { T } from '@threlte/core'
  import { useGltf, useMeshopt } from '@threlte/extras'
  import { RigidBody, Collider } from '@threlte/rapier'
  import {
    Euler,
    Matrix4,
    Mesh,
    MeshStandardMaterial,
    Object3D,
    Quaternion,
    Vector3
  } from 'three'
  import { onMount } from 'svelte'

  type Vector3Tuple = [number, number, number]

  interface ParsedCollider {
    id: string
    position: Vector3Tuple
    rotation: Vector3Tuple
    vertices: Float32Array
    indices: Uint32Array
    friction: number
    restitution: number
  }

  interface ParsedSemantics {
    anchors: Record<string, Vector3Tuple>
    zones: Array<{
      id: string
      position: Vector3Tuple
      rotation: Vector3Tuple
      vertices: Float32Array
      indices: Uint32Array
    }>
  }

  let { anchors = $bindable({}) } = $props()

  const GAME_ASSET_ROOT = '/games/fgc-2026'
  const SUPPRESSOR_OPACITY = 0.35
  const meshoptDecoder = useMeshopt()
  const visualGltf = useGltf(`${GAME_ASSET_ROOT}/field.glb`, { meshoptDecoder })
  const configuredScenes = new WeakSet<Object3D>()

  let colliders = $state<ParsedCollider[]>([])
  let semantics = $state<ParsedSemantics>({ anchors: {}, zones: [] })

  function parseAssimpPhysics(data: any): ParsedCollider[] {
    if (!data || !data.rootnode || !data.rootnode.children || !data.meshes) return []

    const result: ParsedCollider[] = []

    for (const c of data.rootnode.children) {
      if (!c.meshes || c.meshes.length === 0) continue
      const m = data.meshes[c.meshes[0]]
      if (!m || !m.vertices || !m.faces) continue

      const mat = new Matrix4().fromArray(c.transformation).transpose()
      const pos = new Vector3()
      const quat = new Quaternion()
      const scale = new Vector3()
      mat.decompose(pos, quat, scale)

      const euler = new Euler().setFromQuaternion(quat)

      const rawVerts = m.vertices
      const scaledVerts = new Float32Array(rawVerts.length)
      for (let i = 0; i < rawVerts.length; i += 3) {
        scaledVerts[i] = rawVerts[i] * scale.x
        scaledVerts[i + 1] = rawVerts[i + 1] * scale.y
        scaledVerts[i + 2] = rawVerts[i + 2] * scale.z
      }

      const indicesArr: number[] = []
      for (const f of m.faces) {
        if (Array.isArray(f) && f.length >= 3) {
          indicesArr.push(f[0], f[1], f[2])
          if (f.length === 4) {
            indicesArr.push(f[0], f[2], f[3])
          }
        }
      }

      let friction = 0.45
      let restitution = 0.05
      if (c.name.includes('SU') || c.name.includes('polycarbonate')) {
        friction = 0.35
        restitution = 0.06
      } else if (c.name.includes('floor') || c.name.includes('Tile')) {
        friction = 0.8
        restitution = 0.02
      }

      result.push({
        id: c.name,
        position: [pos.x, pos.y, pos.z],
        rotation: [euler.x, euler.y, euler.z],
        vertices: scaledVerts,
        indices: new Uint32Array(indicesArr),
        friction,
        restitution
      })
    }

    return result
  }

  function parseAssimpSemantics(data: any): ParsedSemantics {
    const parsedAnchors: Record<string, Vector3Tuple> = {}
    const parsedZones: ParsedSemantics['zones'] = []

    if (!data || !data.rootnode || !data.rootnode.children) {
      return { anchors: parsedAnchors, zones: parsedZones }
    }

    for (const c of data.rootnode.children) {
      const mat = new Matrix4().fromArray(c.transformation).transpose()
      const pos = new Vector3()
      const quat = new Quaternion()
      const scale = new Vector3()
      mat.decompose(pos, quat, scale)

      const euler = new Euler().setFromQuaternion(quat)
      const posTuple: Vector3Tuple = [pos.x, pos.y, pos.z]

      if (!c.meshes || c.meshes.length === 0) {
        parsedAnchors[c.name] = posTuple
      } else {
        const m = data.meshes?.[c.meshes[0]]
        if (m && m.vertices && m.faces) {
          const rawVerts = m.vertices
          const scaledVerts = new Float32Array(rawVerts.length)
          for (let i = 0; i < rawVerts.length; i += 3) {
            scaledVerts[i] = rawVerts[i] * scale.x
            scaledVerts[i + 1] = rawVerts[i + 1] * scale.y
            scaledVerts[i + 2] = rawVerts[i + 2] * scale.z
          }
          const indicesArr: number[] = []
          for (const f of m.faces) {
            if (Array.isArray(f) && f.length >= 3) {
              indicesArr.push(f[0], f[1], f[2])
              if (f.length === 4) {
                indicesArr.push(f[0], f[2], f[3])
              }
            }
          }
          parsedZones.push({
            id: c.name,
            position: posTuple,
            rotation: [euler.x, euler.y, euler.z],
            vertices: scaledVerts,
            indices: new Uint32Array(indicesArr)
          })
        }
      }
    }

    return { anchors: parsedAnchors, zones: parsedZones }
  }

  onMount(() => {
    let cancelled = false

    async function loadJson(fileName: string) {
      const response = await fetch(`${GAME_ASSET_ROOT}/${fileName}`)
      if (!response.ok) {
        throw new Error(`Unable to load ${fileName}: ${response.status} ${response.statusText}`)
      }
      return response.json()
    }

    Promise.all([
      loadJson('field.physics.json'),
      loadJson('field.semantics.json')
    ]).then(([physData, semData]) => {
      if (cancelled) return
      colliders = parseAssimpPhysics(physData)
      semantics = parseAssimpSemantics(semData)
      anchors = semantics.anchors
    })

    return () => {
      cancelled = true
    }
  })

  function configureFieldVisual(scene: Object3D): Object3D {
    if (configuredScenes.has(scene)) return scene

    const transparentMaterials = new Map<MeshStandardMaterial, MeshStandardMaterial>()

    scene.traverse((object) => {
      if (!(object instanceof Mesh)) return

      const sourceMaterials = Array.isArray(object.material)
        ? object.material
        : [object.material]
      const configuredMaterials = sourceMaterials.map((sourceMaterial) => {
        if (!(sourceMaterial instanceof MeshStandardMaterial)) {
          return sourceMaterial
        }

        const existingMaterial = transparentMaterials.get(sourceMaterial)
        if (existingMaterial) return existingMaterial

        const transparentMaterial = sourceMaterial.clone()
        transparentMaterial.name = 'SuppressionUnitPolycarbonateTransparentMaterial'
        transparentMaterial.transparent = true
        transparentMaterial.depthWrite = true
        transparentMaterial.onBeforeCompile = (shader) => {
          shader.vertexShader = shader.vertexShader
            .replace(
              '#include <common>',
              `#include <common>
varying vec3 vSuppressionUnitWorldPosition;`
            )
            .replace(
              '#include <project_vertex>',
              `vec4 suppressionUnitWorldPosition = vec4(transformed, 1.0);
#ifdef USE_BATCHING
  suppressionUnitWorldPosition = batchingMatrix * suppressionUnitWorldPosition;
#endif
#ifdef USE_INSTANCING
  suppressionUnitWorldPosition = instanceMatrix * suppressionUnitWorldPosition;
#endif
vSuppressionUnitWorldPosition = (modelMatrix * suppressionUnitWorldPosition).xyz;
#include <project_vertex>`
            )

          shader.fragmentShader = shader.fragmentShader
            .replace(
              '#include <common>',
              `#include <common>
varying vec3 vSuppressionUnitWorldPosition;

float insideSuppressionUnit(vec3 point, vec3 minimum, vec3 maximum) {
  vec3 aboveMinimum = step(minimum, point);
  vec3 belowMaximum = step(point, maximum);
  return aboveMinimum.x * aboveMinimum.y * aboveMinimum.z
    * belowMaximum.x * belowMaximum.y * belowMaximum.z;
}`
            )
            .replace(
              '#include <opaque_fragment>',
              `float insideRedSuppressionUnit = insideSuppressionUnit(
  vSuppressionUnitWorldPosition,
  vec3(-2.442313, 0.598408, -4.132003),
  vec3(-0.625139, 2.607840, -3.026414)
);
float insideBlueSuppressionUnit = insideSuppressionUnit(
  vSuppressionUnitWorldPosition,
  vec3(0.369907, 0.598408, -4.132003),
  vec3(2.187081, 2.607840, -3.026414)
);
float suppressionUnitRegion = max(
  insideRedSuppressionUnit,
  insideBlueSuppressionUnit
);

// The source CAD assigns this light-green material only to the clear
// polycarbonate panels (Part 1 and Part 4). Color is linearized by map_fragment.
vec3 suppressionUnitPanelColor = vec3(0.796078, 0.905882, 0.745098);
float panelColorDistance = distance(diffuseColor.rgb, suppressionUnitPanelColor);
float polycarbonatePanelMask = 1.0 - smoothstep(0.015, 0.05, panelColorDistance);
float transparentPanelMask = suppressionUnitRegion * polycarbonatePanelMask;

diffuseColor.a *= mix(1.0, ${SUPPRESSOR_OPACITY.toFixed(2)}, transparentPanelMask);
#include <opaque_fragment>`
            )
        }
        transparentMaterial.customProgramCacheKey = () =>
          'fgc26-suppression-unit-polycarbonate-transparent-v2'
        transparentMaterial.needsUpdate = true
        transparentMaterials.set(sourceMaterial, transparentMaterial)
        return transparentMaterial
      })

      object.material = Array.isArray(object.material)
        ? configuredMaterials
        : configuredMaterials[0]
    })

    configuredScenes.add(scene)
    return scene
  }
</script>

<T.Group>
  <!-- Field Visual Model (field.glb) -->
  {#await visualGltf then gltf}
    <T is={configureFieldVisual(gltf.scene)} />
  {/await}

  <!-- Physics Colliders (field.physics.json) -->
  {#each colliders as collider (collider.id)}
    <T.Group position={collider.position} rotation={collider.rotation}>
      <RigidBody type="fixed">
        <Collider
          shape="trimesh"
          args={[collider.vertices, collider.indices]}
          friction={collider.friction}
          restitution={collider.restitution}
        />
      </RigidBody>
    </T.Group>
  {/each}
</T.Group>
