<script lang="ts">
  import ModelViewer from './ModelViewer.svelte'
  import {
    BUFFER_TARGETS,
    COMPONENT_TYPES,
    PRIMITIVE_MODES,
    downloadJson,
    formatBytes,
    inspectGlb,
    type GlbInspection
  } from './glb-inspector'
  import type { RenderStats, RuntimeModelReport, RuntimeNode } from './viewer-types'

  type Tab =
    | 'overview'
    | 'meshes'
    | 'materials'
    | 'textures'
    | 'data'
    | 'animations'
    | 'extensions'
    | 'diagnostics'
    | 'raw'

  const tabs: Array<{ id: Tab; label: string }> = [
    { id: 'overview', label: 'Overview' },
    { id: 'meshes', label: 'Meshes' },
    { id: 'materials', label: 'Materials' },
    { id: 'textures', label: 'Textures' },
    { id: 'data', label: 'Buffers & Accessors' },
    { id: 'animations', label: 'Animations' },
    { id: 'extensions', label: 'Extensions' },
    { id: 'diagnostics', label: 'Diagnostics' },
    { id: 'raw', label: 'Raw JSON' }
  ]

  let input: HTMLInputElement
  let buffer = $state<ArrayBuffer | null>(null)
  let inspection = $state<GlbInspection | null>(null)
  let runtime = $state<RuntimeModelReport | null>(null)
  let renderStats = $state<RenderStats | null>(null)
  let selectedUuid = $state<string | null>(null)
  let hiddenUuids = $state<string[]>([])
  let error = $state('')
  let loading = $state(false)
  let dragging = $state(false)
  let activeTab = $state<Tab>('overview')
  let hierarchyFilter = $state('')
  let rawFilter = $state('')
  let wireframe = $state(false)
  let showGrid = $state(true)
  let showAxes = $state(true)
  let showBounds = $state(false)
  let showNormals = $state(false)
  let showSkeleton = $state(false)
  let background = $state('#10141c')
  let animationIndex = $state(-1)
  let animationPlaying = $state(false)
  let copied = $state(false)

  let nodeMap = $derived(new Map(runtime?.nodes.map((node) => [node.uuid, node]) ?? []))
  let selectedNode = $derived(selectedUuid ? (nodeMap.get(selectedUuid) ?? null) : null)
  let hierarchyRows = $derived(buildHierarchyRows())
  let rawMatches = $derived(
    rawFilter && inspection
      ? inspection.jsonText
          .split('\n')
          .map((line, index) => ({ line, number: index + 1 }))
          .filter(({ line }) => line.toLowerCase().includes(rawFilter.toLowerCase()))
      : []
  )

  function buildHierarchyRows(): Array<{ node: RuntimeNode; depth: number }> {
    if (!runtime) return []
    const rows: Array<{ node: RuntimeNode; depth: number }> = []
    const query = hierarchyFilter.trim().toLowerCase()
    const matching = new Set<string>()

    if (query) {
      for (const node of runtime.nodes) {
        if (`${node.name} ${node.type} ${node.uuid}`.toLowerCase().includes(query)) {
          matching.add(node.uuid)
          let parent = node.parentUuid
          while (parent) {
            matching.add(parent)
            parent = nodeMap.get(parent)?.parentUuid ?? null
          }
        }
      }
    }

    const visit = (uuid: string, depth: number) => {
      const node = nodeMap.get(uuid)
      if (!node || (query && !matching.has(uuid))) return
      rows.push({ node, depth })
      for (const child of node.children) visit(child, depth + 1)
    }
    for (const root of runtime.rootUuids) visit(root, 0)
    return rows
  }

  async function loadFile(file: File) {
    if (!file.name.toLowerCase().endsWith('.glb')) {
      error = 'Choose a binary glTF (.glb). External .gltf resources cannot be resolved from one file.'
      return
    }
    loading = true
    error = ''
    runtime = null
    renderStats = null
    selectedUuid = null
    hiddenUuids = []
    animationIndex = -1
    animationPlaying = false
    try {
      const nextBuffer = await file.arrayBuffer()
      const nextInspection = inspectGlb(nextBuffer, file.name)
      buffer = nextBuffer
      inspection = nextInspection
      activeTab = nextInspection.diagnostics.some((item) => item.level === 'error')
        ? 'diagnostics'
        : 'overview'
    } catch (cause) {
      buffer = null
      inspection = null
      error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      loading = false
    }
  }

  async function loadBundledField() {
    loading = true
    error = ''
    try {
      const response = await fetch('/games/fgc-2026/field.glb')
      if (!response.ok) throw new Error(`Unable to load bundled field: HTTP ${response.status}`)
      const nextBuffer = await response.arrayBuffer()
      buffer = nextBuffer
      inspection = inspectGlb(nextBuffer, 'fgc-2026-field.glb')
      runtime = null
      selectedUuid = null
      hiddenUuids = []
      activeTab = 'overview'
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      loading = false
    }
  }

  function handleDrop(event: DragEvent) {
    event.preventDefault()
    dragging = false
    const file = event.dataTransfer?.files[0]
    if (file) loadFile(file)
  }

  function toggleNodeVisibility(uuid: string) {
    hiddenUuids = hiddenUuids.includes(uuid)
      ? hiddenUuids.filter((item) => item !== uuid)
      : [...hiddenUuids, uuid]
  }

  function resetWorkbench() {
    buffer = null
    inspection = null
    runtime = null
    renderStats = null
    selectedUuid = null
    hiddenUuids = []
    error = ''
  }

  async function copyRawJson() {
    if (!inspection) return
    await navigator.clipboard.writeText(inspection.jsonText)
    copied = true
    setTimeout(() => (copied = false), 1200)
  }

  function exportReport() {
    if (!inspection) return
    const baseName = inspection.fileName.replace(/\.glb$/i, '')
    downloadJson(`${baseName}.inspection.json`, {
      generatedAt: new Date().toISOString(),
      file: {
        name: inspection.fileName,
        size: inspection.fileSize,
        version: inspection.version,
        chunks: inspection.chunks
      },
      summary: inspection.summary,
      diagnostics: inspection.diagnostics,
      runtime,
      gltf: inspection.json
    })
  }

  function compact(value: unknown): string {
    return JSON.stringify(value)
  }

  function vector(value?: number[], digits = 4): string {
    return value?.map((item) => Number(item).toFixed(digits)).join(', ') ?? '—'
  }

  function extensionOccurrences() {
    if (!inspection) return {}
    const extensions = (values: unknown) =>
      Array.isArray(values)
        ? values.map((value: { extensions?: unknown }) => value.extensions ?? null)
        : []
    return {
      scenes: extensions(inspection.json.scenes),
      nodes: extensions(inspection.json.nodes),
      meshes: extensions(inspection.json.meshes),
      materials: extensions(inspection.json.materials),
      textures: extensions(inspection.json.textures),
      bufferViews: extensions(inspection.json.bufferViews)
    }
  }
