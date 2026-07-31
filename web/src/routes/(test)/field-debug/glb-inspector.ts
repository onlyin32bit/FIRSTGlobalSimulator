export interface GlbChunk {
  index: number
  type: string
  typeHex: string
  byteOffset: number
  dataOffset: number
  byteLength: number
}

export interface Diagnostic {
  level: 'error' | 'warning' | 'info'
  message: string
  path?: string
}

export interface GlbInspection {
  fileName: string
  fileSize: number
  magic: string
  version: number
  declaredLength: number
  chunks: GlbChunk[]
  json: Record<string, any>
  jsonText: string
  summary: {
    scenes: number
    nodes: number
    meshes: number
    primitives: number
    vertices: number
    triangles: number
    materials: number
    textures: number
    images: number
    animations: number
    animationChannels: number
    skins: number
    cameras: number
    lights: number
    accessors: number
    bufferViews: number
    buffers: number
    extensionsUsed: number
  }
  diagnostics: Diagnostic[]
}

const GLB_MAGIC = 0x46546c67
const JSON_CHUNK = 0x4e4f534a
const BIN_CHUNK = 0x004e4942

export const COMPONENT_TYPES: Record<number, string> = {
  5120: 'BYTE',
  5121: 'UNSIGNED_BYTE',
  5122: 'SHORT',
  5123: 'UNSIGNED_SHORT',
  5125: 'UNSIGNED_INT',
  5126: 'FLOAT'
}

export const BUFFER_TARGETS: Record<number, string> = {
  34962: 'ARRAY_BUFFER',
  34963: 'ELEMENT_ARRAY_BUFFER'
}

export const PRIMITIVE_MODES: Record<number, string> = {
  0: 'POINTS',
  1: 'LINES',
  2: 'LINE_LOOP',
  3: 'LINE_STRIP',
  4: 'TRIANGLES',
  5: 'TRIANGLE_STRIP',
  6: 'TRIANGLE_FAN'
}

function fourCc(value: number): string {
  return String.fromCharCode(
    value & 0xff,
    (value >> 8) & 0xff,
    (value >> 16) & 0xff,
    (value >> 24) & 0xff
  ).replace(/\0/g, '·')
}

function primitiveElementCount(primitive: any, accessors: any[]): number {
  const accessorIndex = primitive.indices ?? primitive.attributes?.POSITION
  return accessors[accessorIndex]?.count ?? 0
}

function primitiveTriangleCount(primitive: any, accessors: any[]): number {
  const count = primitiveElementCount(primitive, accessors)
  switch (primitive.mode ?? 4) {
    case 4:
      return Math.floor(count / 3)
    case 5:
    case 6:
      return Math.max(0, count - 2)
    default:
      return 0
  }
}

function inspectDocument(json: Record<string, any>, actualLength: number): Diagnostic[] {
  const diagnostics: Diagnostic[] = []
  const nodes = json.nodes ?? []
  const meshes = json.meshes ?? []
  const accessors = json.accessors ?? []
  const bufferViews = json.bufferViews ?? []
  const buffers = json.buffers ?? []

  if (json.asset?.version !== '2.0') {
    diagnostics.push({
      level: 'error',
      path: 'asset.version',
      message: `Expected glTF 2.0, found ${json.asset?.version ?? 'no version'}.`
    })
  }
  if (!json.scenes?.length) {
    diagnostics.push({ level: 'warning', path: 'scenes', message: 'No scenes are declared.' })
  }
  if (json.scene != null && !json.scenes?.[json.scene]) {
    diagnostics.push({
      level: 'error',
      path: 'scene',
      message: `Default scene index ${json.scene} does not exist.`
    })
  }

  nodes.forEach((node: any, index: number) => {
    if (node.mesh != null && !meshes[node.mesh]) {
      diagnostics.push({
        level: 'error',
        path: `nodes[${index}].mesh`,
        message: `Node ${index} references missing mesh ${node.mesh}.`
      })
    }
    for (const child of node.children ?? []) {
      if (!nodes[child]) {
        diagnostics.push({
          level: 'error',
          path: `nodes[${index}].children`,
          message: `Node ${index} references missing child ${child}.`
        })
      }
    }
  })

  accessors.forEach((accessor: any, index: number) => {
    if (accessor.bufferView != null && !bufferViews[accessor.bufferView]) {
      diagnostics.push({
        level: 'error',
        path: `accessors[${index}].bufferView`,
        message: `Accessor ${index} references missing bufferView ${accessor.bufferView}.`
      })
    }
    if (!COMPONENT_TYPES[accessor.componentType]) {
      diagnostics.push({
        level: 'warning',
        path: `accessors[${index}].componentType`,
        message: `Accessor ${index} uses unknown component type ${accessor.componentType}.`
      })
    }
    if (accessor.count === 0) {
      diagnostics.push({
        level: 'warning',
        path: `accessors[${index}].count`,
        message: `Accessor ${index} is empty.`
      })
    }
  })

  bufferViews.forEach((view: any, index: number) => {
    if (!buffers[view.buffer ?? 0]) {
      diagnostics.push({
        level: 'error',
        path: `bufferViews[${index}].buffer`,
        message: `bufferView ${index} references missing buffer ${view.buffer ?? 0}.`
      })
    }
  })

  for (const extension of json.extensionsRequired ?? []) {
    if (!(json.extensionsUsed ?? []).includes(extension)) {
      diagnostics.push({
        level: 'warning',
        path: 'extensionsRequired',
        message: `Required extension ${extension} is not listed in extensionsUsed.`
      })
    }
  }

  const unnamedNodes = nodes.filter((node: any) => !node.name).length
  if (unnamedNodes > 0) {
    diagnostics.push({
      level: 'info',
      path: 'nodes',
      message: `${unnamedNodes} of ${nodes.length} nodes have no name.`
    })
  }

  const unnamedMaterials = (json.materials ?? []).filter((material: any) => !material.name).length
  if (unnamedMaterials > 0) {
    diagnostics.push({
      level: 'info',
      path: 'materials',
      message: `${unnamedMaterials} materials have no name.`
    })
  }

  for (const [index, buffer] of buffers.entries()) {
    if (buffer.uri) {
      diagnostics.push({
        level: 'warning',
        path: `buffers[${index}].uri`,
        message: `Buffer ${index} is external (${buffer.uri}); a standalone GLB should embed it.`
      })
    }
  }
  for (const [index, image] of (json.images ?? []).entries()) {
    if (image.uri && !image.uri.startsWith('data:')) {
      diagnostics.push({
        level: 'warning',
        path: `images[${index}].uri`,
        message: `Image ${index} is external (${image.uri}).`
      })
    }
  }

  diagnostics.push({
    level: 'info',
    message: `Parsed ${formatBytes(actualLength)} of GLB data successfully.`
  })
  return diagnostics
}

