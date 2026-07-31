export interface RuntimeGeometry {
  name: string
  uuid: string
  type: string
  drawRange: { start: number; count: number }
  groups: Array<{ start: number; count: number; materialIndex?: number }>
  indexCount: number
  attributes: Record<
    string,
    {
      itemSize: number
      count: number
      normalized: boolean
      arrayType: string
      bytes: number
    }
  >
  morphAttributes: string[]
}

export interface RuntimeMaterial {
  uuid: string
  name: string
  type: string
  transparent?: boolean
  opacity?: number
  alphaTest?: number
  depthTest?: boolean
  depthWrite?: boolean
  side?: number
  color?: string
  emissive?: string
  metalness?: number
  roughness?: number
  map?: string | null
}

export interface RuntimeNode {
  uuid: string
  parentUuid: string | null
  children: string[]
  name: string
  type: string
  visible: boolean
  renderOrder: number
  position: number[]
  rotation: (number | string)[]
  quaternion: number[]
  scale: number[]
  matrix: number[]
  matrixWorld: number[]
  worldPosition: number[]
  bounds: { min: number[]; max: number[]; size: number[] } | null
  userData: Record<string, unknown>
  geometry?: RuntimeGeometry
  materials?: RuntimeMaterial[]
}

export interface RendererDetails {
  renderer: string
  vendor: string
  webglVersion: string
  maxTextureSize: number
  maxCubemapSize: number
  maxSamples: number
  maxAnisotropy: number
  precision: string
}

export interface RuntimeModelReport {
  rootUuids: string[]
  nodes: RuntimeNode[]
  animations: Array<{ name: string; duration: number; tracks: number }>
  bounds: { min: number[]; max: number[]; size: number[]; center: number[] }
  renderer: RendererDetails
}

export interface RenderStats {
  calls: number
  triangles: number
  points: number
  lines: number
  geometries: number
  textures: number
  programs: number
}
