import { Hono } from 'hono'
import { and, asc, count, desc, eq, isNull, like, or, sql } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import * as schema from '../db/schema'
import { writeAdminAudit } from '../lib/audit'
import {
  adminListQuerySchema,
  createInvitationSchema,
  isResponse,
  parseJson,
  userRoleSchema
} from '../lib/validation'
import { requireAdmin } from '../middleware'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'

const app = new Hono<{ Bindings: Bindings }>()

function invitationDto(invitation: typeof schema.invitations.$inferSelect) {
  return {
    code: invitation.code,
    createdAt: invitation.createdAt,
    expiresAt: invitation.expiresAt,
    revokedAt: invitation.revokedAt,
    redeemedAt: invitation.redeemedAt,
    redeemedByUserId: invitation.redeemedByUserId,
    status: invitation.revokedAt
      ? 'revoked'
      : invitation.used
        ? 'redeemed'
        : invitation.expiresAt && invitation.expiresAt <= new Date()
          ? 'expired'
          : 'active'
  }
}

function newInvitationCode() {
  return crypto.randomUUID().replaceAll('-', '').slice(0, 6).toUpperCase()
}

function pagination(c: Parameters<typeof adminListQuerySchema.parse>[0]) {
  return adminListQuerySchema.parse(c)
}

function pageResult<T>(items: T[], page: number, pageSize: number, total: number) {
  return { items, page, pageSize, total, pageCount: Math.max(1, Math.ceil(total / pageSize)) }
}

app.get('/overview', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')

  const db = drizzle(c.env.DB, { schema })
  const [usersResult, adminsResult, robotsResult, matchesResult, activeInvitesResult, audits, recentMatches] = await Promise.all([
    db.select({ value: count() }).from(schema.user),
    db.select({ value: count() }).from(schema.user).where(eq(schema.user.role, 'admin')),
    db.select({ value: count() }).from(schema.robots),
    db.select({ value: count() }).from(schema.matches),
    db.select({ value: count() }).from(schema.invitations).where(and(eq(schema.invitations.used, false), isNull(schema.invitations.revokedAt))),
    db.select({
      id: schema.adminAuditLog.id,
      action: schema.adminAuditLog.action,
      targetType: schema.adminAuditLog.targetType,
      targetId: schema.adminAuditLog.targetId,
      metadata: schema.adminAuditLog.metadata,
      createdAt: schema.adminAuditLog.createdAt,
      actorName: schema.user.name,
      actorEmail: schema.user.email
    }).from(schema.adminAuditLog).innerJoin(schema.user, eq(schema.adminAuditLog.actorUserId, schema.user.id)).orderBy(desc(schema.adminAuditLog.createdAt)).limit(8),
    db.select({
      id: schema.matches.id,
      status: schema.matches.status,
      gamePackId: schema.matches.gamePackId,
      maxPlayers: schema.matches.maxPlayers,
      createdAt: schema.matches.createdAt,
      hostName: schema.user.name,
      hostEmail: schema.user.email
    }).from(schema.matches).innerJoin(schema.user, eq(schema.matches.hostId, schema.user.id)).orderBy(desc(schema.matches.createdAt)).limit(6)
  ])

  return jsonSuccess(c, {
    metrics: {
      users: usersResult[0]?.value ?? 0,
      admins: adminsResult[0]?.value ?? 0,
      robots: robotsResult[0]?.value ?? 0,
      matches: matchesResult[0]?.value ?? 0,
      activeInvitations: activeInvitesResult[0]?.value ?? 0
    },
    recentActivity: audits.map((entry) => ({ ...entry, metadata: entry.metadata ? JSON.parse(entry.metadata) : null })),
    recentMatches
  })
})

app.get('/users', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')

  const query = pagination(c.req.query())
  const db = drizzle(c.env.DB, { schema })
  const filter = query.search
    ? or(like(schema.user.name, `%${query.search}%`), like(schema.user.email, `%${query.search}%`), like(schema.user.team, `%${query.search}%`))
    : undefined
  const [totalResult, users] = await Promise.all([
    db.select({ value: count() }).from(schema.user).where(filter),
    db.select({
      id: schema.user.id,
      name: schema.user.name,
      email: schema.user.email,
      team: schema.user.team,
      role: schema.user.role,
      createdAt: schema.user.createdAt,
      updatedAt: schema.user.updatedAt,
      sessionCount: sql<number>`(SELECT COUNT(*) FROM session WHERE session.userId = ${schema.user.id})`,
      robotCount: sql<number>`(SELECT COUNT(*) FROM robots WHERE robots.userId = ${schema.user.id})`,
      matchHostCount: sql<number>`(SELECT COUNT(*) FROM matches WHERE matches.hostId = ${schema.user.id})`
    }).from(schema.user).where(filter).orderBy(asc(schema.user.name)).limit(query.pageSize).offset((query.page - 1) * query.pageSize)
  ])

  return jsonSuccess(c, { users: pageResult(users, query.page, query.pageSize, totalResult[0]?.value ?? 0) })
})

