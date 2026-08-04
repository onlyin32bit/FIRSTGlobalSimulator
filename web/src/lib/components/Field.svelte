<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { useGltf, useMeshopt, HTML } from '@threlte/extras'
  import { RigidBody, Collider } from '@threlte/rapier'
  import {
    Euler,
    Matrix4,
    Mesh,
    MeshStandardMaterial,
    MeshBasicMaterial,
    Object3D,
    Quaternion,
    Vector3,
    Raycaster
  } from 'three'
  import { onMount } from 'svelte'
  import { get } from 'svelte/store'
  import type { ZoneAABB } from '$lib/scoreStore'
  import { handleLiftActive } from '../../routes/(test)/scene/stores'

  type Vector3Tuple = [number, number, number]

  interface ParsedCollider {
    id: string
    position: Vector3Tuple
    rotation: Vector3Tuple
    vertices: Float32Array
    indices: Uint32Array
    friction: number
    restitution: number
    primitive?: 'cylinder' | 'capsule'
    cylinderHalfHeight?: number
    cylinderRadius?: number
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

  type HumanPlayerBounds = {
    minX: number
    maxX: number
    minZ: number
    maxZ: number
  }

  type FieldAssetUrls = {
    visual: string
    physics: string
    semantics: string
  }

  let {
    anchors = $bindable({}),
    zones = $bindable<ZoneAABB[]>([]),
    humanPlayerAlliance = 'red',
    humanPlayerPosition = $bindable<[number, number, number]>([-4.41658, 1.8, 2.99308]),
    humanPlayerBounds = $bindable<HumanPlayerBounds>({
      minX: -5.196519,
      maxX: -3.636641,
      minZ: 2.320723,
      maxZ: 3.665437
    }),
    assetUrls = {
      visual: '/api/game-packs/fgc-2026/assets/field.glb',
      physics: '/api/game-packs/fgc-2026/assets/field.physics.json',
      semantics: '/api/game-packs/fgc-2026/assets/field.semantics.json'
    } as FieldAssetUrls,
    clientPhysics = true
  } = $props()

  const SUPPRESSOR_OPACITY = 0.35
  const SUPPRESSION_PANEL_COLOR = [0.796078, 0.905882, 0.745098] as const

  let parsedHpZones = $state<Record<string, { pos: [number, number, number]; bounds: HumanPlayerBounds }>>({
    red: {
      pos: [-4.41658, 1.8, 2.99308],
      bounds: { minX: -5.196519, maxX: -3.636641, minZ: 2.320723, maxZ: 3.665437 }
    },
    blue: {
      pos: [4.36804, 1.8, 2.99308],
      bounds: { minX: 3.588101, maxX: 5.147979, minZ: 2.320723, maxZ: 3.665437 }
    }
  })

  $effect(() => {
    const hp = parsedHpZones[humanPlayerAlliance] ?? parsedHpZones.red
    humanPlayerPosition = hp.pos
    humanPlayerBounds = hp.bounds
  })
  const BRACE_COLLIDER_IDS = new Set(['Cylinder', 'Cylinder.001', 'Cylinder.002', 'Cylinder.003'])
  const meshoptDecoder = useMeshopt()
  const visualGltf = useGltf(assetUrls.visual, { meshoptDecoder })
  const configuredScenes = new WeakSet<Object3D>()

  let colliders = $state<ParsedCollider[]>([])
  let semantics = $state<ParsedSemantics>({ anchors: {}, zones: [] })
  let redHandleMesh: Object3D | null = null
  let blueHandleMesh: Object3D | null = null
  let initialRedQuat = new Quaternion()
  let initialBlueQuat = new Quaternion()
  const localXAxis = new Vector3(1, 0, 0)
  const tmpQuat = new Quaternion()
  let currentRedAngleOffset = 0
  let currentBlueAngleOffset = 0
  const HANDLE_LIFT_RAD = -8.3 * (Math.PI / 180)

  let isRedPulling = false
  let isBluePulling = false

  let pulseTimer = 0
  let auraScale = $state(1.0)
  let auraOpacity = $state(0.5)
  let targetedHandlePos = $state<[number, number, number] | null>(null)
  let targetedHandleMesh = $state<Object3D | null>(null)
  let crosshairTextPos = $state<[number, number, number] | null>(null)

  let redHandleAura: Object3D | null = null
  let blueHandleAura: Object3D | null = null

  function createHandleAura(source: Object3D) {
    // Clean up any old auras from HMR
    for (let i = source.children.length - 1; i >= 0; i--) {
      if (source.children[i].name === 'HandleAuraWrapper') {
        source.remove(source.children[i])
      }
    }

    const wrapper = new Object3D()
    wrapper.name = 'HandleAuraWrapper'
    
    const wireframeMesh = source.clone(true)
    wireframeMesh.position.set(0, 0, 0)
    wireframeMesh.quaternion.identity()
    wireframeMesh.scale.set(1, 1, 1)

    wireframeMesh.traverse((child) => {
      if (child instanceof Mesh) {
        child.material = new MeshBasicMaterial({
          color: '#7dd3fc',
          wireframe: true,
          transparent: true,
          opacity: 0.5,
          depthWrite: false,
          depthTest: false
        })
      }
    })

    const coreMesh = source.clone(true)
    coreMesh.position.set(0, 0, 0)
    coreMesh.quaternion.identity()
    coreMesh.scale.set(1, 1, 1)

    coreMesh.traverse((child) => {
      if (child instanceof Mesh) {
        child.material = new MeshBasicMaterial({
          color: '#38bdf8',
          transparent: true,
          opacity: 0.3,
          depthWrite: false,
          depthTest: false
        })
      }
    })

    wrapper.add(wireframeMesh)
    wrapper.add(coreMesh)
    wrapper.visible = false
    
    source.add(wrapper)
    return wrapper
  }

  const { camera } = useThrelte()

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
      if (BRACE_COLLIDER_IDS.has(c.name)) {
        // Lower friction for smooth metal brace/post tubes so robots don't stick to them
        friction = 0.12
        restitution = 0.05
      } else if (c.name.includes('SU') || c.name.includes('polycarbonate')) {
        friction = 0.35
        restitution = 0.06
      } else if (c.name.includes('floor') || c.name.includes('Tile')) {
        friction = 0.8
        restitution = 0.02
      }

      const isBrace = BRACE_COLLIDER_IDS.has(c.name)

      result.push({
        id: c.name,
        position: [pos.x, pos.y, pos.z],
        rotation: [euler.x, euler.y, euler.z],
        vertices: scaledVerts,
        indices: new Uint32Array(indicesArr),
        friction,
        restitution,
        ...(isBrace
          ? {
              primitive: 'capsule' as const,
              // Using capsule primitive eliminates sharp flat end-cap rims
              // that cause dynamic colliders to snag or get wedged at angles.
              cylinderHalfHeight: scale.y,
              cylinderRadius: Math.min(scale.x, scale.z)
            }
          : {})
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

    async function loadJson(url: string) {
      const response = await fetch(url)
      if (!response.ok) {
        throw new Error(`Unable to load ${url}: ${response.status} ${response.statusText}`)
      }
      return response.json()
    }

    Promise.all([
      loadJson(assetUrls.physics),
      loadJson(assetUrls.semantics)
    ]).then(([physData, semData]) => {
      if (cancelled) return
      if (clientPhysics) colliders = parseAssimpPhysics(physData)
      semantics = parseAssimpSemantics(semData)
      anchors = semantics.anchors
      const computeZoneData = (zoneId: string) => {
        const zone = semantics.zones.find((z) => z.id === zoneId)
        if (!zone) return null

        const zoneQuaternion = new Quaternion().setFromEuler(
          new Euler(...zone.rotation)
        )
        const transformedVertex = new Vector3()
        let minX = Infinity
        let maxX = -Infinity
        let minZ = Infinity
        let maxZ = -Infinity

        for (let i = 0; i < zone.vertices.length; i += 3) {
          transformedVertex
            .set(
              zone.vertices[i],
              zone.vertices[i + 1],
              zone.vertices[i + 2]
            )
            .applyQuaternion(zoneQuaternion)

          minX = Math.min(minX, transformedVertex.x + zone.position[0])
          maxX = Math.max(maxX, transformedVertex.x + zone.position[0])
          minZ = Math.min(minZ, transformedVertex.z + zone.position[2])
          maxZ = Math.max(maxZ, transformedVertex.z + zone.position[2])
        }

        if (Number.isFinite(minX) && Number.isFinite(minZ)) {
          return {
            pos: [zone.position[0], 1.8, zone.position[2]] as [number, number, number],
            bounds: { minX, maxX, minZ, maxZ }
          }
        }
        return null
      }

      const redParsed = computeZoneData('redHPzone')
      const blueParsed = computeZoneData('blueHPzone')

      if (redParsed || blueParsed) {
        parsedHpZones = {
          red: redParsed ?? parsedHpZones.red,
          blue: blueParsed ?? parsedHpZones.blue
        }
        const currentHp = parsedHpZones[humanPlayerAlliance] ?? parsedHpZones.red
        humanPlayerPosition = currentHp.pos
        humanPlayerBounds = currentHp.bounds
      }

      // The 5 zone IDs that affect the scoreboard (from FIELDSEMANTICSDEF.txt)
      const SCORING_ZONE_IDS = new Set([
        'blueSUscore', 'redSUscore',
        'blueFSscore', 'redFSscore',
        'EXTscore',
      ])

      // Compute an AABB for every scoring zone from its local vertices, correctly
      // applying the zone's full rotation so tilted/angled zones are accurate.
      zones = semantics.zones
        .filter((zone) => SCORING_ZONE_IDS.has(zone.id))
        .map((zone) => {
        const verts = zone.vertices
        let minX = Infinity, minY = Infinity, minZ = Infinity
        let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity

        const [px, py, pz] = zone.position
        const [rx, ry, rz] = zone.rotation

        // Rebuild the quaternion from the Euler angles stored by parseAssimpSemantics
        const q = new Quaternion().setFromEuler(new Euler(rx, ry, rz))
        const tmp = new Vector3()

        for (let i = 0; i < verts.length; i += 3) {
          // Vertices are already scaled; apply rotation then translate to world space
          tmp.set(verts[i], verts[i + 1], verts[i + 2])
          tmp.applyQuaternion(q)
          const wx = tmp.x + px
          const wy = tmp.y + py
          const wz = tmp.z + pz
          if (wx < minX) minX = wx; if (wx > maxX) maxX = wx
          if (wy < minY) minY = wy; if (wy > maxY) maxY = wy
          if (wz < minZ) minZ = wz; if (wz > maxZ) maxZ = wz
        }

        return {
          id: zone.id,
          min: [minX, minY, minZ] as [number, number, number],
          max: [maxX, maxY, maxZ] as [number, number, number],
        }
      })
    })

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === 'e') {
        handleLiftActive.set(true)
      }
    }
    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === 'e') {
        handleLiftActive.set(false)
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    window.addEventListener('keyup', handleKeyUp)

    return () => {
      cancelled = true
      window.removeEventListener('keydown', handleKeyDown)
      window.removeEventListener('keyup', handleKeyUp)
    }
  })

  useTask((delta) => {
    pulseTimer += delta
    auraScale = 1.0 + Math.sin(pulseTimer * 6.0) * 0.08
    auraOpacity = 0.45 + Math.sin(pulseTimer * 6.0) * 0.2

    const cam = camera.current
    let newTargetedPos: [number, number, number] | null = null
    let newTargetedMesh: Object3D | null = null

    if (cam) {
      const rayOrigin = cam.position
      const rayDirection = new Vector3()
      cam.getWorldDirection(rayDirection).normalize()

      const raycaster = new Raycaster()
      raycaster.set(rayOrigin, rayDirection)
      
      let closestDist = 15.0
      
      const checkHandle = (handleMesh: Object3D | null) => {
        if (!handleMesh) return false
        const intersects = raycaster.intersectObject(handleMesh, true)
        let hit = false
        for (const intersect of intersects) {
          // Ignore any object that is an aura or inside HandleAuraWrapper
          let isAura = false
          intersect.object.traverseAncestors((ancestor) => {
            if (ancestor.name === 'HandleAuraWrapper') isAura = true
          })
          if (isAura || intersect.object.name === 'HandleAuraWrapper') continue
          if ((intersect.object as Mesh).material instanceof MeshBasicMaterial) continue

          if (intersect.distance < closestDist) {
            closestDist = intersect.distance
            hit = true
          }
        }
        return hit
      }

      let hitRed = checkHandle(redHandleMesh)
      let hitBlue = checkHandle(blueHandleMesh)
      if (hitBlue) hitRed = false // Only target the closest one

      if (hitRed && redHandleMesh) {
        newTargetedMesh = redHandleMesh
        const pos = new Vector3()
        redHandleMesh.getWorldPosition(pos)
        pos.y += 0.05
        pos.z += 0.1
        newTargetedPos = [pos.x, pos.y, pos.z]
      } else if (hitBlue && blueHandleMesh) {
        newTargetedMesh = blueHandleMesh
        const pos = new Vector3()
        blueHandleMesh.getWorldPosition(pos)
        pos.y += 0.05
        pos.z += 0.1
        newTargetedPos = [pos.x, pos.y, pos.z]
      }
    }
    targetedHandlePos = newTargetedPos
    targetedHandleMesh = newTargetedMesh

    const isLiftPressed = get(handleLiftActive)

    if (isLiftPressed) {
      if (!isRedPulling && targetedHandleMesh === redHandleMesh) {
        isRedPulling = true
      }
      if (!isBluePulling && targetedHandleMesh === blueHandleMesh) {
        isBluePulling = true
      }
    } else {
      isRedPulling = false
      isBluePulling = false
    }

    const redTargetOffset = isRedPulling ? HANDLE_LIFT_RAD : 0
    const blueTargetOffset = isBluePulling ? HANDLE_LIFT_RAD : 0

    const lerpFactor = Math.min(1.0, 15.0 * delta)

    currentRedAngleOffset += (redTargetOffset - currentRedAngleOffset) * lerpFactor
    currentBlueAngleOffset += (blueTargetOffset - currentBlueAngleOffset) * lerpFactor

    if (redHandleMesh) {
      tmpQuat.setFromAxisAngle(localXAxis, currentRedAngleOffset)
      redHandleMesh.quaternion.copy(initialRedQuat).multiply(tmpQuat)
    }
    if (blueHandleMesh) {
      tmpQuat.setFromAxisAngle(localXAxis, currentBlueAngleOffset)
      blueHandleMesh.quaternion.copy(initialBlueQuat).multiply(tmpQuat)
    }

    if (targetedHandleMesh && cam) {
      const forward = new Vector3()
      cam.getWorldDirection(forward)
      const right = new Vector3().crossVectors(forward, new Vector3(0, 1, 0)).normalize()
      const up = new Vector3(0, 1, 0)

      const textPos = new Vector3()
        .copy(cam.position)
        .addScaledVector(forward, 0.8)
        .addScaledVector(right, 0.15)
        .addScaledVector(up, -0.05)

      crosshairTextPos = [textPos.x, textPos.y, textPos.z]
    } else {
      crosshairTextPos = null
    }

    if (redHandleAura) {
      redHandleAura.visible = (targetedHandleMesh === redHandleMesh)
      if (redHandleAura.visible) {
        redHandleAura.traverse(c => {
          if (c instanceof Mesh) {
            c.material.opacity = c.material.wireframe ? auraOpacity * 0.7 : auraOpacity
          }
        })
      }
    }
    if (blueHandleAura) {
      blueHandleAura.visible = (targetedHandleMesh === blueHandleMesh)
      if (blueHandleAura.visible) {
        blueHandleAura.traverse(c => {
          if (c instanceof Mesh) {
            c.material.opacity = c.material.wireframe ? auraOpacity * 0.7 : auraOpacity
          }
        })
      }
    }
  })

  function configureFieldVisual(scene: Object3D): Object3D {
    if (configuredScenes.has(scene)) return scene

    const transparentMaterials = new Map<MeshStandardMaterial, MeshStandardMaterial>()

    scene.traverse((object) => {
      if (object.name === 'RedHandle') {
        redHandleMesh = object
        if (!object.userData.initialQuat) {
          object.userData.initialQuat = object.quaternion.clone()
        }
        initialRedQuat.copy(object.userData.initialQuat)
      }
      if (object.name === 'BlueHandle') {
        blueHandleMesh = object
        if (!object.userData.initialQuat) {
          object.userData.initialQuat = object.quaternion.clone()
        }
        initialBlueQuat.copy(object.userData.initialQuat)
      }

      if (!(object instanceof Mesh)) return

      const sourceMaterials = Array.isArray(object.material)
        ? object.material
        : [object.material]
      const configuredMaterials = sourceMaterials.map((sourceMaterial) => {
        if (!(sourceMaterial instanceof MeshStandardMaterial)) {
          return sourceMaterial
        }

        // Part 1 and Part 4 are the only source meshes using this material.
        // Do not turn an arbitrary shared material transparent: the GLB may be
        // optimized, and transparent sorting of goal backing panels would then
        // become camera-angle dependent.
        const panelColor = sourceMaterial.color
        const panelColorDistanceSquared =
          (panelColor.r - SUPPRESSION_PANEL_COLOR[0]) ** 2 +
          (panelColor.g - SUPPRESSION_PANEL_COLOR[1]) ** 2 +
          (panelColor.b - SUPPRESSION_PANEL_COLOR[2]) ** 2
        const isSuppressionPanel =
          sourceMaterial.name === '0.796078_0.905882_0.745098_0.000000_0.380392' ||
          panelColorDistanceSquared < 0.00001
        if (!isSuppressionPanel) return sourceMaterial

        const existingMaterial = transparentMaterials.get(sourceMaterial)
        if (existingMaterial) return existingMaterial

        const transparentMaterial = sourceMaterial.clone()
        transparentMaterial.name = 'SuppressionUnitPolycarbonateTransparentMaterial'
        transparentMaterial.transparent = true
        // A blended polycarbonate fragment must never become an occluder in
        // the depth buffer. With depth writes enabled, a near clear panel
        // could hide the red/blue backing geometry behind it depending on the
        // camera's transparent-object sort order. Keep depth testing so walls
        // in front still hide it, but let opaque field geometry establish the
        // depth buffer first.
        transparentMaterial.depthWrite = false
        transparentMaterial.depthTest = true
        transparentMaterial.opacity = SUPPRESSOR_OPACITY
        transparentMaterial.needsUpdate = true
        transparentMaterials.set(sourceMaterial, transparentMaterial)
        return transparentMaterial
      })

      object.material = Array.isArray(object.material)
        ? configuredMaterials
        : configuredMaterials[0]
    })

    if (redHandleMesh && !redHandleAura) {
      redHandleAura = createHandleAura(redHandleMesh)
    }
    if (blueHandleMesh && !blueHandleAura) {
      blueHandleAura = createHandleAura(blueHandleMesh)
    }

    configuredScenes.add(scene)
    return scene
  }
