import { Hono, type Context } from 'hono'
import { and, eq, inArray, sql } from 'drizzle-orm'
import type { BatchItem } from 'drizzle-orm/batch'
import { drizzle } from 'drizzle-orm/d1'
import { verify } from 'hono/jwt'
import * as schema from '../db/schema'
import { gameServerHeartbeatSchema, gameServerMatchCompletionSchema, gameServerMatchEventsSchema, gameServerTicketVerifySchema, isResponse, parseJson } from '../lib/validation'
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

function lobbyFor(c: GameServerContext, matchId: string) {
  return c.env.MATCH_LOBBY.getByName(matchId)
}

async function assignedMatch(c: GameServerContext, matchId: string) {
  const server = await authenticatedGameServer(c)
  if (!server || server.disabledAt) return { server: null, match: null }
  const match = await drizzle(c.env.DB, { schema }).query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
  if (!match || match.gameServerId !== server.id) return { server, match: null }
  return { server, match }
}

/**
 * D1 batches every statement into a single round-trip and transaction. All
 * heartbeat writes are pushed into one batch, and inserts are chunked to stay
 * under D1's 100 bound-parameters-per-statement limit.
 */
function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = []
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size))
  return out
}

app.post('/heartbeat', async (c) => {
  const body = await parseJson(c, gameServerHeartbeatSchema)
  if (isResponse(body)) return body
  const server = await authenticatedGameServer(c)
  if (!server || server.disabledAt) return jsonError(c, 401, 'AUTH_FAILED', 'The game server key is invalid or disabled.')
  const db = drizzle(c.env.DB, { schema })
  const now = new Date()

  const statements: BatchItem<'sqlite'>[] = []
  let serverUpdateIndex = -1
  let currentMatchesIndex = -1
  let pendingIndex = -1

  const results = body.commandResults ?? []
  for (const group of chunk(results.filter((r) => r.ok), 40)) {
    statements.push(db.update(schema.gameServerCommands).set({
      status: 'completed',
      error: null,
      completedAt: now
    }).where(and(inArray(schema.gameServerCommands.id, group.map((r) => r.id)), eq(schema.gameServerCommands.serverId, server.id))))
  }
  for (const result of results.filter((r) => !r.ok)) {
    statements.push(db.update(schema.gameServerCommands).set({
      status: 'failed',
      error: result.error ?? 'Host rejected the command.',
      completedAt: now
    }).where(and(eq(schema.gameServerCommands.id, result.id), eq(schema.gameServerCommands.serverId, server.id))))
  }

  serverUpdateIndex = statements.length
  statements.push(db.update(schema.gameServers).set({
    activeUsers: body.activeUsers,
    activeMatches: body.activeMatches,
    maxUsers: body.maxUsers,
    maxMatches: body.maxMatches,
    slots: body.slots ?? server.slots,
    status: 'online',
    lastHeartbeatAt: now,
    updatedAt: now,
    runtimeJson: body.runtime ? JSON.stringify(body.runtime) : server.runtimeJson
  }).where(eq(schema.gameServers.id, server.id)).returning({ id: schema.gameServers.id, status: schema.gameServers.status }))

  const instances = body.instances ?? []
  for (const group of chunk(instances, 12)) {
    statements.push(db.insert(schema.gameServerInstances).values(group.map((instance) => ({
      id: `${server.id}:${instance.machineId}`,
      serverId: server.id,
      machineId: instance.machineId,
      appName: instance.appName ?? null,
      region: instance.region ?? null,
      privateIp: instance.privateIp ?? null,
      discoveredAt: now,
      lastSeenAt: now
    }))).onConflictDoUpdate({
      target: [schema.gameServerInstances.serverId, schema.gameServerInstances.machineId],
      set: { appName: sql`excluded.appName`, region: sql`excluded.region`, privateIp: sql`excluded.privateIp`, lastSeenAt: now }
    }))
  }

  const matches = body.matches
  if (matches) {
    currentMatchesIndex = statements.length
    statements.push(db.select({ matchId: schema.gameServerRuntimeMatches.matchId })
      .from(schema.gameServerRuntimeMatches)
      .where(eq(schema.gameServerRuntimeMatches.serverId, server.id)))
    for (const group of chunk(matches, 8)) {
      statements.push(db.insert(schema.gameServerRuntimeMatches).values(group.map((match) => ({
        id: `${server.id}:${match.id}`,
        serverId: server.id,
        matchId: match.id,
        players: match.players,
        objects: match.objects,
        contacts: match.contacts,
        tick: match.tick,
        tps: match.tps,
        physicsTickMs: match.physicsTickMs,
        physicsLoadPercent: match.physicsLoadPercent,
        clockDriftMs: match.clockDriftMs,
        updatedAt: now
      }))).onConflictDoUpdate({
        target: [schema.gameServerRuntimeMatches.serverId, schema.gameServerRuntimeMatches.matchId],
        set: {
          players: sql`excluded.players`,
          objects: sql`excluded.objects`,
          contacts: sql`excluded.contacts`,
          tick: sql`excluded.tick`,
          tps: sql`excluded.tps`,
          physicsTickMs: sql`excluded.physicsTickMs`,
          physicsLoadPercent: sql`excluded.physicsLoadPercent`,
          clockDriftMs: sql`excluded.clockDriftMs`,
          updatedAt: now
        }
      }))
    }
  }

  pendingIndex = statements.length
  statements.push(db.select().from(schema.gameServerCommands)
    .where(and(eq(schema.gameServerCommands.serverId, server.id), eq(schema.gameServerCommands.status, 'pending')))
    .limit(50))

  const batchResults = await db.batch(statements as [BatchItem<'sqlite'>, ...BatchItem<'sqlite'>[]])

  const updated = batchResults[serverUpdateIndex] as { id: string; status: string }[]
  const commands = batchResults[pendingIndex] as typeof schema.gameServerCommands.$inferSelect[]

  const postStatements: BatchItem<'sqlite'>[] = []
  if (matches) {
    const present = new Set(matches.map((match) => match.id))
    const removed = (batchResults[currentMatchesIndex] as { matchId: string }[])
      .map((row) => row.matchId)
      .filter((id) => !present.has(id))
    for (const group of chunk(removed, 90)) {
      postStatements.push(db.delete(schema.gameServerRuntimeMatches)
        .where(and(eq(schema.gameServerRuntimeMatches.serverId, server.id), inArray(schema.gameServerRuntimeMatches.matchId, group))))
    }
  }
  for (const group of chunk(commands, 50)) {
    postStatements.push(db.update(schema.gameServerCommands).set({ status: 'delivered', deliveredAt: now })
      .where(and(inArray(schema.gameServerCommands.id, group.map((command) => command.id)), eq(schema.gameServerCommands.status, 'pending'))))
  }
  if (postStatements.length > 0) await db.batch(postStatements as [BatchItem<'sqlite'>, ...BatchItem<'sqlite'>[]])

  return jsonSuccess(c, {
    server: updated[0],
    heartbeatAt: now,
    commands: commands.map((command) => ({ id: command.id, type: command.type, ...JSON.parse(command.payload) }))
  })
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
    const robotId = typeof payload.robot_id === 'string' ? payload.robot_id : undefined
    const robotRevision = typeof payload.robot_revision === 'number' ? payload.robot_revision : undefined
    const exp = typeof payload.exp === 'number' ? payload.exp : null
    if (!sub || !matchId || !teamName || !displayName || !robotData || !exp) {
      return jsonError(c, 401, 'AUTH_FAILED', 'The ticket claims are incomplete.')
    }
    const db = drizzle(c.env.DB, { schema })
    const match = await db.query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
    if (!match || match.status !== 'IN_PROGRESS' || match.gameServerId !== server.id) {
      return jsonError(c, 403, 'AUTH_FAILED', 'This ticket is not assigned to this game server.')
    }
    if (matchId !== 'arena') {
      const bootstrap = await lobbyFor(c, matchId).getBootstrap()
      const participant = bootstrap.participants.find((entry) => entry.userId === sub)
      const role = typeof payload.role === 'string' ? payload.role : undefined
      const slotId = typeof payload.slot_id === 'string' ? payload.slot_id : undefined
      const alliance = typeof payload.alliance === 'string' ? payload.alliance : undefined
      const isHost = role === 'host' && sub === bootstrap.hostId
      const isObserver = role === 'spectator' || role === 'reviewer' || isHost
      if (!isObserver && (!participant || participant.role !== role || participant.slotId !== slotId || participant.alliance !== alliance || participant.robotId !== (robotId ?? null) || participant.robotRevision !== (robotRevision ?? null))) {
        return jsonError(c, 403, 'AUTH_FAILED', 'This ticket no longer matches the locked roster.')
      }
    }
    return jsonSuccess(c, {
      claims: {
        sub,
        match_id: matchId,
        team_name: teamName,
        display_name: displayName,
        robot_data: robotData,
        robot_id: robotId,
        robot_revision: robotRevision,
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

app.get('/matches/:id/bootstrap', async (c) => {
  const { server, match } = await assignedMatch(c, c.req.param('id'))
  if (!server) return jsonError(c, 401, 'AUTH_FAILED', 'The game server key is invalid or disabled.')
  if (!match || match.status !== 'IN_PROGRESS') return jsonError(c, 403, 'AUTH_FAILED', 'This match is not assigned to this game server.')
  if (match.id === 'arena') {
    return jsonSuccess(c, { bootstrap: {
      matchId: match.id, hostId: match.hostId, assignedServerId: server.id, gamePackId: match.gamePackId,
      gamePackVersion: match.packVersion || '1.0.0', matchSeed: match.matchSeed || 0,
      startsAt: match.startsAt?.getTime() || Date.now(), durationSeconds: 86_400, maxPlayers: match.maxPlayers,
      scenarioId: 'open-arena', visibility: 'public', openArena: true, participants: []
    } })
  }
  try {
    const bootstrap = await lobbyFor(c, match.id).getBootstrap()
    if (bootstrap.assignedServerId !== server.id) return jsonError(c, 403, 'AUTH_FAILED', 'This server does not own the locked roster.')
    return jsonSuccess(c, { bootstrap })
  } catch {
    return jsonError(c, 409, 'LOBBY_INVALID_STATE', 'The match has no locked bootstrap.')
  }
})

app.post('/matches/:id/events', async (c) => {
  const body = await parseJson(c, gameServerMatchEventsSchema)
  if (isResponse(body)) return body
  const { server, match } = await assignedMatch(c, c.req.param('id'))
  if (!server) return jsonError(c, 401, 'AUTH_FAILED', 'The game server key is invalid or disabled.')
  if (!match || match.status !== 'IN_PROGRESS' || body.events.some((event) => event.gamePackVersion !== match.packVersion)) return jsonError(c, 403, 'AUTH_FAILED', 'These events are not valid for the assigned match.')
  const db = drizzle(c.env.DB, { schema })
  const statements: BatchItem<'sqlite'>[] = body.events.map((event) => db.insert(schema.matchEvents).values({
    id: `${match.id}:${event.eventId}`, matchId: match.id, eventId: event.eventId, tick: event.tick,
    kind: event.kind, payloadJson: JSON.stringify(event.payload), gamePackVersion: event.gamePackVersion, createdAt: new Date()
  }).onConflictDoNothing())
  if (statements.length > 0) {
    await db.batch(statements as [BatchItem<'sqlite'>, ...BatchItem<'sqlite'>[]])
  }
  return jsonSuccess(c, { accepted: body.events.length })
})

app.post('/matches/:id/complete', async (c) => {
  const body = await parseJson(c, gameServerMatchCompletionSchema)
  if (isResponse(body)) return body
  const { server, match } = await assignedMatch(c, c.req.param('id'))
  if (!server) return jsonError(c, 401, 'AUTH_FAILED', 'The game server key is invalid or disabled.')
  if (!match || match.packVersion !== body.gamePackVersion) return jsonError(c, 403, 'AUTH_FAILED', 'This completion is not valid for the assigned match.')
  const db = drizzle(c.env.DB, { schema })
  const events = await db.select().from(schema.matchEvents).where(eq(schema.matchEvents.matchId, match.id))
  const replayKey = `matches/${match.id}/events.json`
  await c.env.OBJECT_STORAGE.put(replayKey, JSON.stringify({ match: { id: match.id, gamePackId: match.gamePackId, gamePackVersion: body.gamePackVersion, seed: match.matchSeed }, completion: body, events }), { httpMetadata: { contentType: 'application/json' } })
  const cancelled = /cancel|reset|fatal/i.test(body.reason)
  await db.update(schema.matches).set({
    status: cancelled ? 'CANCELLED' : 'FINISHED', completedAt: new Date(), completionReason: body.reason,
    resultJson: JSON.stringify(body.result), replayKey, updatedAt: new Date(),
    cancelledAt: cancelled ? new Date() : match.cancelledAt, cancelReason: cancelled ? body.reason : match.cancelReason
  }).where(eq(schema.matches.id, match.id))
  if (match.id !== 'arena') await lobbyFor(c, match.id).complete(body.reason, cancelled)
  return jsonSuccess(c, { accepted: true, replayKey })
})

export { hashKey }
export default app
