import { Hono } from 'hono'
import { drizzle } from 'drizzle-orm/d1'
import { desc, eq } from 'drizzle-orm'
import * as schema from '../db/schema'
import { isResponse, parseJson, robotSchema } from '../lib/validation'
import { requireUser } from '../middleware'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()

function robotDto(robot: typeof schema.robots.$inferSelect) {
  return {
    id: robot.id,
    name: robot.name,
    buildData: JSON.parse(robot.buildData) as Record<string, unknown>,
    createdAt: robot.createdAt,
    updatedAt: robot.updatedAt
  }
}

app.get('/', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')

  const db = drizzle(c.env.DB, { schema })
  const robots = await db.select().from(schema.robots)
    .where(eq(schema.robots.userId, session.user.id))
    .orderBy(desc(schema.robots.updatedAt))

  return jsonSuccess(c, { robots: robots.map(robotDto) })
})

app.post('/', async (c) => {
  const session = await requireUser(c)
  if (!session) return jsonError(c, 401, 'AUTH_FAILED', 'Sign in is required.')

  const body = await parseJson(c, robotSchema)
  if (isResponse(body)) return body

  const db = drizzle(c.env.DB, { schema })
  const now = new Date()
  const robot = {
    id: crypto.randomUUID(),
    userId: session.user.id,
    name: body.name,
    buildData: JSON.stringify(body.buildData),
    createdAt: now,
    updatedAt: now
  }
  await db.insert(schema.robots).values(robot)

  return jsonSuccess(c, { robot: robotDto(robot) }, 201)
})

export default app
