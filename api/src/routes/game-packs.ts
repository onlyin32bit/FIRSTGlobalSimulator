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

export default app