export function inspectGlb(buffer: ArrayBuffer, fileName: string): GlbInspection {
  if (buffer.byteLength < 20) throw new Error('File is too small to be a valid GLB.')

  const view = new DataView(buffer)
  const magicValue = view.getUint32(0, true)
  if (magicValue !== GLB_MAGIC) {
    throw new Error('Not a binary glTF file. Expected GLB magic "glTF".')
  }

  const version = view.getUint32(4, true)
  const declaredLength = view.getUint32(8, true)
  if (declaredLength > buffer.byteLength) {
    throw new Error(
      `GLB declares ${declaredLength} bytes but the selected file has ${buffer.byteLength}.`
    )
  }

  const chunks: GlbChunk[] = []
  let offset = 12
  let jsonText = ''

  while (offset + 8 <= declaredLength) {
    const byteLength = view.getUint32(offset, true)
    const typeValue = view.getUint32(offset + 4, true)
    const dataOffset = offset + 8
    if (dataOffset + byteLength > declaredLength) {
      throw new Error(`Chunk ${chunks.length} extends beyond the declared GLB length.`)
    }

    chunks.push({
      index: chunks.length,
      type: typeValue === JSON_CHUNK ? 'JSON' : typeValue === BIN_CHUNK ? 'BIN' : fourCc(typeValue),
      typeHex: `0x${typeValue.toString(16).padStart(8, '0')}`,
      byteOffset: offset,
      dataOffset,
      byteLength
    })

    if (typeValue === JSON_CHUNK) {
      jsonText = new TextDecoder().decode(new Uint8Array(buffer, dataOffset, byteLength)).trim()
    }
    offset = dataOffset + byteLength
  }

  if (!jsonText) throw new Error('GLB has no JSON chunk.')
  const json = JSON.parse(jsonText) as Record<string, any>
  const accessors = json.accessors ?? []
  const primitives = (json.meshes ?? []).flatMap((mesh: any) => mesh.primitives ?? [])
  const vertices = primitives.reduce(
    (total: number, primitive: any) =>
      total + (accessors[primitive.attributes?.POSITION]?.count ?? 0),
    0
  )
  const triangles = primitives.reduce(
    (total: number, primitive: any) => total + primitiveTriangleCount(primitive, accessors),
    0
  )

  return {
    fileName,
    fileSize: buffer.byteLength,
    magic: fourCc(magicValue),
    version,
    declaredLength,
    chunks,
    json,
    jsonText: JSON.stringify(json, null, 2),
    summary: {
      scenes: json.scenes?.length ?? 0,
      nodes: json.nodes?.length ?? 0,
      meshes: json.meshes?.length ?? 0,
      primitives: primitives.length,
      vertices,
      triangles,
      materials: json.materials?.length ?? 0,
      textures: json.textures?.length ?? 0,
      images: json.images?.length ?? 0,
      animations: json.animations?.length ?? 0,
      animationChannels: (json.animations ?? []).reduce(
        (total: number, animation: any) => total + (animation.channels?.length ?? 0),
        0
      ),
      skins: json.skins?.length ?? 0,
      cameras: json.cameras?.length ?? 0,
      lights: json.extensions?.KHR_lights_punctual?.lights?.length ?? 0,
      accessors: accessors.length,
      bufferViews: json.bufferViews?.length ?? 0,
      buffers: json.buffers?.length ?? 0,
      extensionsUsed: json.extensionsUsed?.length ?? 0
    },
    diagnostics: inspectDocument(json, buffer.byteLength)
  }
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return '—'
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  const value = bytes / 1024 ** unitIndex
  return `${value.toFixed(value >= 100 || unitIndex === 0 ? 0 : value >= 10 ? 1 : 2)} ${units[unitIndex]}`
}

export function downloadJson(fileName: string, value: unknown): void {
  const blob = new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = fileName
  anchor.click()
  URL.revokeObjectURL(url)
}
