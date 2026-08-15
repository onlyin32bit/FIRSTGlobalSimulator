import { Hono } from 'hono'
import { desc, eq } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import { sign } from 'hono/jwt'
import * as schema from '../db/schema'
import { isResponse, lobbyReadySchema, lobbySlotSchema, matchSchema, parseJson } from '../lib/validation'
import type { LobbySlotId, LobbyUser, MatchBootstrap } from '../match-lobby'
import { requireAdmin, requireUser } from '../middleware'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()
const PACK_ROBOT_ID_PREFIX = 'pack:'

type PackManifest = {
  version?: unknown
  defaultRobot?: unknown
  robots?: Record<string, { name?: unknown }>
}

type DriverRobot = {
  id: string
  data: string
  revision: number | null
}

function gameServerUrl(origin: string, matchId: string, ticket: string) {
  const gameServerOrigin = origin.replace(/^http:/, 'ws:').replace(/^https:/, 'wss:').replace(/\/$/, '')
  return `${gameServerOrigin}/ws/match/${encodeURIComponent(matchId)}?ticket=${encodeURIComponent(ticket)}`
}

async function chooseGameServer(c: Parameters<typeof requireUser>[0], matchId: string) {
  const db = drizzle(c.env.DB, { schema })
  const existingMatch = await db.query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
  if (existingMatch?.gameServerId) {
    const existing = await db.query.gameServers.findFirst({ where: eq(schema.gameServers.id, existingMatch.gameServerId) })
    if (existing && !existing.disabledAt) return existing
  }
  const servers = await db.select().from(schema.gameServers).orderBy(desc(schema.gameServers.activeMatches))
  const now = Date.now()
  const server = servers
    .filter((candidate) => !candidate.disabledAt && candidate.lastHeartbeatAt && now - candidate.lastHeartbeatAt.getTime() < 30_000)
    .filter((candidate) => candidate.activeMatches < candidate.maxMatches && candidate.activeUsers < candidate.maxUsers)
    .sort((a, b) => (a.activeMatches / a.maxMatches) - (b.activeMatches / b.maxMatches))[0]
  if (!server) return null
  if (existingMatch) {
    await db.update(schema.gameServers).set({ activeMatches: server.activeMatches + 1, updatedAt: new Date() }).where(eq(schema.gameServers.id, server.id))
    await db.update(schema.matches).set({ gameServerId: server.id, updatedAt: new Date() }).where(eq(schema.matches.id, matchId))
  }
  return server
}

async function issueTicket(c: Parameters<typeof requireUser>[0], input: { userId: string; teamName: string; displayName: string; matchId: string; robotData: string; robotId?: string; slotId?: string; role?: string; alliance?: string }) {
  if (!c.env.JWT_SECRET) return null
  return sign({
    sub: input.userId,
    match_id: input.matchId,
    team_name: input.teamName,
    display_name: input.displayName,
    robot_data: input.robotData,
    robot_id: input.robotId,
    slot_id: input.slotId,
    role: input.role,
    alliance: input.alliance,
    exp: Math.floor(Date.now() / 1000) + 60 * 5
  }, c.env.JWT_SECRET)
}

function lobbyUser(session: NonNullable<Awaited<ReturnType<typeof requireUser>>>): LobbyUser {
  return {
    userId: session.user.id,
    name: session.user.name || 'Simulator player',
    teamName: session.user.team || null
  }
}

async function findMatch(c: Parameters<typeof requireUser>[0], matchId: string) {
  const db = drizzle(c.env.DB, { schema })
  return db.query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
}

function lobbyFor(c: Parameters<typeof requireUser>[0], matchId: string) {
  return c.env.MATCH_LOBBY.getByName(matchId)
}

function matchSeed() {
  const values = crypto.getRandomValues(new Uint32Array(2))
  return (values[0] & 0x1fffff) * 0x1_0000_0000 + values[1]
}

async function gamePackManifest(c: Parameters<typeof requireUser>[0], gamePackId: string): Promise<PackManifest> {
  const url = new URL(c.req.url)
  url.pathname = `/${gamePackId}/manifest.json`
  const response = await c.env.PACK_ASSETS.fetch(new Request(url))
  if (!response.ok) throw new Error('The selected game pack is unavailable.')
  return response.json<PackManifest>()
}

async function gamePackVersion(c: Parameters<typeof requireUser>[0], gamePackId: string) {
  const manifest = await gamePackManifest(c, gamePackId)
  if (typeof manifest.version !== 'string' || !manifest.version) throw new Error('The selected game pack has no version.')
  return manifest.version
}

