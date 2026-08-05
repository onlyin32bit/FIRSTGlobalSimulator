import { Hono, type Context } from 'hono'
import { eq } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import { verify } from 'hono/jwt'
import * as schema from '../db/schema'
import { gameServerHeartbeatSchema, gameServerTicketVerifySchema, isResponse, parseJson } from '../lib/validation'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()
type GameServerContext = Context<{ Bindings: Bindings }>

async function hashKey(key: string) {
  const bytes = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(key))
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, '0')).join('')
}

export async function authenticatedGameServer(c: GameServerContext) {
  const key = c.req.header('X-Game-Server-Key')
  if (!key) return null
  const db = drizzle(c.env.DB, { schema })
  return db.query.gameServers.findFirst({ where: eq(schema.gameServers.keyHash, await hashKey(key)) })
}

app.post('/heartbeat', async (c) => {
  const body = await parseJson(c, gameServerHeartbeatSchema)
  if (isResponse(body)) return body
  const server = await authenticatedGameServer(c)
  if (!server || server.disabledAt) return jsonError(c, 401, 'AUTH_FAILED', 'The game server key is invalid or disabled.')
  const db = drizzle(c.env.DB, { schema })
  const now = new Date()
  const updated = await db.update(schema.gameServers).set({
    activeUsers: body.activeUsers,
    activeMatches: body.activeMatches,
    maxUsers: body.maxUsers,
    maxMatches: body.maxMatches,
    slots: body.slots ?? server.slots,
    status: 'online',
    lastHeartbeatAt: now,
    updatedAt: now
  }).where(eq(schema.gameServers.id, server.id)).returning({ id: schema.gameServers.id, status: schema.gameServers.status })
  return jsonSuccess(c, { server: updated[0], heartbeatAt: now })
})

/**
 * Hosts never receive the signing secret. They authenticate with their
 * generated server key and ask the API to validate the short-lived ticket.
 */
app.post('/tickets/verify', async (c) => {
  const body = await parseJson(c, gameServerTicketVerifySchema)
  if (isResponse(body)) return body
  const server = await authenticatedGameServer(c)
  if (!server || server.disabledAt) return jsonError(c, 401, 'AUTH_FAILED', 'The game server key is invalid or disabled.')
  if (!c.env.JWT_SECRET) return jsonError(c, 503, 'INTERNAL_ERROR', 'Ticket verification is not configured.')
  try {
    const payload = await verify(body.ticket, c.env.JWT_SECRET, 'HS256')
    const sub = typeof payload.sub === 'string' ? payload.sub : null
    const matchId = typeof payload.match_id === 'string' ? payload.match_id : null
    const teamName = typeof payload.team_name === 'string' ? payload.team_name : null
    const displayName = typeof payload.display_name === 'string' ? payload.display_name : null
    const robotData = typeof payload.robot_data === 'string' ? payload.robot_data : null
    const exp = typeof payload.exp === 'number' ? payload.exp : null
    if (!sub || !matchId || !teamName || !displayName || !robotData || !exp) {
      return jsonError(c, 401, 'AUTH_FAILED', 'The ticket claims are incomplete.')
    }
    // test-match has no persisted match row; its WebSocket URL is still issued
    // from a selected healthy host. Persisted matches are bound strictly.
    if (matchId !== 'test-match') {
      const db = drizzle(c.env.DB, { schema })
      const match = await db.query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
      if (!match || match.status !== 'IN_PROGRESS' || match.gameServerId !== server.id) {
        return jsonError(c, 403, 'AUTH_FAILED', 'This ticket is not assigned to this game server.')
      }
    }
    return jsonSuccess(c, {
      claims: {
        sub,
        match_id: matchId,
        team_name: teamName,
        display_name: displayName,
        robot_data: robotData,
        slot_id: typeof payload.slot_id === 'string' ? payload.slot_id : undefined,
        role: typeof payload.role === 'string' ? payload.role : undefined,
        alliance: typeof payload.alliance === 'string' ? payload.alliance : undefined,
        exp
      }
    })
  } catch {
    return jsonError(c, 401, 'AUTH_FAILED', 'The ticket is invalid or expired.')
  }
})

export { hashKey }
export default app
