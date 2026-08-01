import { Hono } from 'hono'
import { cors } from 'hono/cors'
import { jsonSuccess, jsonError } from './responses'
import { createAuth, type AuthEnvironment } from './auth'
import { drizzle } from 'drizzle-orm/d1'
import * as schema from './db/schema'
import { eq } from 'drizzle-orm'
import { v4 as uuidv4 } from 'uuid'
import type { Context } from 'hono'

export type Bindings = AuthEnvironment & {
  JWT_SECRET: string
  N8N_WEBHOOK_URL?: string
}

const app = new Hono<{ Bindings: Bindings }>()

type AuthenticatedUser = {
  id: string
  role: 'user' | 'admin'
  team?: string | null
}

async function requireUser(c: Context<{ Bindings: Bindings }>) {
  const auth = createAuth(c.env, c.req.url)
  const session = await auth.api.getSession({ headers: c.req.raw.headers })
  if (!session) return null
  return session as typeof session & { user: AuthenticatedUser }
}

async function requireAdmin(c: Context<{ Bindings: Bindings }>) {
  const session = await requireUser(c)
  return session?.user.role === 'admin' ? session : null
}

/** Allow the configured web app to send its session cookie to a separate API origin. */
app.use('*', async (c, next) => {
  const webOrigin = (c.env.WEB_ORIGIN || 'http://localhost:5173').replace(/\/$/, '')
  const apiOrigin = (c.env.API_ORIGIN || new URL(c.req.url).origin).replace(/\/$/, '')
  const requestOrigin = c.req.header('Origin')

  return cors({
    origin: requestOrigin === webOrigin || requestOrigin === apiOrigin ? requestOrigin : '',
    credentials: true,
    allowHeaders: ['Content-Type', 'Authorization'],
    allowMethods: ['GET', 'POST', 'OPTIONS']
  })(c, next)
})

// Intercept signup to validate invitation code
app.use('/api/auth/sign-up/email', async (c, next) => {
	let invitationCode: string | undefined
  if (c.req.method === 'POST') {
    try {
      // Clone the request to read the body without consuming the original body stream for better-auth
      const bodyStr = await c.req.text()
      const body = JSON.parse(bodyStr)
			invitationCode = body.invitationCode

      if (!invitationCode) {
        return jsonError(c, 400, 'VALIDATION_ERROR', 'Invitation code is required.')
      }

      const db = drizzle(c.env.DB, { schema })
      const invite = await db.query.invitations.findFirst({
        where: eq(schema.invitations.code, invitationCode)
      })

      if (!invite || invite.used) {
        return jsonError(c, 400, 'VALIDATION_ERROR', 'Invalid or already used invitation code.')
      }

      // Re-construct the request so better-auth can read the body
      c.req.raw = new Request(c.req.raw.url, {
        method: c.req.raw.method,
        headers: c.req.raw.headers,
        body: bodyStr
      })
    } catch (e) {
      console.error(e)
      return jsonError(c, 400, 'VALIDATION_ERROR', 'Invalid request body')
    }
  }
  await next()

  // Consume a code only when Better Auth actually created the account. This
  // prevents a transient password/database error from permanently burning it.
  if (c.res.ok && invitationCode) {
    const db = drizzle(c.env.DB, { schema })
    await db.update(schema.invitations)
      .set({ used: true })
      .where(eq(schema.invitations.code, invitationCode))
  }
})

app.post('/api/auth/request-invite', async (c) => {
  const body = await c.req.json().catch(() => null)
  if (!body || !body.email || !body.name || !body.team) {
    return jsonError(c, 400, 'VALIDATION_ERROR', 'Missing required fields.')
  }

  // Optional: Send to n8n Webhook
  const n8nWebhookUrl = c.env.N8N_WEBHOOK_URL || 'http://localhost:5678/webhook/invite-request'
  try {
    await fetch(n8nWebhookUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        email: body.email,
        name: body.name,
        team: body.team,
        message: body.message || ''
      })
    })
  } catch (err) {
    console.error('Failed to send to n8n:', err)
    // We can still return success to the user so they don't know it failed
  }

  return jsonSuccess(c, { message: 'Invite request received!' })
})

// Extract better-auth handler
app.on(['POST', 'GET'], '/api/auth/**', (c) => {
  const auth = createAuth(c.env, c.req.url)
  return auth.handler(c.req.raw)
})

const userApi = new Hono<{ Bindings: Bindings }>()
userApi.get('/me', async (c) => {
  const session = await requireUser(c)
  if (!session) {
    return jsonError(c, 401, 'AUTH_FAILED', 'Unauthorized')
  }
  return jsonSuccess(c, { user: session.user })
})

