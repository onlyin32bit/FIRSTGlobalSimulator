import { Hono, type Context } from 'hono'
import { authenticatedGameServer } from './game-servers'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const PACK_ID = 'fgc-2026'
const ALLOWED_ASSETS = new Set(['field.glb', 'field.physics.json', 'field.semantics.json'])

type Manifest = {
  id: string
  name: string
  version: string
  engineVersion: string
  field: { visual: string; physics: string; semantics: string }
  objects: unknown[]
  phases: unknown[]
  scripts: Record<string, string>
}

type Bounds = { id: string; min: [number, number, number]; max: [number, number, number] }
type OrientedBounds = Bounds & {
  center: [number, number, number]
  halfExtents: [number, number, number]
  axes: [[number, number, number], [number, number, number], [number, number, number]]
}

const app = new Hono<{ Bindings: Bindings }>()
type PackContext = Context<{ Bindings: Bindings }>

function packPath(path: string) {
  return `/${PACK_ID}/${path.replace(/^\/+/, '')}`
}

async function getPackAsset(c: PackContext, path: string) {
  const url = new URL(c.req.url)
  url.pathname = packPath(path)
  url.search = ''
  return c.env.PACK_ASSETS.fetch(new Request(url))
}

async function readPackJson<T>(c: PackContext, path: string): Promise<T> {
  const response = await getPackAsset(c, path)
  if (!response.ok) throw new Error(`Pack asset ${path} is unavailable (${response.status})`)
  return response.json<T>()
}

async function readPackText(c: PackContext, path: string): Promise<string> {
  const response = await getPackAsset(c, path)
  if (!response.ok) throw new Error(`Pack asset ${path} is unavailable (${response.status})`)
  return response.text()
}

function transformPoint(matrix: number[] | undefined, point: [number, number, number]): [number, number, number] {
  if (!matrix || matrix.length < 16) return point
  return [
    matrix[0] * point[0] + matrix[1] * point[1] + matrix[2] * point[2] + matrix[3],
    matrix[4] * point[0] + matrix[5] * point[1] + matrix[6] * point[2] + matrix[7],
    matrix[8] * point[0] + matrix[9] * point[1] + matrix[10] * point[2] + matrix[11]
  ]
}

function orientedBoundsForNode(node: any, scene: any): OrientedBounds | null {
  const meshIndex = node?.meshes?.[0]
  const vertices = Number.isInteger(meshIndex) ? scene?.meshes?.[meshIndex]?.vertices : null
  if (!node?.name || !Array.isArray(vertices)) return null
  const matrix = Array.isArray(node.transformation) && node.transformation.length >= 16 ? node.transformation : null
  if (!matrix) return null
  const localMin: [number, number, number] = [Infinity, Infinity, Infinity]
  const localMax: [number, number, number] = [-Infinity, -Infinity, -Infinity]
  const min: [number, number, number] = [Infinity, Infinity, Infinity]
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity]
  for (let index = 0; index + 2 < vertices.length; index += 3) {
    const local: [number, number, number] = [vertices[index], vertices[index + 1], vertices[index + 2]]
    for (let axis = 0; axis < 3; axis += 1) {
      localMin[axis] = Math.min(localMin[axis], local[axis])
      localMax[axis] = Math.max(localMax[axis], local[axis])
    }
    const point = transformPoint(matrix, local)
    for (let axis = 0; axis < 3; axis += 1) {
      min[axis] = Math.min(min[axis], point[axis])
      max[axis] = Math.max(max[axis], point[axis])
    }
  }
  if (!Number.isFinite(min[0])) return null
  const center = transformPoint(matrix, [
    (localMin[0] + localMax[0]) * 0.5,
    (localMin[1] + localMax[1]) * 0.5,
    (localMin[2] + localMax[2]) * 0.5
  ])
  const rawAxes = [
    [matrix[0], matrix[4], matrix[8]],
    [matrix[1], matrix[5], matrix[9]],
    [matrix[2], matrix[6], matrix[10]]
  ] as [[number, number, number], [number, number, number], [number, number, number]]
  const axes = rawAxes.map((axis) => {
    const length = Math.hypot(axis[0], axis[1], axis[2]) || 1
    return [axis[0] / length, axis[1] / length, axis[2] / length] as [number, number, number]
  }) as OrientedBounds['axes']
  const halfExtents = rawAxes.map((axis, index) => {
    const length = Math.hypot(axis[0], axis[1], axis[2]) || 1
    return ((localMax[index] - localMin[index]) * 0.5) * length
  }) as [number, number, number]
  return { id: node.name, min, max, center, halfExtents, axes }
}

function extrudeThinBounds(bounds: OrientedBounds): OrientedBounds {
  const halfExtents = [...bounds.halfExtents] as OrientedBounds['halfExtents']
  for (let axis = 0; axis < 3; axis += 1) {
    halfExtents[axis] = Math.max(halfExtents[axis], 0.025)
  }
  const min = [...bounds.center] as Bounds['min']
  const max = [...bounds.center] as Bounds['max']
  for (let worldAxis = 0; worldAxis < 3; worldAxis += 1) {
    const radius = bounds.axes.reduce((sum, axis, localAxis) => sum + Math.abs(axis[worldAxis]) * halfExtents[localAxis], 0)
    min[worldAxis] -= radius
    max[worldAxis] += radius
  }
  return { ...bounds, min, max, halfExtents }
}