app.patch('/users/:id/role', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, userRoleSchema)
  if (isResponse(body)) return body

  const targetId = c.req.param('id')
  if (targetId === session.user.id && body.role !== 'admin') {
    return jsonError(c, 409, 'VALIDATION_ERROR', 'You cannot remove your own administrator role.')
  }

  const db = drizzle(c.env.DB, { schema })
  const target = await db.query.user.findFirst({ where: eq(schema.user.id, targetId) })
  if (!target) return jsonError(c, 404, 'VALIDATION_ERROR', 'User not found.')
  if (target.role === body.role) return jsonSuccess(c, { user: target })

  if (target.role === 'admin' && body.role === 'user') {
    const admins = await db.select({ value: count() }).from(schema.user).where(eq(schema.user.role, 'admin'))
    if ((admins[0]?.value ?? 0) <= 1) {
      return jsonError(c, 409, 'VALIDATION_ERROR', 'At least one administrator must remain.')
    }
  }

  const updated = await db.update(schema.user).set({ role: body.role, updatedAt: new Date() }).where(eq(schema.user.id, targetId)).returning()
  await writeAdminAudit(c.env, {
    actorUserId: session.user.id,
    action: 'user.role_changed',
    targetType: 'user',
    targetId,
    metadata: { previousRole: target.role, role: body.role }
  })
  return jsonSuccess(c, { user: updated[0] })
})

app.post('/users/:id/revoke-sessions', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')

  const targetId = c.req.param('id')
  if (targetId === session.user.id) {
    return jsonError(c, 409, 'VALIDATION_ERROR', 'You cannot revoke your own sessions from the control center.')
  }

  const db = drizzle(c.env.DB, { schema })
  const target = await db.query.user.findFirst({ where: eq(schema.user.id, targetId) })
  if (!target) return jsonError(c, 404, 'VALIDATION_ERROR', 'User not found.')
  const sessions = await db.delete(schema.session).where(eq(schema.session.userId, targetId)).returning({ id: schema.session.id })
  await writeAdminAudit(c.env, {
    actorUserId: session.user.id,
    action: 'user.sessions_revoked',
    targetType: 'user',
    targetId,
    metadata: { sessionsRevoked: sessions.length }
  })
  return jsonSuccess(c, { sessionsRevoked: sessions.length })
})

app.get('/invitations', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const db = drizzle(c.env.DB, { schema })
  const invitations = await db.select().from(schema.invitations).orderBy(desc(schema.invitations.createdAt))
  return jsonSuccess(c, { invitations: invitations.map(invitationDto) })
})

app.post('/invitations', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, createInvitationSchema)
  if (isResponse(body)) return body
  if (body.expiresAt && body.expiresAt <= new Date()) return jsonError(c, 400, 'VALIDATION_ERROR', 'Invitation expiry must be in the future.')

  const db = drizzle(c.env.DB, { schema })
  const invitation = { code: newInvitationCode(), used: false, createdAt: new Date(), expiresAt: body.expiresAt ?? null, revokedAt: null, redeemedAt: null, redeemedByUserId: null }
  await db.insert(schema.invitations).values(invitation)
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'invitation.created', targetType: 'invitation', targetId: invitation.code, metadata: { expiresAt: invitation.expiresAt?.toISOString() ?? null } })
  return jsonSuccess(c, { invitation: invitationDto(invitation) }, 201)
})

app.post('/invitations/:code/revoke', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const code = c.req.param('code').trim().toUpperCase()
  const db = drizzle(c.env.DB, { schema })
  const updated = await db.update(schema.invitations).set({ revokedAt: new Date() }).where(and(eq(schema.invitations.code, code), eq(schema.invitations.used, false), isNull(schema.invitations.revokedAt))).returning()
  if (updated.length === 0) return jsonError(c, 404, 'VALIDATION_ERROR', 'An active invitation with that code was not found.')
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'invitation.revoked', targetType: 'invitation', targetId: code })
  return jsonSuccess(c, { invitation: invitationDto(updated[0]) })
})

app.get('/matches', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const query = pagination(c.req.query())
  const db = drizzle(c.env.DB, { schema })
  const [totalResult, matches] = await Promise.all([
    db.select({ value: count() }).from(schema.matches),
    db.select({ id: schema.matches.id, status: schema.matches.status, gamePackId: schema.matches.gamePackId, maxPlayers: schema.matches.maxPlayers, createdAt: schema.matches.createdAt, hostId: schema.user.id, hostName: schema.user.name, hostEmail: schema.user.email }).from(schema.matches).innerJoin(schema.user, eq(schema.matches.hostId, schema.user.id)).orderBy(desc(schema.matches.createdAt)).limit(query.pageSize).offset((query.page - 1) * query.pageSize)
  ])
  return jsonSuccess(c, { matches: pageResult(matches, query.page, query.pageSize, totalResult[0]?.value ?? 0) })
})

app.get('/game-packs', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  return jsonSuccess(c, { packs: [{ id: 'fgc-2026', name: 'Igniting Innovation', version: '1.0.0', status: 'shipped', engineCompatibility: '>=0.1.0' }] })
})

app.get('/audit-log', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const query = pagination(c.req.query())
  const db = drizzle(c.env.DB, { schema })
  const [totalResult, entries] = await Promise.all([
    db.select({ value: count() }).from(schema.adminAuditLog),
    db.select({ id: schema.adminAuditLog.id, action: schema.adminAuditLog.action, targetType: schema.adminAuditLog.targetType, targetId: schema.adminAuditLog.targetId, metadata: schema.adminAuditLog.metadata, createdAt: schema.adminAuditLog.createdAt, actorName: schema.user.name, actorEmail: schema.user.email }).from(schema.adminAuditLog).innerJoin(schema.user, eq(schema.adminAuditLog.actorUserId, schema.user.id)).orderBy(desc(schema.adminAuditLog.createdAt)).limit(query.pageSize).offset((query.page - 1) * query.pageSize)
  ])
  return jsonSuccess(c, { auditLog: pageResult(entries.map((entry) => ({ ...entry, metadata: entry.metadata ? JSON.parse(entry.metadata) : null })), query.page, query.pageSize, totalResult[0]?.value ?? 0) })
})

export default app