</script>

<svelte:head>
  <title>GLB Field Debugger</title>
  <meta
    name="description"
    content="Deep inspection workbench for FIRST Global field GLB assets."
  />
</svelte:head>

<div
  role="main"
  class="min-h-screen bg-[#090d14] text-slate-100"
  ondragover={(event) => {
    event.preventDefault()
    dragging = true
  }}
  ondragleave={(event) => {
    if (event.currentTarget === event.target) dragging = false
  }}
  ondrop={handleDrop}
>
  <header class="sticky top-0 z-30 border-b border-slate-800 bg-[#0b1019]/95 backdrop-blur">
    <div class="mx-auto flex max-w-[1900px] items-center gap-4 px-4 py-3">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="rounded bg-cyan-400/10 px-2 py-0.5 font-mono text-[10px] font-bold tracking-widest text-cyan-300">ASSET LAB</span>
          <h1 class="truncate text-lg font-semibold">GLB Field Debugger</h1>
        </div>
        <p class="truncate text-xs text-slate-500">
          {inspection
            ? `${inspection.fileName} · ${formatBytes(inspection.fileSize)} · glTF ${inspection.version}`
            : 'Inspect scenes, geometry, materials, textures, buffers, extensions, and runtime state'}
        </p>
      </div>
      {#if inspection}
        <button class="button-secondary" onclick={exportReport}>Export report</button>
        <button class="button-secondary" onclick={() => input.click()}>Open another</button>
        <button class="button-danger" onclick={resetWorkbench}>Close</button>
      {/if}
      <input
        bind:this={input}
        class="hidden"
        type="file"
        accept=".glb,model/gltf-binary"
        onchange={(event) => {
          const file = event.currentTarget.files?.[0]
          if (file) loadFile(file)
          event.currentTarget.value = ''
        }}
      />
    </div>
  </header>

  {#if !inspection || !buffer}
    <main class="mx-auto flex min-h-[calc(100vh-72px)] max-w-5xl items-center px-4 py-10">
      <section
        class={`w-full rounded-2xl border-2 border-dashed border-slate-700 bg-slate-900/40 p-10 text-center shadow-2xl shadow-black/30 transition ${dragging ? 'border-cyan-400 bg-cyan-400/5' : ''}`}
      >
        <div class="mx-auto mb-5 grid h-20 w-20 place-items-center rounded-2xl border border-cyan-400/20 bg-cyan-400/10 text-4xl text-cyan-300">⬡</div>
        <h2 class="text-2xl font-semibold">Drop a GLB field asset here</h2>
        <p class="mx-auto mt-3 max-w-xl text-sm leading-6 text-slate-400">
          The file stays in your browser. Inspect the binary container, raw glTF document,
          runtime scene, GPU data, and rendered model without uploading it.
        </p>
        <div class="mt-7 flex flex-wrap justify-center gap-3">
          <button class="button-primary" onclick={() => input.click()}>Choose .glb file</button>
          <button class="button-secondary" onclick={loadBundledField}>Open bundled FGC 2026 field</button>
        </div>
        {#if loading}
          <p class="mt-5 text-sm text-cyan-300">Parsing and decoding model…</p>
        {/if}
        {#if error}
          <div class="mx-auto mt-5 max-w-2xl rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-left text-sm text-red-200">{error}</div>
        {/if}
      </section>
    </main>
  {:else}
    <main class="mx-auto max-w-[1900px] space-y-4 p-4">
      {#if dragging}
        <div class="pointer-events-none fixed inset-3 z-50 grid place-items-center rounded-2xl border-2 border-dashed border-cyan-300 bg-cyan-950/85 text-xl font-semibold text-cyan-100 backdrop-blur">Drop to inspect another GLB</div>
      {/if}

      <section class="grid grid-cols-2 gap-2 sm:grid-cols-4 lg:grid-cols-8">
        {#each [
          ['Nodes', inspection.summary.nodes],
          ['Meshes', inspection.summary.meshes],
          ['Primitives', inspection.summary.primitives],
          ['Vertices', inspection.summary.vertices.toLocaleString()],
          ['Triangles', inspection.summary.triangles.toLocaleString()],
          ['Materials', inspection.summary.materials],
          ['Textures', inspection.summary.textures],
          ['Animations', inspection.summary.animations]
        ] as metric}
          <div class="metric-card">
            <span>{metric[0]}</span>
            <strong>{metric[1]}</strong>
          </div>
        {/each}
      </section>

      <section class="grid min-h-[560px] gap-4 xl:grid-cols-[310px_minmax(520px,1fr)_360px]">
        <aside class="panel flex max-h-[680px] min-h-[420px] flex-col overflow-hidden">
          <div class="panel-heading">
            <div>
              <h2>Scene hierarchy</h2>
              <span>{runtime?.nodes.length ?? 0} runtime objects</span>
            </div>
          </div>
          <div class="border-b border-slate-800 p-2">
            <input class="input" bind:value={hierarchyFilter} placeholder="Filter name, type, UUID…" />
          </div>
          <div class="min-h-0 flex-1 overflow-auto p-1.5 font-mono text-[11px]">
            {#if runtime}
              {#each hierarchyRows as row (row.node.uuid)}
                <div
                  class:selected-row={selectedUuid === row.node.uuid}
                  class="group flex items-center gap-1 rounded px-1 py-1 hover:bg-slate-800"
                  style={`padding-left:${row.depth * 13 + 4}px`}
                >
                  <button
                    class="w-5 shrink-0 text-slate-500 hover:text-white"
                    title={hiddenUuids.includes(row.node.uuid) ? 'Show object' : 'Hide object'}
                    onclick={() => toggleNodeVisibility(row.node.uuid)}
                  >
                    {hiddenUuids.includes(row.node.uuid) ? '○' : '●'}
                  </button>
                  <button
                    class="min-w-0 flex-1 truncate text-left"
                    title={`${row.node.name}\n${row.node.uuid}`}
                    onclick={() => (selectedUuid = row.node.uuid)}
                  >
                    <span class="text-cyan-400/80">{row.node.type}</span>
                    <span class="ml-1 text-slate-300">{row.node.name}</span>
                  </button>
                </div>
              {/each}
              {#if hierarchyRows.length === 0}
                <p class="p-4 text-center text-slate-500">No matching nodes.</p>
              {/if}
            {:else}
              <p class="p-4 text-center text-slate-500">Decoding runtime scene…</p>
            {/if}
          </div>
        </aside>

        <div class="panel min-h-[560px] overflow-hidden p-2">
          <ModelViewer
            {buffer}
            {selectedUuid}
            {wireframe}
            {showGrid}
            {showAxes}
            {showBounds}
            {showNormals}
            {showSkeleton}
            {background}
            {animationIndex}
            {animationPlaying}
            {hiddenUuids}
            onselect={(uuid) => (selectedUuid = uuid)}
            onloaded={(report) => (runtime = report)}
            onstats={(stats) => (renderStats = stats)}
            onerror={(message) => (error = message)}
          />
        </div>

        <aside class="panel max-h-[680px] min-h-[420px] overflow-auto">
          <div class="panel-heading sticky top-0 z-10 bg-slate-900/95 backdrop-blur">
            <div>
              <h2>Runtime inspector</h2>
              <span>{selectedNode ? selectedNode.type : 'No selection'}</span>
            </div>
          </div>
          {#if selectedNode}
            <div class="space-y-4 p-3 text-xs">
              <section>
                <h3 class="section-label">Identity</h3>
                <dl class="property-grid">
                  <dt>Name</dt><dd>{selectedNode.name}</dd>
                  <dt>Type</dt><dd>{selectedNode.type}</dd>
                  <dt>UUID</dt><dd class="break-all">{selectedNode.uuid}</dd>
                  <dt>Visible</dt><dd>{selectedNode.visible ? 'yes' : 'no'}</dd>
                  <dt>Children</dt><dd>{selectedNode.children.length}</dd>
                  <dt>Render order</dt><dd>{selectedNode.renderOrder}</dd>
                </dl>
              </section>
              <section>
                <h3 class="section-label">Transform</h3>
                <dl class="property-grid font-mono">
                  <dt>Position</dt><dd>{vector(selectedNode.position)}</dd>
                  <dt>World</dt><dd>{vector(selectedNode.worldPosition)}</dd>
                  <dt>Rotation</dt><dd>{selectedNode.rotation.slice(0, 3).map((v) => typeof v === 'number' ? v.toFixed(4) : v).join(', ')}</dd>
                  <dt>Quaternion</dt><dd>{vector(selectedNode.quaternion)}</dd>
                  <dt>Scale</dt><dd>{vector(selectedNode.scale)}</dd>
                </dl>
              </section>
              {#if selectedNode.bounds}
                <section>
                  <h3 class="section-label">World bounds</h3>
                  <dl class="property-grid font-mono">
                    <dt>Minimum</dt><dd>{vector(selectedNode.bounds.min)}</dd>
                    <dt>Maximum</dt><dd>{vector(selectedNode.bounds.max)}</dd>
                    <dt>Size</dt><dd>{vector(selectedNode.bounds.size)}</dd>
                  </dl>
                </section>
              {/if}
              {#if selectedNode.geometry}
                <section>
                  <h3 class="section-label">Geometry</h3>
                  <dl class="property-grid">
                    <dt>Name</dt><dd>{selectedNode.geometry.name}</dd>
                    <dt>Index count</dt><dd>{selectedNode.geometry.indexCount.toLocaleString()}</dd>
                    <dt>Groups</dt><dd>{selectedNode.geometry.groups.length}</dd>
                    <dt>Attributes</dt><dd>{Object.keys(selectedNode.geometry.attributes).join(', ')}</dd>
                  </dl>
                  {#each Object.entries(selectedNode.geometry.attributes) as [name, attribute]}
                    <div class="mt-2 rounded border border-slate-800 bg-slate-950 p-2 font-mono text-[10px]">
                      <strong class="text-cyan-300">{name}</strong>
                      · {attribute.count.toLocaleString()} × {attribute.itemSize}
                      · {attribute.arrayType} · {formatBytes(attribute.bytes)}
                    </div>
                  {/each}
                </section>
              {/if}
              {#if selectedNode.materials?.length}
                <section>
                  <h3 class="section-label">Materials</h3>
                  {#each selectedNode.materials as material}
                    <div class="mb-2 rounded border border-slate-800 bg-slate-950 p-2">
                      <strong>{material.name}</strong>
                      <div class="mt-1 font-mono text-[10px] text-slate-400">
                        {material.type} · opacity {material.opacity} · transparent {String(material.transparent)}
                        {#if material.color} · color {material.color}{/if}
                      </div>
                    </div>
                  {/each}
                </section>
              {/if}
              <details>
                <summary class="cursor-pointer text-slate-400">Matrices and userData</summary>
                <pre class="code-block mt-2">{JSON.stringify({
                  matrix: selectedNode.matrix,
                  matrixWorld: selectedNode.matrixWorld,
                  userData: selectedNode.userData
                }, null, 2)}</pre>
              </details>
            </div>
          {:else}
            <div class="p-8 text-center text-sm leading-6 text-slate-500">
              Click a rendered surface or choose an object from the hierarchy.
            </div>
          {/if}
        </aside>
      </section>

      <section class="panel">
        <div class="flex flex-wrap items-center gap-x-5 gap-y-2 border-b border-slate-800 p-3 text-xs">
          <strong class="mr-1 text-slate-300">Viewer</strong>
          {#each [
            ['Wireframe', wireframe, (value: boolean) => (wireframe = value)],
            ['Grid', showGrid, (value: boolean) => (showGrid = value)],
            ['Axes', showAxes, (value: boolean) => (showAxes = value)],
            ['Bounds', showBounds, (value: boolean) => (showBounds = value)],
            ['Normals', showNormals, (value: boolean) => (showNormals = value)],
            ['Skeleton', showSkeleton, (value: boolean) => (showSkeleton = value)]
          ] as control}
            <label class="flex cursor-pointer items-center gap-1.5 text-slate-400">
              <input
                type="checkbox"
                checked={control[1] as boolean}
                onchange={(event) => (control[2] as (value: boolean) => void)(event.currentTarget.checked)}
              />
              {control[0]}
            </label>
          {/each}
          <label class="ml-auto flex items-center gap-2 text-slate-400">
            Background
            <input type="color" bind:value={background} class="h-6 w-9 cursor-pointer border-0 bg-transparent" />
          </label>
          {#if runtime?.animations.length}
            <select class="select" bind:value={animationIndex}>
              <option value={-1}>No animation</option>
              {#each runtime.animations as animation, index}
                <option value={index}>{animation.name} ({animation.duration.toFixed(2)}s)</option>
              {/each}
            </select>
            <button class="button-secondary !py-1" onclick={() => (animationPlaying = !animationPlaying)}>
              {animationPlaying ? 'Pause' : 'Play'}
            </button>
          {/if}
          {#if renderStats}
            <span class="font-mono text-[10px] text-slate-500">
              {renderStats.calls} calls · {renderStats.triangles.toLocaleString()} tris ·
              {renderStats.geometries} geoms · {renderStats.textures} tex
            </span>
          {/if}
        </div>

        <nav class="flex gap-1 overflow-x-auto border-b border-slate-800 p-2">
          {#each tabs as tab}
            <button
              class:active-tab={activeTab === tab.id}
              class="tab-button"
              onclick={() => (activeTab = tab.id)}
            >{tab.label}</button>
          {/each}
        </nav>

        <div class="min-h-[360px] p-4">
          {#if activeTab === 'overview'}
            <div class="grid gap-4 lg:grid-cols-3">
              <article class="detail-card">
                <h3>Asset metadata</h3>
                <dl class="property-grid">
                  <dt>Generator</dt><dd>{inspection.json.asset?.generator ?? '—'}</dd>
                  <dt>Version</dt><dd>{inspection.json.asset?.version ?? '—'}</dd>
                  <dt>Min version</dt><dd>{inspection.json.asset?.minVersion ?? '—'}</dd>
                  <dt>Copyright</dt><dd>{inspection.json.asset?.copyright ?? '—'}</dd>
                  <dt>Default scene</dt><dd>{inspection.json.scene ?? 0}</dd>
                  <dt>Declared length</dt><dd>{formatBytes(inspection.declaredLength)}</dd>
                </dl>
                {#if inspection.json.asset?.extras}
                  <pre class="code-block mt-3">{JSON.stringify(inspection.json.asset.extras, null, 2)}</pre>
                {/if}
              </article>
              <article class="detail-card">
                <h3>GLB container chunks</h3>
                {#each inspection.chunks as chunk}
                  <div class="mb-2 rounded border border-slate-800 bg-slate-950 p-2 font-mono text-xs">
                    <div class="flex justify-between"><strong class="text-cyan-300">{chunk.type}</strong><span>{formatBytes(chunk.byteLength)}</span></div>
                    <div class="mt-1 text-[10px] text-slate-500">header @{chunk.byteOffset} · data @{chunk.dataOffset} · {chunk.typeHex}</div>
                  </div>
                {/each}
              </article>
              <article class="detail-card">
                <h3>Runtime and GPU</h3>
                {#if runtime}
                  <dl class="property-grid">
                    <dt>Bounds size</dt><dd>{vector(runtime.bounds.size, 3)}</dd>
                    <dt>Bounds center</dt><dd>{vector(runtime.bounds.center, 3)}</dd>
                    <dt>Renderer</dt><dd>{runtime.renderer.renderer}</dd>
                    <dt>Vendor</dt><dd>{runtime.renderer.vendor}</dd>
                    <dt>API</dt><dd>{runtime.renderer.webglVersion}</dd>
                    <dt>Precision</dt><dd>{runtime.renderer.precision}</dd>
                    <dt>Max texture</dt><dd>{runtime.renderer.maxTextureSize}px</dd>
                    <dt>Max samples</dt><dd>{runtime.renderer.maxSamples}</dd>
                    <dt>Anisotropy</dt><dd>{runtime.renderer.maxAnisotropy}×</dd>
                  </dl>
                {:else}
                  <p class="text-sm text-slate-500">Waiting for runtime decoder…</p>
                {/if}
              </article>
              <article class="detail-card lg:col-span-3">
                <h3>Scenes</h3>
                <div class="table-wrap">
                  <table>
                    <thead><tr><th>#</th><th>Name</th><th>Root nodes</th><th>Extras</th><th>Extensions</th></tr></thead>
                    <tbody>
                      {#each inspection.json.scenes ?? [] as scene, index}
                        <tr><td>{index}</td><td>{scene.name ?? '—'}</td><td>{scene.nodes?.join(', ') ?? '—'}</td><td><code>{compact(scene.extras ?? {})}</code></td><td><code>{compact(scene.extensions ?? {})}</code></td></tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              </article>
            </div>
          {:else if activeTab === 'meshes'}
            <div class="space-y-3">
              {#each inspection.json.meshes ?? [] as mesh, meshIndex}
                <details class="detail-card" open={meshIndex < 2}>
                  <summary><strong>Mesh {meshIndex}: {mesh.name ?? '(unnamed)'}</strong> <span class="ml-2 text-xs text-slate-500">{mesh.primitives?.length ?? 0} primitives · {mesh.weights?.length ?? 0} morph weights</span></summary>
                  <div class="mt-3 table-wrap">
                    <table>
                      <thead><tr><th>Primitive</th><th>Mode</th><th>Indices</th><th>Material</th><th>Attributes</th><th>Targets</th><th>Extensions</th></tr></thead>
                      <tbody>
                        {#each mesh.primitives ?? [] as primitive, primitiveIndex}
                          <tr>
                            <td>{primitiveIndex}</td>
                            <td>{PRIMITIVE_MODES[primitive.mode ?? 4] ?? primitive.mode}</td>
                            <td>{primitive.indices ?? '—'}</td>
                            <td>{primitive.material ?? '—'}</td>
                            <td><code>{compact(primitive.attributes)}</code></td>
                            <td>{primitive.targets?.length ?? 0}</td>
                            <td><code>{compact(primitive.extensions ?? {})}</code></td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  </div>
                  <pre class="code-block mt-3">{JSON.stringify(mesh, null, 2)}</pre>
                </details>
              {/each}
            </div>
          {:else if activeTab === 'materials'}
            <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
              {#each inspection.json.materials ?? [] as material, index}
                <article class="detail-card">
                  <div class="flex items-center justify-between"><h3>{index}: {material.name ?? '(unnamed)'}</h3><span class="badge">{material.alphaMode ?? 'OPAQUE'}</span></div>
                  <dl class="property-grid">
                    <dt>Double-sided</dt><dd>{String(material.doubleSided ?? false)}</dd>
                    <dt>Alpha cutoff</dt><dd>{material.alphaCutoff ?? '—'}</dd>
                    <dt>Base color</dt><dd><code>{compact(material.pbrMetallicRoughness?.baseColorFactor ?? [1,1,1,1])}</code></dd>
                    <dt>Metallic</dt><dd>{material.pbrMetallicRoughness?.metallicFactor ?? 1}</dd>
                    <dt>Roughness</dt><dd>{material.pbrMetallicRoughness?.roughnessFactor ?? 1}</dd>
                    <dt>Emissive</dt><dd><code>{compact(material.emissiveFactor ?? [0,0,0])}</code></dd>
                  </dl>
                  <pre class="code-block mt-3 max-h-64">{JSON.stringify(material, null, 2)}</pre>
                </article>
              {/each}
              {#if !(inspection.json.materials?.length)}
                <p class="text-slate-500">No materials.</p>
              {/if}
            </div>
          {:else if activeTab === 'textures'}
            <div class="grid gap-4 lg:grid-cols-2">
              <article class="detail-card">
                <h3>Textures</h3>
                <div class="table-wrap">
                  <table><thead><tr><th>#</th><th>Name</th><th>Source</th><th>Sampler</th><th>Extensions</th></tr></thead>
                    <tbody>{#each inspection.json.textures ?? [] as texture, index}<tr><td>{index}</td><td>{texture.name ?? '—'}</td><td>{texture.source ?? 'extension'}</td><td>{texture.sampler ?? 'default'}</td><td><code>{compact(texture.extensions ?? {})}</code></td></tr>{/each}</tbody>
                  </table>
                </div>
              </article>
              <article class="detail-card">
                <h3>Images</h3>
                <div class="table-wrap">
                  <table><thead><tr><th>#</th><th>Name</th><th>MIME</th><th>bufferView / URI</th></tr></thead>
                    <tbody>{#each inspection.json.images ?? [] as image, index}<tr><td>{index}</td><td>{image.name ?? '—'}</td><td>{image.mimeType ?? 'inferred'}</td><td>{image.bufferView ?? image.uri ?? '—'}</td></tr>{/each}</tbody>
                  </table>
                </div>
              </article>
              <article class="detail-card lg:col-span-2">
                <h3>Samplers</h3>
                <pre class="code-block">{JSON.stringify(inspection.json.samplers ?? [], null, 2)}</pre>
              </article>
            </div>
          {:else if activeTab === 'data'}
            <div class="space-y-4">
              <article class="detail-card">
                <h3>Accessors ({inspection.summary.accessors})</h3>
                <div class="table-wrap max-h-[520px]">
                  <table><thead><tr><th>#</th><th>Name</th><th>Type</th><th>Component</th><th>Count</th><th>bufferView</th><th>Offset</th><th>Normalized</th><th>Min</th><th>Max</th><th>Sparse</th></tr></thead>
                    <tbody>{#each inspection.json.accessors ?? [] as accessor, index}<tr><td>{index}</td><td>{accessor.name ?? '—'}</td><td>{accessor.type}</td><td>{COMPONENT_TYPES[accessor.componentType] ?? accessor.componentType}</td><td>{accessor.count?.toLocaleString()}</td><td>{accessor.bufferView ?? '—'}</td><td>{accessor.byteOffset ?? 0}</td><td>{String(accessor.normalized ?? false)}</td><td><code>{compact(accessor.min ?? [])}</code></td><td><code>{compact(accessor.max ?? [])}</code></td><td>{accessor.sparse?.count ?? '—'}</td></tr>{/each}</tbody>
                  </table>
                </div>
              </article>
              <article class="detail-card">
                <h3>Buffer views ({inspection.summary.bufferViews})</h3>
                <div class="table-wrap max-h-[420px]">
                  <table><thead><tr><th>#</th><th>Name</th><th>Buffer</th><th>Offset</th><th>Length</th><th>Stride</th><th>Target</th><th>Extensions</th></tr></thead>
                    <tbody>{#each inspection.json.bufferViews ?? [] as view, index}<tr><td>{index}</td><td>{view.name ?? '—'}</td><td>{view.buffer ?? 0}</td><td>{view.byteOffset ?? 0}</td><td>{formatBytes(view.byteLength)}</td><td>{view.byteStride ?? '—'}</td><td>{BUFFER_TARGETS[view.target] ?? view.target ?? '—'}</td><td><code>{compact(view.extensions ?? {})}</code></td></tr>{/each}</tbody>
                  </table>
                </div>
              </article>
              <article class="detail-card"><h3>Buffers</h3><pre class="code-block">{JSON.stringify(inspection.json.buffers ?? [], null, 2)}</pre></article>
            </div>
          {:else if activeTab === 'animations'}
            <div class="space-y-3">
              {#each inspection.json.animations ?? [] as animation, index}
                <details class="detail-card" open>
                  <summary><strong>{index}: {animation.name ?? '(unnamed)'}</strong> <span class="ml-2 text-xs text-slate-500">{animation.channels?.length ?? 0} channels · {animation.samplers?.length ?? 0} samplers</span></summary>
                  <pre class="code-block mt-3">{JSON.stringify(animation, null, 2)}</pre>
                </details>
              {/each}
              {#if !inspection.summary.animations}<p class="text-slate-500">No animations.</p>{/if}
              <article class="detail-card"><h3>Skins</h3><pre class="code-block">{JSON.stringify(inspection.json.skins ?? [], null, 2)}</pre></article>
              <article class="detail-card"><h3>Cameras</h3><pre class="code-block">{JSON.stringify(inspection.json.cameras ?? [], null, 2)}</pre></article>
            </div>
          {:else if activeTab === 'extensions'}
            <div class="grid gap-4 lg:grid-cols-2">
              <article class="detail-card">
                <h3>Extensions used</h3>
                <div class="mt-3 flex flex-wrap gap-2">{#each inspection.json.extensionsUsed ?? [] as extension}<span class="badge">{extension}</span>{/each}</div>
              </article>
              <article class="detail-card">
                <h3>Extensions required</h3>
                <div class="mt-3 flex flex-wrap gap-2">{#each inspection.json.extensionsRequired ?? [] as extension}<span class="badge !border-amber-500/30 !text-amber-300">{extension}</span>{/each}</div>
              </article>
              <article class="detail-card lg:col-span-2"><h3>Root extension payload</h3><pre class="code-block">{JSON.stringify(inspection.json.extensions ?? {}, null, 2)}</pre></article>
              <article class="detail-card lg:col-span-2"><h3>All extension occurrences</h3><pre class="code-block">{JSON.stringify(extensionOccurrences(), null, 2)}</pre></article>
            </div>
          {:else if activeTab === 'diagnostics'}
            <div class="space-y-2">
              {#each inspection.diagnostics as diagnostic}
                <div class="diagnostic" class:error-diagnostic={diagnostic.level === 'error'} class:warning-diagnostic={diagnostic.level === 'warning'}>
                  <span class="badge">{diagnostic.level}</span>
                  <div><p>{diagnostic.message}</p>{#if diagnostic.path}<code>{diagnostic.path}</code>{/if}</div>
                </div>
              {/each}
              <p class="pt-3 text-xs text-slate-500">These are structural and portability checks, not a substitute for Khronos glTF Validator conformance testing.</p>
            </div>
          {:else if activeTab === 'raw'}
            <div>
              <div class="mb-3 flex gap-2">
                <input class="input max-w-md" bind:value={rawFilter} placeholder="Search raw JSON…" />
                <button class="button-secondary" onclick={copyRawJson}>{copied ? 'Copied' : 'Copy JSON'}</button>
              </div>
              {#if rawFilter}
                <p class="mb-2 text-xs text-slate-500">{rawMatches.length} matching lines</p>
                <pre class="code-block max-h-[620px]">{rawMatches.map((match) => `${String(match.number).padStart(6)}  ${match.line}`).join('\n')}</pre>
              {:else}
                <pre class="code-block max-h-[720px]">{inspection.jsonText}</pre>
              {/if}
            </div>
          {/if}
        </div>
      </section>

      {#if error}
        <div class="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-200">{error}</div>
      {/if}
    </main>
  {/if}
</div>

<style>
  :global(body) {
    margin: 0;
    background: #090d14;
  }
  :global(*) {
    scrollbar-color: #334155 #0f172a;
    scrollbar-width: thin;
  }
  :global(button), :global(input), :global(select) {
    font: inherit;
  }
  .panel {
    border: 1px solid rgb(30 41 59);
    border-radius: 0.75rem;
    background: rgb(15 23 42 / 0.72);
  }
  .panel-heading {
    display: flex;
    min-height: 3.4rem;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid rgb(30 41 59);
    padding: 0.7rem 0.85rem;
  }
  .panel-heading h2 {
    font-size: 0.82rem;
    font-weight: 650;
    color: rgb(226 232 240);
  }
  .panel-heading span {
    display: block;
    margin-top: 0.1rem;
    font-size: 0.65rem;
    color: rgb(100 116 139);
  }
  .metric-card {
    border: 1px solid rgb(30 41 59);
    border-radius: 0.6rem;
    background: rgb(15 23 42 / 0.7);
    padding: 0.65rem 0.75rem;
  }
  .metric-card span {
    display: block;
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: rgb(100 116 139);
  }
  .metric-card strong {
    display: block;
    margin-top: 0.1rem;
    font-family: ui-monospace, monospace;
    font-size: 1.05rem;
    color: rgb(226 232 240);
  }
  .button-primary, .button-secondary, .button-danger {
    border-radius: 0.45rem;
    padding: 0.48rem 0.75rem;
    font-size: 0.75rem;
    font-weight: 600;
  }
  .button-primary {
    border: 1px solid rgb(34 211 238 / 0.6);
    background: rgb(8 145 178);
    color: white;
  }
  .button-secondary {
    border: 1px solid rgb(51 65 85);
    background: rgb(30 41 59 / 0.75);
    color: rgb(203 213 225);
  }
  .button-danger {
    border: 1px solid rgb(127 29 29);
    background: rgb(69 10 10 / 0.45);
    color: rgb(254 202 202);
  }
  .button-primary:hover, .button-secondary:hover {
    border-color: rgb(34 211 238 / 0.8);
    color: white;
  }
  .input, .select {
    width: 100%;
    border: 1px solid rgb(51 65 85);
    border-radius: 0.4rem;
    background: rgb(2 6 23 / 0.75);
    padding: 0.43rem 0.55rem;
    color: rgb(226 232 240);
    font-size: 0.72rem;
    outline: none;
  }
  .select {
    width: auto;
  }
  .input:focus, .select:focus {
    border-color: rgb(34 211 238 / 0.8);
  }
  .selected-row {
    background: rgb(8 145 178 / 0.18);
    outline: 1px solid rgb(34 211 238 / 0.2);
  }
  .section-label {
    margin-bottom: 0.45rem;
    border-bottom: 1px solid rgb(30 41 59);
    padding-bottom: 0.25rem;
    font-size: 0.62rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: rgb(34 211 238);
  }
  .property-grid {
    display: grid;
    grid-template-columns: minmax(80px, 0.42fr) minmax(0, 1fr);
    gap: 0.3rem 0.65rem;
    font-size: 0.7rem;
  }
  .property-grid dt {
    color: rgb(100 116 139);
  }
  .property-grid dd {
    min-width: 0;
    overflow-wrap: anywhere;
    color: rgb(203 213 225);
  }
  .tab-button {
    flex: none;
    border: 1px solid transparent;
    border-radius: 0.4rem;
    padding: 0.38rem 0.65rem;
    color: rgb(148 163 184);
    font-size: 0.7rem;
  }
  .tab-button:hover {
    background: rgb(30 41 59);
    color: white;
  }
  .active-tab {
    border-color: rgb(34 211 238 / 0.3);
    background: rgb(8 145 178 / 0.18);
    color: rgb(103 232 249);
  }
  .detail-card {
    border: 1px solid rgb(30 41 59);
    border-radius: 0.6rem;
    background: rgb(2 6 23 / 0.38);
    padding: 0.8rem;
  }
  .detail-card h3, .detail-card summary {
    color: rgb(226 232 240);
    font-size: 0.78rem;
  }
  .badge {
    display: inline-flex;
    border: 1px solid rgb(51 65 85);
    border-radius: 999px;
    background: rgb(30 41 59 / 0.7);
    padding: 0.15rem 0.45rem;
    font-family: ui-monospace, monospace;
    font-size: 0.6rem;
    color: rgb(103 232 249);
  }
  .code-block {
    max-width: 100%;
    overflow: auto;
    border: 1px solid rgb(30 41 59);
    border-radius: 0.4rem;
    background: rgb(2 6 23);
    padding: 0.65rem;
    font-family: ui-monospace, monospace;
    font-size: 0.62rem;
    line-height: 1.55;
    color: rgb(148 163 184);
    white-space: pre;
  }
  .table-wrap {
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.67rem;
    white-space: nowrap;
  }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    background: rgb(15 23 42);
    color: rgb(100 116 139);
    text-align: left;
    font-weight: 600;
  }
  th, td {
    border-bottom: 1px solid rgb(30 41 59);
    padding: 0.45rem 0.55rem;
  }
  td {
    color: rgb(203 213 225);
  }
  td code {
    font-size: 0.61rem;
    color: rgb(125 211 252);
  }
  .diagnostic {
    display: flex;
    align-items: flex-start;
    gap: 0.65rem;
    border: 1px solid rgb(30 41 59);
    border-radius: 0.5rem;
    background: rgb(15 23 42 / 0.6);
    padding: 0.7rem;
    font-size: 0.75rem;
  }
  .diagnostic code {
    color: rgb(100 116 139);
    font-size: 0.65rem;
  }
  .error-diagnostic {
    border-color: rgb(239 68 68 / 0.35);
    background: rgb(127 29 29 / 0.12);
  }
  .warning-diagnostic {
    border-color: rgb(245 158 11 / 0.3);
    background: rgb(120 53 15 / 0.1);
  }
</style>