userApi.post('/profile', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Unauthorized')

  const body = await c.req.json()
  const db = drizzle(c.env.DB, { schema })
  
  if (body.name) {
    await db.update(schema.user).set({ name: body.name }).where(eq(schema.user.id, session.user.id))
  }
  
  return jsonSuccess(c, { message: 'Profile updated' })
})
app.route('/api/user', userApi)


const robotApi = new Hono<{ Bindings: Bindings }>()
robotApi.get('/', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Unauthorized')

  const db = drizzle(c.env.DB, { schema })
  const userRobots = await db.select().from(schema.robots).where(eq(schema.robots.userId, session.user.id))
  
  return jsonSuccess(c, { robots: userRobots })
})

robotApi.post('/', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Unauthorized')

  const body = await c.req.json()
  if (!body.name || !body.buildData) {
    return jsonError(c, 400, 'VALIDATION_ERROR', 'Missing name or buildData')
  }

  const db = drizzle(c.env.DB, { schema })
  const robotId = uuidv4()
  
  await db.insert(schema.robots).values({
    id: robotId,
    userId: session.user.id,
    name: body.name,
    buildData: JSON.stringify(body.buildData),
    createdAt: new Date(),
    updatedAt: new Date(),
  })
  
  return jsonSuccess(c, { robot_id: robotId })
})
app.route('/api/robot', robotApi)


const matchesApi = new Hono<{ Bindings: Bindings }>()
matchesApi.post('/', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Unauthorized')

  const body = await c.req.json()
  if (!body.gamePackId) {
    return jsonError(c, 400, 'VALIDATION_ERROR', 'Missing gamePackId')
  }

  const db = drizzle(c.env.DB, { schema })
  const matchId = uuidv4()
  
  await db.insert(schema.matches).values({
    id: matchId,
    hostId: session.user.id,
    gamePackId: body.gamePackId,
    status: 'LOBBY',
    maxPlayers: 6,
    createdAt: new Date(),
  })
  
  // Generating an invite link assuming the frontend lives at /join
  const inviteLink = `${new URL(c.req.url).origin}/join/${matchId}`

  return jsonSuccess(c, { match_id: matchId, invite_link: inviteLink })
})

import { sign } from 'hono/jwt'

matchesApi.post('/:id/ticket', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Unauthorized')

  const matchId = c.req.param('id')
  
  // Here we would fetch their active robot from DB
  const db = drizzle(c.env.DB, { schema })
  const userRobot = await db.query.robots.findFirst({
    where: eq(schema.robots.userId, session.user.id),
    orderBy: (robots, { desc }) => [desc(robots.updatedAt)]
  })

  if (!userRobot) {
    return jsonError(c, 400, 'VALIDATION_ERROR', 'You must build a robot first.')
  }

  // Generate a short-lived JWT ticket for the Rust server to verify
  const secret = c.env.JWT_SECRET || 'secret' // Make sure env.JWT_SECRET exists
  const payload = {
    sub: session.user.id,
    match_id: matchId,
    team_name: (session.user as any).team || 'Unknown',
    robot_data: userRobot.buildData,
    exp: Math.floor(Date.now() / 1000) + 60 * 5, // 5 minutes expiration
  }

  const ticket = await sign(payload, secret)

  return jsonSuccess(c, { ticket, ws_url: `ws://localhost:3000/ws/match/${matchId}?ticket=${ticket}` })
})

app.route('/api/matches', matchesApi)


const gamePacksApi = new Hono<{ Bindings: Bindings }>()
gamePacksApi.get('/', (c) => {
  // In a real database, this would read from a game_packs table or cloud storage.
  // We mock the FGC 2026 pack for now since it's shipped with the client/server.
  return jsonSuccess(c, {
    packs: [
      { id: 'fgc-2026', name: 'Igniting Innovation', version: '1.0.0' }
    ]
  })
})
app.route('/api/game-packs', gamePacksApi)

const adminApi = new Hono<{ Bindings: Bindings }>()
adminApi.get('/users', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required')

  const db = drizzle(c.env.DB, { schema })
  const users = await db
    .select({ id: schema.user.id, name: schema.user.name, email: schema.user.email, team: schema.user.team, role: schema.user.role })
    .from(schema.user)

  return jsonSuccess(c, { users })
})
app.route('/api/admin', adminApi)


app.get('/', (c) => {
  return c.text('FGC 2026 Simulator API (Hono)')
})

app.onError((err, c) => {
  console.error('Unhandled Exception:', err)
  return jsonError(c, 500, 'INTERNAL_ERROR', err.message || 'An unexpected error occurred')
})

export default app
