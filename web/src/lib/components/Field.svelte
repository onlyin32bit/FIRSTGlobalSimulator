<script lang="ts">
  import { T } from '@threlte/core'
  import { useGltf, useMeshopt } from '@threlte/extras'
  import { RigidBody, Collider } from '@threlte/rapier'
  import {
    Euler,
    Mesh,
    MeshStandardMaterial,
    Object3D,
    Quaternion
  } from 'three'
  import { onMount } from 'svelte'
  import { scores } from '$lib/scoreStore'

  type Vector3Tuple = [number, number, number]
  type QuaternionTuple = [number, number, number, number]
  type ScoreKey = 'blueSU' | 'redSU' | 'blueFS' | 'redFS' | 'EXT'

  interface PhysicsMaterial {
    friction: number
    restitution: number
  }

  interface MaterialRule {
    idIncludes: string
    material: string
  }

  interface CuboidColliderDefinition {
    id: string
    shape: 'cuboid'
    position: Vector3Tuple
    rotation: QuaternionTuple
    halfExtents: Vector3Tuple
    material?: string
    friction?: number
    restitution?: number
  }

  interface CylinderColliderDefinition {
    id: string
    shape: 'cylinder'
    position: Vector3Tuple
    rotation: QuaternionTuple
    halfHeight: number
    radius: number
    material?: string
    friction?: number
    restitution?: number
  }

  interface PhysicsDefinition {
    schemaVersion: number
    coordinateSpace: 'field-local'
    defaultMaterial?: string
    materials?: Record<string, PhysicsMaterial>
    materialRules?: MaterialRule[]
    colliders: Array<CuboidColliderDefinition | CylinderColliderDefinition>
  }

  interface ScoringZoneDefinition {
    id: string
    scoreKey: ScoreKey
    shape: 'cuboid'
    position: Vector3Tuple
    halfExtents: Vector3Tuple
  }

  interface SemanticsDefinition {
    schemaVersion: number
    fieldTransform: {
      position: Vector3Tuple
    }
    anchors: Record<string, Vector3Tuple>
    scoringZones: ScoringZoneDefinition[]
  }

  let { anchors = $bindable({}) } = $props()

  const GAME_ASSET_ROOT = '/games/fgc-2026'
  const DEFAULT_FIELD_POSITION: Vector3Tuple = [-2, -5, 0]
  const SUPPRESSOR_OPACITY = 0.35
  const meshoptDecoder = useMeshopt()
  const visualGltf = useGltf(`${GAME_ASSET_ROOT}/field.glb`, { meshoptDecoder })
  const configuredScenes = new WeakSet<Object3D>()

  let physics = $state<PhysicsDefinition>()
  let semantics = $state<SemanticsDefinition>()
  let fieldPosition = $derived(semantics?.fieldTransform.position ?? DEFAULT_FIELD_POSITION)

  onMount(() => {
    let cancelled = false

    async function loadJson<T>(fileName: string): Promise<T> {
      const response = await fetch(`${GAME_ASSET_ROOT}/${fileName}`)
      if (!response.ok) {
        throw new Error(`Unable to load ${fileName}: ${response.status} ${response.statusText}`)
      }
      return response.json() as Promise<T>
    }

    Promise.all([
      loadJson<PhysicsDefinition>('field.physics.json'),
      loadJson<SemanticsDefinition>('field.semantics.json')
    ]).then(([loadedPhysics, loadedSemantics]) => {
      if (cancelled) return

      physics = loadedPhysics
      semantics = loadedSemantics
      const [offsetX, offsetY, offsetZ] = loadedSemantics.fieldTransform.position
      anchors = Object.fromEntries(
        Object.entries(loadedSemantics.anchors).map(([name, [x, y, z]]) => [
          name,
          [x + offsetX, y + offsetY, z + offsetZ]
        ])
      )
    })

    return () => {
      cancelled = true
    }
  })

  function quaternionToEuler([x, y, z, w]: QuaternionTuple): Vector3Tuple {
    const euler = new Euler().setFromQuaternion(new Quaternion(x, y, z, w))
    return [euler.x, euler.y, euler.z]
  }

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

  function resolveColliderMaterial(
    collider: CuboidColliderDefinition | CylinderColliderDefinition
  ): PhysicsMaterial {
    const ruleMaterial = physics?.materialRules?.find((rule) =>
      collider.id.includes(rule.idIncludes)
    )?.material
    const materialName = collider.material ?? ruleMaterial ?? physics?.defaultMaterial
    const material = materialName ? physics?.materials?.[materialName] : undefined

    return {
      friction: collider.friction ?? material?.friction ?? 0.5,
      restitution: collider.restitution ?? material?.restitution ?? 0
    }
  }

  function handleScore(key: ScoreKey) {
    scores.update((score) => ({ ...score, [key]: score[key] + 1 }))
  }
</script>

<T.Group position={fieldPosition}>
  {#await visualGltf then gltf}
    <T is={configureFieldVisual(gltf.scene)} />
  {/await}

  {#each physics?.colliders ?? [] as collider (collider.id)}
    {@const contactMaterial = resolveColliderMaterial(collider)}
    <T.Group position={collider.position} rotation={quaternionToEuler(collider.rotation)}>
      <RigidBody type="fixed">
        {#if collider.shape === 'cuboid'}
          <Collider
            shape="cuboid"
            args={collider.halfExtents}
            friction={contactMaterial.friction}
            restitution={contactMaterial.restitution}
          />
        {:else}
          <Collider
            shape="cylinder"
            args={[collider.halfHeight, collider.radius]}
            friction={contactMaterial.friction}
            restitution={contactMaterial.restitution}
          />
        {/if}
      </RigidBody>
    </T.Group>
  {/each}

  {#each semantics?.scoringZones ?? [] as zone (zone.id)}
    <T.Group position={zone.position}>
      <RigidBody type="fixed">
        <Collider
          shape="cuboid"
          args={zone.halfExtents}
          sensor
          onsensorenter={() => handleScore(zone.scoreKey)}
        />
      </RigidBody>
    </T.Group>
  {/each}
</T.Group>