async function defaultPackRobot(c: Parameters<typeof requireUser>[0], gamePackId: string): Promise<DriverRobot> {
  const manifest = await gamePackManifest(c, gamePackId)
  const robotId = manifest.defaultRobot
  if (typeof robotId !== 'string' || !manifest.robots?.[robotId]) {
    throw new Error('The selected game pack has no playable default robot.')
  }
  return {
    id: `${PACK_ROBOT_ID_PREFIX}${robotId}`,
    data: JSON.stringify({ kind: 'pack-robot', gamePackId, robotId }),
    revision: null
  }
}

async function resolveDriverRobot(
  c: Parameters<typeof requireUser>[0],
  userId: string,
  gamePackId: string,
  requestedId: string | null | undefined
): Promise<DriverRobot> {
  const packRobot = await defaultPackRobot(c, gamePackId)
  if (!requestedId || requestedId === packRobot.id) return packRobot
  if (requestedId.startsWith(PACK_ROBOT_ID_PREFIX)) throw new Error('The selected pack robot is unavailable.')

  const robot = await drizzle(c.env.DB, { schema }).query.robots.findFirst({ where: eq(schema.robots.id, requestedId) })
  if (!robot || robot.userId !== userId) throw new Error('Choose one of your saved robots.')
  return { id: robot.id, data: robot.buildData, revision: robot.updatedAt.getTime() }
}

async function lockMatchBootstrap(c: Parameters<typeof requireUser>[0], match: typeof schema.matches.$inferSelect, serverId: string): Promise<MatchBootstrap> {
  const startsAt = Date.now() + 5_000
  const bootstrap = await lobbyFor(c, match.id).lockBootstrap({
    assignedServerId: serverId,
    gamePackId: match.gamePackId,
    gamePackVersion: await gamePackVersion(c, match.gamePackId),
    matchSeed: matchSeed(),
    startsAt,
    durationSeconds: 150,
    maxPlayers: match.maxPlayers,
    scenarioId: 'default',
    visibility: 'private'
  })
  await drizzle(c.env.DB, { schema }).update(schema.matches).set({
    status: 'IN_PROGRESS', matchSeed: bootstrap.matchSeed, packVersion: bootstrap.gamePackVersion,
    startsAt: new Date(bootstrap.startsAt), updatedAt: new Date()
  }).where(eq(schema.matches.id, match.id))
  await drizzle(c.env.DB, { schema }).insert(schema.gameServerCommands).values({
    id: crypto.randomUUID(), serverId, type: 'bootstrap_match', payload: JSON.stringify({ matchId: match.id }),
    status: 'pending', createdAt: new Date(), deliveredAt: null, completedAt: null, error: null
  })
  return bootstrap
}

function lobbyError(c: Parameters<typeof requireUser>[0], error: unknown) {
  const message = error instanceof Error ? error.message : 'Unable to update lobby.'
  const code = /slot|robot/i.test(message) ? 'LOBBY_SLOT_UNAVAILABLE' : 'LOBBY_INVALID_STATE'
  return jsonError(c, 409, code, message)
}

app.post('/', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')

  const body = await parseJson(c, matchSchema)
  if (isResponse(body)) return body

  const db = drizzle(c.env.DB, { schema })
  const match = {
    id: crypto.randomUUID(),
    hostId: session.user.id,
    gamePackId: body.gamePackId,
    status: 'LOBBY',
    maxPlayers: 8,
    updatedAt: new Date(),
    cancelledAt: null,
    cancelReason: null,
    createdAt: new Date()
  }
  await db.insert(schema.matches).values(match)
  await lobbyFor(c, match.id).initialize(match.id, match.hostId)

  const webOrigin = (c.env.WEB_ORIGIN || new URL(c.req.url).origin).replace(/\/$/, '')
  return jsonSuccess(c, {
    match_id: match.id,
    invite_link: `${webOrigin}/match/${match.id}/lobby`
  }, 201)
})

app.get('/:id/lobby', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')
  const match = await findMatch(c, c.req.param('id'))
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  try {
    return jsonSuccess(c, { lobby: await lobbyFor(c, match.id).getState() })
  } catch (error) {
    return lobbyError(c, error)
  }
})

app.get('/:id/lobby/ws', async (c) => {
  const session = await requireUser(c)
  if (!session) return c.text('Sign in is required.', 401)
  const match = await findMatch(c, c.req.param('id'))
  if (!match) return c.text('Match not found.', 404)
  if (c.req.header('Upgrade') !== 'websocket') return c.text('Expected WebSocket upgrade.', 426)
  const headers = new Headers(c.req.raw.headers)
  headers.set('X-Lobby-User-Id', session.user.id)
  return lobbyFor(c, match.id).fetch(new Request('https://match-lobby.internal/ws', {
    method: 'GET',
    headers
  }))
})

