import { Hono } from 'hono'
import { eq } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import { sign } from 'hono/jwt'
import * as schema from '../db/schema'
import { isResponse, matchSchema, parseJson } from '../lib/validation'
import { requireUser } from '../middleware'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()

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

  const ticket = await sign({
    sub: session.user.id,
    match_id: matchId,
    team_name: session.user.team || 'Unknown',
    robot_data: userRobot.buildData,
    exp: Math.floor(Date.now() / 1000) + 60 * 5
  }, c.env.JWT_SECRET)

  const gameServerOrigin = (c.env.GAME_SERVER_ORIGIN || 'ws://localhost:3000').replace(/\/$/, '')
  return jsonSuccess(c, {
    ticket,
    ws_url: `${gameServerOrigin}/ws/match/${encodeURIComponent(matchId)}?ticket=${encodeURIComponent(ticket)}`
  })
})

export default app
