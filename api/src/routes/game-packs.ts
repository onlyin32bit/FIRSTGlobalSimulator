import { Hono } from 'hono'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()

app.get('/', (c) => {
  return jsonSuccess(c, {
    packs: [
      { id: 'fgc-2026', name: 'Igniting Innovation', version: '1.0.0' }
    ]
  })
})

app.get('/:id/metadata', async (c) => {
  if (c.req.param('id') !== 'fgc-2026') return jsonError(c, 404, 'VALIDATION_ERROR', 'Game pack not found.')
  const origin = c.env.GAME_SERVER_ORIGIN?.replace(/^ws:/, 'http:').replace(/^wss:/, 'https:').replace(/\/$/, '')
  if (!origin) return jsonError(c, 503, 'INTERNAL_ERROR', 'Game server metadata is not configured.')
  try {
    const response = await fetch(`${origin}/pack/metadata`)
    const payload = await response.json() as { success?: boolean; data?: unknown; error?: { message?: string } }
    if (!response.ok || !payload.success || !payload.data) return jsonError(c, 503, 'INTERNAL_ERROR', payload.error?.message || 'Game pack metadata is unavailable.')
    return jsonSuccess(c, payload.data)
  } catch {
    return jsonError(c, 503, 'INTERNAL_ERROR', 'Unable to reach the game server metadata endpoint.')
  }
})

app.get('/:id/assets', (c) => {
  if (c.req.param('id') !== 'fgc-2026') return jsonError(c, 404, 'VALIDATION_ERROR', 'Game pack not found.')
  const base = new URL(c.req.url).origin
  const prefix = `${base}/api/game-packs/fgc-2026/assets`
  return jsonSuccess(c, {
    visual: `${prefix}/field.glb`,
    physics: `${prefix}/field.physics.json`,
    semantics: `${prefix}/field.semantics.json`
  })
})

app.get('/:id/assets/:asset', async (c) => {
  if (c.req.param('id') !== 'fgc-2026') return c.text('Game pack not found.', 404)
  const asset = c.req.param('asset')
  if (!['field.glb', 'field.physics.json', 'field.semantics.json'].includes(asset)) return c.text('Unknown pack asset.', 404)
  const origin = c.env.GAME_SERVER_ORIGIN?.replace(/^ws:/, 'http:').replace(/^wss:/, 'https:').replace(/\/$/, '')
  if (!origin) return c.text('Game server metadata is not configured.', 503)
  try {
    const upstream = `${origin}/pack/assets/${encodeURIComponent(asset)}`
    const response = await fetch(upstream)
    if (!response.ok) {
      const detail = (await response.text()).trim().slice(0, 200)
      return c.text(
        `Game server asset request failed: ${response.status} ${response.statusText}${detail ? ` — ${detail}` : ''}`,
        502
      )
    }
    return new Response(response.body, { headers: { 'content-type': response.headers.get('content-type') || 'application/octet-stream' } })
  } catch {
    return c.text('Unable to reach game server.', 503)
  }
})

export default app
