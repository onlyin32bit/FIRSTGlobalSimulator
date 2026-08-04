import type { Context } from 'hono'
import { eq } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import { createAuth } from './auth'
import * as schema from './db/schema'
import type { Bindings, AuthenticatedUser } from './types'

export async function requireUser(c: Context<{ Bindings: Bindings }>) {
  const auth = createAuth(c.env, c.req.url)
  const session = await auth.api.getSession({ headers: c.req.raw.headers })
  if (!session) return null

  const db = drizzle(c.env.DB, { schema })
  const currentUser = await db.query.user.findFirst({ where: eq(schema.user.id, session.user.id) })
  if (!currentUser || currentUser.disabledAt) return null

  return session as typeof session & { user: AuthenticatedUser }
}

export async function requireAdmin(c: Context<{ Bindings: Bindings }>) {
  const session = await requireUser(c)
  if (!session) return null

  const db = drizzle(c.env.DB, { schema })
  const currentUser = await db.query.user.findFirst({
    where: eq(schema.user.id, session.user.id)
  })
  if (currentUser?.role !== 'admin') return null

  return session as typeof session & { user: AuthenticatedUser }
}
