import { Hono } from 'hono'
import { and, eq, isNull, sql } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import { createAuth } from '../auth'
import * as schema from '../db/schema'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'
import { inviteCodeSchema, inviteRequestSchema, isResponse, parseJson } from '../lib/validation'

const app = new Hono<{ Bindings: Bindings }>()

app.use('/sign-up/email', async (c, next) => {
  if (c.req.method !== 'POST') return next()

  const rawBody = await c.req.text()
  const body = await Promise.resolve().then(() => JSON.parse(rawBody)).catch(() => null)
  const invitationCode = inviteCodeSchema.safeParse(body?.invitationCode)

  if (!invitationCode.success) {
    return jsonError(c, 400, 'VALIDATION_ERROR', 'A valid invitation code is required.')
  }

  const db = drizzle(c.env.DB, { schema })
  const invite = await db.query.invitations.findFirst({
    where: eq(schema.invitations.code, invitationCode.data)
  })
  const now = new Date()
  const nowTimestamp = Math.floor(now.getTime() / 1000)

  if (
    !invite ||
    invite.used ||
    invite.revokedAt ||
    (invite.expiresAt && invite.expiresAt <= now)
  ) {
    return jsonError(c, 400, 'VALIDATION_ERROR', 'This invitation code is invalid or unavailable.')
  }

  c.req.raw = new Request(c.req.raw.url, {
    method: c.req.raw.method,
    headers: c.req.raw.headers,
    body: rawBody
  })

  await next()

  if (!c.res.ok) return

  const response = c.res.clone()
  const result = await response.json().catch(() => null) as { user?: { id?: string } } | null
  const userId = result?.user?.id
  if (!userId) return

  const consumption = await db.update(schema.invitations)
    .set({
      used: true,
      redeemedAt: now,
      redeemedByUserId: userId
    })
    .where(and(
      eq(schema.invitations.code, invitationCode.data),
      eq(schema.invitations.used, false),
      isNull(schema.invitations.revokedAt),
      sql`(${schema.invitations.expiresAt} IS NULL OR ${schema.invitations.expiresAt} > ${nowTimestamp})`
    ))
    .returning({ code: schema.invitations.code })

  if (consumption.length === 0) {
    console.error('Invitation code was unavailable after account creation.')
  }
})

app.post('/request-invite', async (c) => {
  const body = await parseJson(c, inviteRequestSchema)
  if (isResponse(body)) return body

  if (!c.env.N8N_WEBHOOK_URL) {
    return jsonError(c, 503, 'INTERNAL_ERROR', 'Invitation requests are not configured yet.')
  }

  try {
    const response = await fetch(c.env.N8N_WEBHOOK_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body)
    })
    if (!response.ok) throw new Error(`Webhook returned ${response.status}`)
  } catch (error) {
    console.error('Invite request delivery failed:', error)
    return jsonError(c, 502, 'INTERNAL_ERROR', 'Unable to submit the invitation request. Please try again later.')
  }

  return jsonSuccess(c, { message: 'Invitation request received.' }, 202)
})

app.all('/*', (c) => {
  const auth = createAuth(c.env, c.req.url)
  return auth.handler(c.req.raw)
})

export default app
