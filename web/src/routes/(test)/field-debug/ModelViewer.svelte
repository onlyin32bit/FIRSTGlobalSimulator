<script lang="ts">
  import { onMount } from 'svelte'
  import {
    AmbientLight,
    AnimationMixer,
    AxesHelper,
    Box3,
    BoxHelper,
    Color,
    DirectionalLight,
    GridHelper,
    Group,
    InstancedMesh,
    Material,
    Mesh,
    Object3D,
    PerspectiveCamera,
    Raycaster,
    Scene,
    SkeletonHelper,
    Vector2,
    Vector3,
    WebGLRenderer
  } from 'three'
  import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
  import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { VertexNormalsHelper } from 'three/examples/jsm/helpers/VertexNormalsHelper.js'
  import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js'
  import type {
    RenderStats,
    RendererDetails,
    RuntimeGeometry,
    RuntimeMaterial,
    RuntimeModelReport,
    RuntimeNode
  } from './viewer-types'

  let {
    buffer,
    selectedUuid = null,
    wireframe = false,
    showGrid = true,
    showAxes = true,
    showBounds = false,
    showNormals = false,
    showSkeleton = false,
    hiddenUuids = [],
    background = '#10141c',
    animationIndex = -1,
    animationPlaying = false,
    onselect = (_uuid: string | null) => {},
    onloaded = (_report: RuntimeModelReport) => {},
    onstats = (_stats: RenderStats) => {},
    onerror = (_message: string) => {}
  }: {
    buffer: ArrayBuffer
    selectedUuid?: string | null
    wireframe?: boolean
    showGrid?: boolean
    showAxes?: boolean
    showBounds?: boolean
    showNormals?: boolean
    showSkeleton?: boolean
    hiddenUuids?: string[]
    background?: string
    animationIndex?: number
    animationPlaying?: boolean
    onselect?: (uuid: string | null) => void
    onloaded?: (report: RuntimeModelReport) => void
    onstats?: (stats: RenderStats) => void
    onerror?: (message: string) => void
  } = $props()

  let canvas: HTMLCanvasElement
  let renderer: WebGLRenderer
  let scene: Scene
  let camera: PerspectiveCamera
  let controls: OrbitControls
  let model: Object3D | null = null
  let modelFrame: Group
  let grid: GridHelper
  let axes: AxesHelper
  let boundsHelper: BoxHelper | null = null
  let selectionHelper: BoxHelper | null = null
  let normalsGroup: Group
  let skeletonGroup: Group
  let mixer: AnimationMixer | null = null
  let clips: any[] = []
  let frameHandle = 0
  let resizeObserver: ResizeObserver
  let reportTimer = 0
  let modelSize = new Vector3(1, 1, 1)

  function materialList(material: Material | Material[]): Material[] {
    return Array.isArray(material) ? material : [material]
  }

  function describeMaterial(material: any): RuntimeMaterial {
    return {
      uuid: material.uuid,
      name: material.name || '(unnamed)',
      type: material.type,
      transparent: material.transparent,
      opacity: material.opacity,
      alphaTest: material.alphaTest,
      depthTest: material.depthTest,
      depthWrite: material.depthWrite,
      side: material.side,
      color: material.color?.getHexString ? `#${material.color.getHexString()}` : undefined,
      emissive: material.emissive?.getHexString
        ? `#${material.emissive.getHexString()}`
        : undefined,
      metalness: material.metalness,
      roughness: material.roughness,
      map: material.map?.name || material.map?.uuid || null
    }
  }

  function describeGeometry(geometry: any): RuntimeGeometry {
    const attributes: RuntimeGeometry['attributes'] = {}
    for (const [name, attribute] of Object.entries<any>(geometry.attributes ?? {})) {
      attributes[name] = {
        itemSize: attribute.itemSize,
        count: attribute.count,
        normalized: attribute.normalized,
        arrayType: attribute.array?.constructor?.name ?? 'unknown',
        bytes: attribute.array?.byteLength ?? 0
      }
    }
    return {
      name: geometry.name || '(unnamed)',
      uuid: geometry.uuid,
      type: geometry.type,
      drawRange: { ...geometry.drawRange },
      groups: geometry.groups.map((group: any) => ({ ...group })),
      indexCount: geometry.index?.count ?? 0,
      attributes,
      morphAttributes: Object.keys(geometry.morphAttributes ?? {})
    }
  }

  function describeNode(object: Object3D): RuntimeNode {
    const bounds = new Box3().setFromObject(object)
    const size = new Vector3()
    const hasBounds = !bounds.isEmpty()
    if (hasBounds) bounds.getSize(size)
    const mesh = object instanceof Mesh ? object : null
    const worldPosition = new Vector3()
    object.getWorldPosition(worldPosition)

    return {
      uuid: object.uuid,
      parentUuid: object.parent === model ? null : (object.parent?.uuid ?? null),
      children: object.children.map((child) => child.uuid),
      name: object.name || '(unnamed)',
      type: object.type,
      visible: object.visible,
      renderOrder: object.renderOrder,
      position: object.position.toArray(),
      rotation: [object.rotation.x, object.rotation.y, object.rotation.z, object.rotation.order],
      quaternion: object.quaternion.toArray(),
      scale: object.scale.toArray(),
      matrix: object.matrix.toArray(),
      matrixWorld: object.matrixWorld.toArray(),
      worldPosition: worldPosition.toArray(),
      bounds: hasBounds
        ? { min: bounds.min.toArray(), max: bounds.max.toArray(), size: size.toArray() }
        : null,
      userData: { ...object.userData },
      geometry: mesh ? describeGeometry(mesh.geometry) : undefined,
      materials: mesh ? materialList(mesh.material).map(describeMaterial) : undefined
    }
  }

  function rendererDetails(): RendererDetails {
    const context = renderer.getContext()
    const debugInfo = context.getExtension('WEBGL_debug_renderer_info')
    return {
      renderer: debugInfo
        ? context.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL)
        : context.getParameter(context.RENDERER),
      vendor: debugInfo
        ? context.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL)
        : context.getParameter(context.VENDOR),
      webglVersion:
        typeof WebGL2RenderingContext !== 'undefined' && context instanceof WebGL2RenderingContext
          ? 'WebGL 2'
          : 'WebGL 1',
      maxTextureSize: renderer.capabilities.maxTextureSize,
      maxCubemapSize: renderer.capabilities.maxCubemapSize,
      maxSamples: renderer.capabilities.maxSamples,
      maxAnisotropy: renderer.capabilities.getMaxAnisotropy(),
      precision: renderer.capabilities.precision
    }
  }

  function buildReport(): RuntimeModelReport {
    const nodes: RuntimeNode[] = []
    model?.traverse((object) => {
      if (object !== model) nodes.push(describeNode(object))
    })
    const box = model ? new Box3().setFromObject(model) : new Box3()
    const size = new Vector3()
    const center = new Vector3()
    box.getSize(size)
    box.getCenter(center)
    return {
      rootUuids: model?.children.map((child) => child.uuid) ?? [],
      nodes,
      animations: clips.map((clip) => ({
        name: clip.name || '(unnamed)',
        duration: clip.duration,
        tracks: clip.tracks.length
      })),
      bounds: {
        min: box.min.toArray(),
        max: box.max.toArray(),
        size: size.toArray(),
        center: center.toArray()
      },
      renderer: rendererDetails()
    }
  }

  function rebuildNormals() {
    normalsGroup?.clear()
    if (!showNormals || !model || !normalsGroup) return
    const length = Math.max(modelSize.length() * 0.003, 0.005)
    model.traverse((object) => {
      if (object instanceof Mesh && !(object instanceof InstancedMesh)) {
        normalsGroup.add(new VertexNormalsHelper(object, length, 0x22d3ee))
      }
    })
  }

  function rebuildSkeletons() {
    skeletonGroup?.clear()
    if (!showSkeleton || !model || !skeletonGroup) return
    model.traverse((object: any) => {
      if (object.isSkinnedMesh) skeletonGroup.add(new SkeletonHelper(object))
    })
  }

  function applyWireframe() {
    model?.traverse((object) => {
      if (!(object instanceof Mesh)) return
      for (const material of materialList(object.material) as any[]) {
        if ('wireframe' in material) material.wireframe = wireframe
      }
    })
  }

  function updateSelection() {
    if (selectionHelper) {
      scene.remove(selectionHelper)
      selectionHelper.dispose()
      selectionHelper = null
    }
    if (!selectedUuid || !model) return
    const selected = model.getObjectByProperty('uuid', selectedUuid)
    if (!selected) return
    selectionHelper = new BoxHelper(selected, 0xfacc15)
    scene.add(selectionHelper)
  }

  function frameObject(object: Object3D | null = model) {
    if (!object) return
    const box = new Box3().setFromObject(object)
    if (box.isEmpty()) return
    const center = box.getCenter(new Vector3())
    const size = box.getSize(new Vector3())
    const radius = Math.max(size.length() * 0.6, 0.5)
    const direction = new Vector3(1, 0.7, 1).normalize()
    camera.position.copy(center).addScaledVector(direction, radius * 1.8)
    camera.near = Math.max(radius / 1000, 0.001)
    camera.far = Math.max(radius * 100, 100)
    camera.updateProjectionMatrix()
    controls.target.copy(center)
    controls.update()
  }

  function setView(direction: 'top' | 'front' | 'right') {
    if (!model) return
    const box = new Box3().setFromObject(model)
    const center = box.getCenter(new Vector3())
    const size = box.getSize(new Vector3())
    const distance = Math.max(size.length(), 1)
    const offset =
      direction === 'top'
        ? new Vector3(0, distance, 0.001)
        : direction === 'front'
          ? new Vector3(0, 0, distance)
          : new Vector3(distance, 0, 0)
    camera.position.copy(center).add(offset)
    camera.up.set(0, 1, 0)
    controls.target.copy(center)
    controls.update()
  }

  function handlePointer(event: PointerEvent) {
    if (!model) return
    const bounds = canvas.getBoundingClientRect()
    const pointer = new Vector2(
      ((event.clientX - bounds.left) / bounds.width) * 2 - 1,
      -((event.clientY - bounds.top) / bounds.height) * 2 + 1
    )
    const raycaster = new Raycaster()
    raycaster.setFromCamera(pointer, camera)
    const hit = raycaster.intersectObject(model, true)[0]
    onselect(hit?.object.uuid ?? null)
  }

  async function loadModel() {
    try {
      const loader = new GLTFLoader()
      loader.setMeshoptDecoder(MeshoptDecoder)
      const gltf = await loader.parseAsync(buffer.slice(0), '')
      model = gltf.scene
      clips = gltf.animations
      mixer = clips.length ? new AnimationMixer(model) : null
      model.updateMatrixWorld(true)
      const originalBounds = new Box3().setFromObject(model)
      const center = originalBounds.getCenter(new Vector3())
      originalBounds.getSize(modelSize)
      model.position.sub(center)
      model.updateMatrixWorld(true)
      modelFrame.add(model)

      boundsHelper = new BoxHelper(model, 0x38bdf8)
      boundsHelper.visible = showBounds
      scene.add(boundsHelper)
      grid.position.y = -modelSize.y / 2
      applyWireframe()
      rebuildNormals()
      rebuildSkeletons()
      frameObject()
      onloaded(buildReport())
    } catch (error) {
      onerror(error instanceof Error ? error.message : String(error))
    }
  }

  $effect(() => {
    if (scene) scene.background = new Color(background)
  })
  $effect(() => {
    if (grid) grid.visible = showGrid
    if (axes) axes.visible = showAxes
    if (boundsHelper) boundsHelper.visible = showBounds
  })
  $effect(() => {
    wireframe
    applyWireframe()
  })
  $effect(() => {
    showNormals
    rebuildNormals()
  })
  $effect(() => {
    showSkeleton
    rebuildSkeletons()
  })
  $effect(() => {
    selectedUuid
    updateSelection()
  })
  $effect(() => {
    if (!model) return
    const hidden = new Set(hiddenUuids)
    model.traverse((object) => {
      object.visible = !hidden.has(object.uuid)
    })
  })
  $effect(() => {
    if (!mixer) return
    mixer.stopAllAction()
    if (animationIndex >= 0 && clips[animationIndex]) {
      const action = mixer.clipAction(clips[animationIndex])
      action.play()
      action.paused = !animationPlaying
    }
  })

  onMount(() => {
    scene = new Scene()
    scene.background = new Color(background)
    camera = new PerspectiveCamera(50, 1, 0.01, 10000)
    renderer = new WebGLRenderer({ canvas, antialias: true, alpha: false })
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
    controls = new OrbitControls(camera, canvas)
    controls.enableDamping = true
    controls.dampingFactor = 0.08

    modelFrame = new Group()
    normalsGroup = new Group()
    skeletonGroup = new Group()
    grid = new GridHelper(20, 20, 0x334155, 0x1e293b)
    axes = new AxesHelper(1)
    const ambient = new AmbientLight(0xffffff, 1.5)
    const key = new DirectionalLight(0xffffff, 3)
    key.position.set(4, 8, 5)
    const fill = new DirectionalLight(0x9ec5ff, 1.2)
    fill.position.set(-4, 3, -5)
    scene.add(modelFrame, normalsGroup, skeletonGroup, grid, axes, ambient, key, fill)

    resizeObserver = new ResizeObserver(([entry]) => {
      const width = Math.max(1, entry.contentRect.width)
      const height = Math.max(1, entry.contentRect.height)
      renderer.setSize(width, height, false)
      camera.aspect = width / height
      camera.updateProjectionMatrix()
    })
    resizeObserver.observe(canvas)
    canvas.addEventListener('pointerup', handlePointer)

    let previous = performance.now()
    const render = (time: number) => {
      const delta = Math.min((time - previous) / 1000, 0.1)
      previous = time
      if (mixer && animationPlaying) mixer.update(delta)
      controls.update()
      selectionHelper?.update()
      boundsHelper?.update()
      renderer.render(scene, camera)
      if (time - reportTimer > 500) {
        reportTimer = time
        onstats({
          calls: renderer.info.render.calls,
          triangles: renderer.info.render.triangles,
          points: renderer.info.render.points,
          lines: renderer.info.render.lines,
          geometries: renderer.info.memory.geometries,
          textures: renderer.info.memory.textures,
          programs: renderer.info.programs?.length ?? 0
        })
      }
      frameHandle = requestAnimationFrame(render)
    }
    frameHandle = requestAnimationFrame(render)
    loadModel()

    return () => {
      cancelAnimationFrame(frameHandle)
      resizeObserver.disconnect()
      canvas.removeEventListener('pointerup', handlePointer)
      controls.dispose()
      model?.traverse((object) => {
        if (!(object instanceof Mesh)) return
        object.geometry.dispose()
        for (const material of materialList(object.material)) material.dispose()
      })
      renderer.dispose()
    }
  })