app.post('/:id/lobby/slot', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')
  const body = await parseJson(c, lobbySlotSchema)
  if (isResponse(body)) return body
  const match = await findMatch(c, c.req.param('id'))
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  if (body.slotId.endsWith('human') && body.robotId) {
    return jsonError(c, 400, 'VALIDATION_ERROR', 'Human-player stations cannot use a robot.')
  }
  try {
    const robotId = body.slotId.includes('driver')
      ? (await resolveDriverRobot(c, session.user.id, match.gamePackId, body.robotId)).id
      : null
    const lobby = await lobbyFor(c, match.id).claimSlot(lobbyUser(session), body.slotId as LobbySlotId, robotId)
    return jsonSuccess(c, { lobby })
  } catch (error) {
    return lobbyError(c, error)
  }
})

app.post('/:id/lobby/leave', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')
  const match = await findMatch(c, c.req.param('id'))
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  try {
    return jsonSuccess(c, { lobby: await lobbyFor(c, match.id).leave(session.user.id) })
  } catch (error) {
    return lobbyError(c, error)
  }
})

app.post('/:id/lobby/ready', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')
  const body = await parseJson(c, lobbyReadySchema)
  if (isResponse(body)) return body
  const match = await findMatch(c, c.req.param('id'))
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  try {
    return jsonSuccess(c, { lobby: await lobbyFor(c, match.id).setReady(session.user.id, body.ready) })
  } catch (error) {
    return lobbyError(c, error)
  }
})

app.post('/:id/lobby/start', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')
  const match = await findMatch(c, c.req.param('id'))
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  const lobby = lobbyFor(c, match.id)
  try {
    await lobby.beginStart(session.user.id)
    const server = await chooseGameServer(c, match.id)
    if (!server) {
      await lobby.reopen('No healthy game server is available right now.')
      return jsonError(c, 503, 'GAME_SERVER_UNAVAILABLE', 'No healthy game server is available right now.')
    }
    const bootstrap = await lockMatchBootstrap(c, match, server.id)
    return jsonSuccess(c, { lobby: await lobby.getState(), game_server_id: server.id, starts_at: bootstrap.startsAt })
  } catch (error) {
    return lobbyError(c, error)
  }
})

app.post('/:id/lobby/admin-start', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const matchId = c.req.param('id')
  const db = drizzle(c.env.DB, { schema })
  const match = await db.query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  if (match.status === 'CANCELLED' || match.status === 'FINISHED') {
    return jsonError(c, 409, 'LOBBY_INVALID_STATE', 'This match cannot be entered immediately.')
  }

  const server = match.gameServerId
    ? await db.query.gameServers.findFirst({ where: eq(schema.gameServers.id, match.gameServerId) })
    : await chooseGameServer(c, matchId)
  if (!server || server.disabledAt || !server.lastHeartbeatAt || Date.now() - server.lastHeartbeatAt.getTime() >= 30_000) {
    return jsonError(c, 503, 'GAME_SERVER_UNAVAILABLE', 'No healthy game server is available right now.')
  }

  try {
    const lobby = lobbyFor(c, matchId)
    const robot = await defaultPackRobot(c, match.gamePackId)
    await lobby.assignAdminDriver(lobbyUser(session), robot.id)
    await lobby.forceStart()
    const bootstrap = await lockMatchBootstrap(c, match, server.id)
    return jsonSuccess(c, { lobby: await lobby.getState(), game_server_id: server.id, starts_at: bootstrap.startsAt })
  } catch (error) {
    return lobbyError(c, error)
  }
})