</script>

<T.Group>
  <!-- Field Visual Model (field.glb) -->
  {#await visualGltf then gltf}
    <T is={configureFieldVisual(gltf.scene)} />
  {/await}

  <!-- Browser collisions are only used by the isolated prototype scene. -->
  {#if clientPhysics}{#each colliders as collider (collider.id)}
    <T.Group position={collider.position} rotation={collider.rotation}>
      <RigidBody type="fixed" userData={{ fieldColliderId: collider.id }}>
        {#if collider.primitive === 'capsule'}
          <Collider
            shape="capsule"
            args={[collider.cylinderHalfHeight!, collider.cylinderRadius!]}
            friction={collider.friction}
            restitution={collider.restitution}
          />
        {:else if collider.primitive === 'cylinder'}
          <Collider
            shape="cylinder"
            args={[collider.cylinderHalfHeight!, collider.cylinderRadius!]}
            friction={collider.friction}
            restitution={collider.restitution}
          />
        {:else}
          <Collider
            shape="trimesh"
            args={[collider.vertices, collider.indices]}
            friction={collider.friction}
            restitution={collider.restitution}
          />
        {/if}
      </RigidBody>
    </T.Group>
  {/each}{/if}

  {#if crosshairTextPos}
    <HTML position={crosshairTextPos} center pointerEvents="none">
      <div class="pointer-events-none text-white text-xs font-bold bg-black/70 px-2.5 py-1 rounded-md shadow-lg border border-white/20 whitespace-nowrap">
        Press E to Pull Handle
      </div>
    </HTML>
  {/if}
</T.Group>