/**
 * Public, renderer/debug-only field description. It deliberately omits Rhai
 * sources and runtime configuration; those are supplied only to game hosts by
 * the `runtime` endpoint.
 */
function buildPublicFieldDefinition(physics: any, semantics: any) {
  const physicsNodes = Array.isArray(physics?.rootnode?.children) ? physics.rootnode.children : []
  const authored = physicsNodes.map((node: any) => orientedBoundsForNode(node, physics)).filter(Boolean) as OrientedBounds[]
  const riser = authored.find((bounds) => bounds.id === 'RISER.001')
  const colliders = authored
    .filter(({ id, min, max }) => id !== 'GUARD_RAIL.001' && id !== 'RISER.001' && max[0] - min[0] <= 2.5 && max[2] - min[2] <= 2.5)
    .map(extrudeThinBounds)
  const anchors: Record<string, [number, number, number]> = {}
  const triggers: Bounds[] = []
  const semanticNodes = Array.isArray(semantics?.rootnode?.children) ? semantics.rootnode.children : []
  for (const node of semanticNodes) {
    if (Array.isArray(node?.meshes)) {
      const bounds = orientedBoundsForNode(node, semantics)
      if (bounds) triggers.push(bounds)
      continue
    }
    if (typeof node?.name === 'string' && Array.isArray(node?.transformation)) {
      anchors[node.name] = [node.transformation[3], node.transformation[7], node.transformation[11]]
    }
  }
  return {
    colliders,
    anchors,
    triggers,
    floorHeightM: riser?.max[1] ?? 0,
    boundary: riser ? { min: riser.min, max: riser.max } : { min: [-8, 0, -8], max: [8, 0, 8] }
  }
}

async function loadPack(c: PackContext) {
  const manifest = await readPackJson<Manifest>(c, 'manifest.json')
  if (manifest.id !== PACK_ID) throw new Error('The deployed pack manifest has an unexpected id.')
  const [fieldPhysics, fieldSemantics] = await Promise.all([
    readPackJson<unknown>(c, manifest.field.physics),
    readPackJson<unknown>(c, manifest.field.semantics)
  ])
  return { manifest, fieldPhysics, fieldSemantics }
}

app.get('/', (c) => jsonSuccess(c, { packs: [{ id: PACK_ID, name: 'Igniting Innovation', version: '1.0.0' }] }))

app.get('/:id/metadata', async (c) => {
  if (c.req.param('id') !== PACK_ID) return jsonError(c, 404, 'VALIDATION_ERROR', 'Game pack not found.')
  try {
    const { manifest, fieldPhysics, fieldSemantics } = await loadPack(c)
    return jsonSuccess(c, {
      manifest,
      // Script source is intentionally absent: browsers do not execute rules.
      scripts: [],
      fieldDefinition: buildPublicFieldDefinition(fieldPhysics, fieldSemantics)
    })
  } catch (error) {
    return jsonError(c, 503, 'INTERNAL_ERROR', error instanceof Error ? error.message : 'Game pack metadata is unavailable.')
  }
})

/** Runtime-only snapshot consumed by a game host before it creates matches. */
app.get('/:id/runtime', async (c) => {
  if (c.req.param('id') !== PACK_ID) return jsonError(c, 404, 'VALIDATION_ERROR', 'Game pack not found.')
  const server = await authenticatedGameServer(c)
  if (!server || server.disabledAt) return jsonError(c, 401, 'AUTH_FAILED', 'A valid game server key is required for runtime pack data.')
  try {
    const { manifest, fieldPhysics, fieldSemantics } = await loadPack(c)
    const scripts = Object.fromEntries(await Promise.all(
      Object.entries(manifest.scripts).map(async ([name, path]) => [path, await readPackText(c, path)] as const)
    ))
    return jsonSuccess(c, { manifest, fieldPhysics, fieldSemantics, scripts })
  } catch (error) {
    return jsonError(c, 503, 'INTERNAL_ERROR', error instanceof Error ? error.message : 'Game pack runtime snapshot is unavailable.')
  }
})

app.get('/:id/assets', (c) => {
  if (c.req.param('id') !== PACK_ID) return jsonError(c, 404, 'VALIDATION_ERROR', 'Game pack not found.')
  const base = new URL(c.req.url).origin
  const prefix = `${base}/api/game-packs/${PACK_ID}/assets`
  return jsonSuccess(c, { visual: `${prefix}/field.glb`, physics: `${prefix}/field.physics.json`, semantics: `${prefix}/field.semantics.json` })
})

app.get('/:id/assets/:asset', async (c) => {
  if (c.req.param('id') !== PACK_ID) return c.text('Game pack not found.', 404)
  const asset = c.req.param('asset')
  if (!ALLOWED_ASSETS.has(asset)) return c.text('Unknown pack asset.', 404)
  try {
    const response = await getPackAsset(c, asset)
    if (!response.ok) return c.text('Pack asset not found.', response.status === 404 ? 404 : 503)
    const headers = new Headers(response.headers)
    headers.set('cache-control', asset === 'field.glb' ? 'public, max-age=86400' : 'public, max-age=300')
    return new Response(response.body, { status: response.status, headers })
  } catch {
    return c.text('Game pack asset service is unavailable.', 503)
  }
})

export default app
