import { Hono } from 'hono'
import { eq } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import * as schema from '../db/schema'
import { gameServerHeartbeatSchema, isResponse, parseJson } from '../lib/validation'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()

async function hashKey(key: string) {
  const bytes = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(key))
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, '0')).join('')
}

app.post('/heartbeat', async (c) => {
  const key = c.req.header('X-Game-Server-Key')
  if (!key) return jsonError(c, 401, 'AUTH_FAILED', 'A game server key is required.')
  const body = await parseJson(c, gameServerHeartbeatSchema)
  if (isResponse(body)) return body
  const db = drizzle(c.env.DB, { schema })
  const server = await db.query.gameServers.findFirst({ where: eq(schema.gameServers.keyHash, await hashKey(key)) })
  if (!server || server.disabledAt) return jsonError(c, 401, 'AUTH_FAILED', 'The game server key is invalid or disabled.')
  const now = new Date()
  const updated = await db.update(schema.gameServers).set({
    activeUsers: body.activeUsers,
    activeMatches: body.activeMatches,
    slots: body.slots ?? server.slots,
    status: 'online',
    lastHeartbeatAt: now,
    updatedAt: now
  }).where(eq(schema.gameServers.id, server.id)).returning({ id: schema.gameServers.id, status: schema.gameServers.status })
  return jsonSuccess(c, { server: updated[0], heartbeatAt: now })
})

export { hashKey }
export default app