</script>

<div class="relative h-full min-h-[420px] overflow-hidden rounded-xl bg-slate-950">
  <canvas bind:this={canvas} class="block h-full w-full cursor-crosshair"></canvas>
  <div class="absolute left-3 top-3 flex flex-wrap gap-1.5">
    <button class="viewer-button" onclick={() => frameObject()}>Frame all</button>
    <button class="viewer-button" onclick={() => setView('top')}>Top</button>
    <button class="viewer-button" onclick={() => setView('front')}>Front</button>
    <button class="viewer-button" onclick={() => setView('right')}>Right</button>
  </div>
  <div class="pointer-events-none absolute bottom-3 left-3 rounded-md bg-black/55 px-2 py-1 text-[11px] text-slate-300">
    Drag: orbit · Wheel: zoom · Right drag: pan · Click: inspect
  </div>
</div>

<style>
  .viewer-button {
    border: 1px solid rgb(71 85 105 / 0.8);
    border-radius: 0.375rem;
    background: rgb(15 23 42 / 0.82);
    padding: 0.3rem 0.55rem;
    color: rgb(226 232 240);
    font-size: 0.7rem;
    backdrop-filter: blur(8px);
  }
  .viewer-button:hover {
    border-color: rgb(56 189 248 / 0.8);
    color: white;
  }
</style>
