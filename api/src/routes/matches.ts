import { Hono } from 'hono'
import { desc, eq } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import { sign } from 'hono/jwt'
import * as schema from '../db/schema'
import { isResponse, matchSchema, parseJson } from '../lib/validation'
import { requireUser } from '../middleware'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

export const TEST_MATCH_ID = 'test-match'

const app = new Hono<{ Bindings: Bindings }>()

function gameServerUrl(origin: string | undefined, matchId: string, ticket: string) {
  const gameServerOrigin = (origin || 'ws://localhost:3000').replace(/^http:/, 'ws:').replace(/^https:/, 'wss:').replace(/\/$/, '')
  return `${gameServerOrigin}/ws/match/${encodeURIComponent(matchId)}?ticket=${encodeURIComponent(ticket)}`
}

async function chooseGameServer(c: Parameters<typeof requireUser>[0], matchId: string) {
  const db = drizzle(c.env.DB, { schema })
  const servers = await db.select().from(schema.gameServers).orderBy(desc(schema.gameServers.activeMatches))
  const now = Date.now()
  const server = servers
    .filter((candidate) => !candidate.disabledAt && candidate.lastHeartbeatAt && now - candidate.lastHeartbeatAt.getTime() < 30_000)
    .filter((candidate) => candidate.activeMatches < candidate.maxMatches && candidate.activeUsers < candidate.maxUsers)
    .sort((a, b) => (a.activeMatches / a.maxMatches) - (b.activeMatches / b.maxMatches))[0]
  if (!server) return null
  await db.update(schema.gameServers).set({ activeMatches: server.activeMatches + 1, updatedAt: new Date() }).where(eq(schema.gameServers.id, server.id))
  await db.update(schema.matches).set({ gameServerId: server.id, updatedAt: new Date() }).where(eq(schema.matches.id, matchId))
  return server
}

async function issueTicket(c: Parameters<typeof requireUser>[0], input: { userId: string; teamName: string; displayName: string; matchId: string; robotData: string }) {
  if (!c.env.JWT_SECRET) return null
  return sign({
    sub: input.userId,
    match_id: input.matchId,
    team_name: input.teamName,
    display_name: input.displayName,
    robot_data: input.robotData,
    exp: Math.floor(Date.now() / 1000) + 60 * 5
  }, c.env.JWT_SECRET)
}

app.post(`/${TEST_MATCH_ID}/ticket`, async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')

  const ticket = await issueTicket(c, {
    userId: session.user.id,
    teamName: session.user.team || 'Simulator player',
    displayName: session.user.name || 'Simulator player',
    matchId: TEST_MATCH_ID,
    robotData: JSON.stringify({ kind: 'test-cube' })
  })
  if (!ticket) return jsonError(c, 503, 'INTERNAL_ERROR', 'Match tickets are not configured.')

  const server = await chooseGameServer(c, TEST_MATCH_ID)

  return jsonSuccess(c, {
    match_id: TEST_MATCH_ID,
    ticket,
    ws_url: gameServerUrl(server?.origin || c.env.GAME_SERVER_ORIGIN, TEST_MATCH_ID, ticket)
  })
})

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
    maxPlayers: 6,
    updatedAt: new Date(),
    cancelledAt: null,
    cancelReason: null,
    createdAt: new Date()
  }
  await db.insert(schema.matches).values(match)

  const webOrigin = (c.env.WEB_ORIGIN || new URL(c.req.url).origin).replace(/\/$/, '')
  return jsonSuccess(c, {
    match_id: match.id,
    invite_link: `${webOrigin}/join/${match.id}`
  }, 201)
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

  const userRobot = await db.query.robots.findFirst({
    where: eq(schema.robots.userId, session.user.id),
    orderBy: (robots, { desc }) => [desc(robots.updatedAt)]
  })
  if (!userRobot) {
    return jsonError(c, 400, 'VALIDATION_ERROR', 'Build a robot before joining a match.')
  }

  const server = match.gameServerId
    ? await db.query.gameServers.findFirst({ where: eq(schema.gameServers.id, match.gameServerId) })
    : await chooseGameServer(c, matchId)
  if (!server || server.disabledAt || !server.lastHeartbeatAt || Date.now() - server.lastHeartbeatAt.getTime() >= 30_000) {
    return jsonError(c, 503, 'GAME_SERVER_UNAVAILABLE', 'No healthy game server is available right now.')
  }

  const ticket = await issueTicket(c, {
    userId: session.user.id,
    teamName: session.user.team || 'Unknown',
    displayName: session.user.name || 'Simulator player',
    matchId,
    robotData: userRobot.buildData
  })
  if (!ticket) return jsonError(c, 503, 'INTERNAL_ERROR', 'Match tickets are not configured.')

  return jsonSuccess(c, {
    ticket,
    ws_url: gameServerUrl(server.origin, matchId, ticket)
  })
})

export default app