app.post('/:id/ticket', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')
  if (!c.env.JWT_SECRET) {
    return jsonError(c, 503, 'INTERNAL_ERROR', 'Match tickets are not configured.')
  }

  const matchId = c.req.param('id')
  const db = drizzle(c.env.DB, { schema })
  const match = await db.query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  if (match.status !== 'IN_PROGRESS') return jsonError(c, 409, 'LOBBY_INVALID_STATE', 'Wait for the host to start the match.')

  let station
  try {
    station = (await lobbyFor(c, match.id).getState()).slots.find((slot) => slot.occupant?.userId === session.user.id)
  } catch (error) {
    return lobbyError(c, error)
  }
  const adminBypass = !station?.occupant && Boolean(await requireAdmin(c))
  if (!station?.occupant && !adminBypass) return jsonError(c, 403, 'LOBBY_INVALID_STATE', 'Claim a lobby station before joining the match.')

  let driverRobot: DriverRobot | null = null
  if (station?.role === 'driver') {
    try {
      driverRobot = await resolveDriverRobot(c, session.user.id, match.gamePackId, station.occupant?.robotId)
    } catch (error) {
      return jsonError(c, 400, 'ROBOT_NOT_FOUND', error instanceof Error ? error.message : 'The selected robot is unavailable.')
    }
  }

  const server = match.gameServerId
    ? await db.query.gameServers.findFirst({ where: eq(schema.gameServers.id, match.gameServerId) })
    : await chooseGameServer(c, matchId)
  if (!server || server.disabledAt || !server.lastHeartbeatAt || Date.now() - server.lastHeartbeatAt.getTime() >= 30_000) {
    return jsonError(c, 503, 'GAME_SERVER_UNAVAILABLE', 'No healthy game server is available right now.')
  }

  const ticket = await issueTicket(c, {
    userId: session.user.id,
    teamName: session.user.team || 'Administrator',
    displayName: session.user.name || 'Simulator player',
    matchId,
    robotData: driverRobot?.data || JSON.stringify({ kind: adminBypass ? 'admin-test-cube' : 'human-player' }),
    robotId: driverRobot?.id,
    slotId: station?.id || 'red-driver-1',
    role: station?.role || 'driver',
    alliance: station?.alliance || 'red'
  })
  if (!ticket) return jsonError(c, 503, 'INTERNAL_ERROR', 'Match tickets are not configured.')

  return jsonSuccess(c, {
    ticket,
    ws_url: gameServerUrl(server.origin, matchId, ticket)
  })
})

/**
 * Open-arena join: issues a ticket for the always-on "arena" match without
 * requiring a lobby slot. The arena match row is upserted in IN_PROGRESS
 * state so that every authenticated user can join at any time, up to the
 * server's active-user cap. Alliance is assigned deterministically by hashing
 * the user ID so the same user always lands on the same side while keeping
 * the teams roughly balanced.
 */
app.post('/arena/open-join', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')
  if (!c.env.JWT_SECRET) return jsonError(c, 503, 'INTERNAL_ERROR', 'Match tickets are not configured.')

  const ARENA_ID = 'arena'
  const ARENA_MAX_PLAYERS = 20
  const db = drizzle(c.env.DB, { schema })

  // Ensure the arena match row exists and is IN_PROGRESS.
  // We do a read-then-write to avoid D1's limited conflict resolution with
  // nullable columns (onConflictDoUpdate chokes on null params in D1).
  // hostId uses the first joiner's real user ID to satisfy the FK constraint.
  const existing = await db.query.matches.findFirst({ where: eq(schema.matches.id, ARENA_ID) })
  if (!existing) {
    await db.insert(schema.matches).values({
      id: ARENA_ID,
      hostId: session.user.id,
      gamePackId: 'fgc-2026',
      status: 'IN_PROGRESS',
      maxPlayers: ARENA_MAX_PLAYERS,
      updatedAt: new Date(),
      createdAt: new Date()
    })
  } else if (existing.status !== 'IN_PROGRESS') {
    await db.update(schema.matches)
      .set({ status: 'IN_PROGRESS', updatedAt: new Date() })
      .where(eq(schema.matches.id, ARENA_ID))
  }


  const server = await chooseGameServer(c, ARENA_ID)
  if (!server || server.disabledAt || !server.lastHeartbeatAt || Date.now() - server.lastHeartbeatAt.getTime() >= 30_000) {
    return jsonError(c, 503, 'GAME_SERVER_UNAVAILABLE', 'No healthy game server is available right now.')
  }

  // Deterministic but balanced alliance assignment via user-id hash.
  const hashBytes = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(session.user.id))
  const firstByte = new Uint8Array(hashBytes)[0] ?? 0
  const alliance = firstByte % 2 === 0 ? 'red' : 'blue'

  let driverRobot: DriverRobot
  try {
    driverRobot = await defaultPackRobot(c, 'fgc-2026')
  } catch (error) {
    return jsonError(c, 503, 'INTERNAL_ERROR', error instanceof Error ? error.message : 'The default robot is unavailable.')
  }

  const ticket = await issueTicket(c, {
    userId: session.user.id,
    teamName: session.user.team || alliance,
    displayName: session.user.name || 'Arena player',
    matchId: ARENA_ID,
    robotData: driverRobot.data,
    robotId: driverRobot.id,
    slotId: `${alliance}-driver-1`,
    role: 'driver',
    alliance
  })
  if (!ticket) return jsonError(c, 503, 'INTERNAL_ERROR', 'Match tickets are not configured.')

  return jsonSuccess(c, {
    ticket,
    ws_url: gameServerUrl(server.origin, ARENA_ID, ticket),
    alliance
  })
})

export default app
