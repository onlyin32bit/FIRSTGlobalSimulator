import { Hono } from 'hono'
import { drizzle } from 'drizzle-orm/d1'
import { eq } from 'drizzle-orm'
import * as schema from '../db/schema'
import { profileSchema, isResponse, parseJson } from '../lib/validation'
import { requireUser } from '../middleware'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()

function userDto(user: typeof schema.user.$inferSelect) {
  return {
    id: user.id,
    name: user.name,
    email: user.email,
    team: user.team,
    role: user.role === 'admin' ? 'admin' : 'user'
  }
}

app.get('/me', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')

  const db = drizzle(c.env.DB, { schema })
  const record = await db.query.user.findFirst({ where: eq(schema.user.id, session.user.id) })
  if (!record) return jsonError(c, 401, 'AUTH_FAILED', 'Your account is unavailable.')

  return jsonSuccess(c, { user: userDto(record) })
})

app.patch('/profile', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')

  const body = await parseJson(c, profileSchema)
  if (isResponse(body)) return body

  const db = drizzle(c.env.DB, { schema })
  await db.update(schema.user)
    .set({ name: body.name, updatedAt: new Date() })
    .where(eq(schema.user.id, session.user.id))

  return jsonSuccess(c, { message: 'Profile updated.' })
})

export default app
