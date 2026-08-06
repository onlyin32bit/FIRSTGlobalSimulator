import { Hono } from 'hono'
import { and, asc, count, desc, eq, isNull, like, or, sql } from 'drizzle-orm'
import { drizzle } from 'drizzle-orm/d1'
import * as schema from '../db/schema'
import { writeAdminAudit } from '../lib/audit'
import {
  adminListQuerySchema,
  cancelMatchSchema,
  createAdminMatchSchema,
  createInvitationSchema,
  disableUserSchema,
  isResponse,
  parseJson,
  updateAdminMatchSchema,
  updateAdminUserSchema,
  updateInvitationSchema,
  userRoleSchema,
  createGameServerSchema,
  updateGameServerSchema,
  gameServerCommandSchema
} from '../lib/validation'
import { requireAdmin } from '../middleware'
import { jsonError, jsonSuccess } from '../responses'
import type { Bindings } from '../types'
import { hashKey } from './game-servers'

const app = new Hono<{ Bindings: Bindings }>()

function gameServerDto(server: typeof schema.gameServers.$inferSelect) {
  const lastHeartbeat = server.lastHeartbeatAt?.getTime() ?? 0
  const healthy = Boolean(server.lastHeartbeatAt && Date.now() - lastHeartbeat < 30_000 && !server.disabledAt)
  return {
    ...server,
    keyHash: undefined,
    runtime: server.runtimeJson ? JSON.parse(server.runtimeJson) : null,
    runtimeJson: undefined,
    health: healthy ? 'online' : server.disabledAt ? 'disabled' : 'offline'
  }
}

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
      disabledAt: schema.user.disabledAt,
      disabledReason: schema.user.disabledReason,
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

app.patch('/users/:id', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, updateAdminUserSchema)
  if (isResponse(body)) return body

  const targetId = c.req.param('id')
  if (targetId === session.user.id && body.role !== 'admin') return jsonError(c, 409, 'VALIDATION_ERROR', 'You cannot remove your own administrator role.')
  const db = drizzle(c.env.DB, { schema })
  const target = await db.query.user.findFirst({ where: eq(schema.user.id, targetId) })
  if (!target) return jsonError(c, 404, 'VALIDATION_ERROR', 'User not found.')
  if (target.role === 'admin' && body.role === 'user') {
    const admins = await db.select({ value: count() }).from(schema.user).where(and(eq(schema.user.role, 'admin'), isNull(schema.user.disabledAt)))
    if ((admins[0]?.value ?? 0) <= 1) return jsonError(c, 409, 'VALIDATION_ERROR', 'At least one enabled administrator must remain.')
  }

  const updated = await db.update(schema.user).set({ name: body.name, team: body.team, role: body.role, updatedAt: new Date() }).where(eq(schema.user.id, targetId)).returning()
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'user.updated', targetType: 'user', targetId, metadata: { role: body.role, nameChanged: target.name !== body.name, teamChanged: target.team !== body.team } })
  return jsonSuccess(c, { user: updated[0] })
})

app.post('/users/:id/disable', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, disableUserSchema)
  if (isResponse(body)) return body
  const targetId = c.req.param('id')
  if (targetId === session.user.id) return jsonError(c, 409, 'VALIDATION_ERROR', 'You cannot disable your own account.')

  const db = drizzle(c.env.DB, { schema })
  const target = await db.query.user.findFirst({ where: eq(schema.user.id, targetId) })
  if (!target) return jsonError(c, 404, 'VALIDATION_ERROR', 'User not found.')
  if (target.disabledAt) return jsonError(c, 409, 'VALIDATION_ERROR', 'This account is already disabled.')
  if (target.role === 'admin') {
    const admins = await db.select({ value: count() }).from(schema.user).where(and(eq(schema.user.role, 'admin'), isNull(schema.user.disabledAt)))
    if ((admins[0]?.value ?? 0) <= 1) return jsonError(c, 409, 'VALIDATION_ERROR', 'At least one enabled administrator must remain.')
  }

  const now = new Date()
  await db.update(schema.user).set({ disabledAt: now, disabledReason: body.reason, updatedAt: now }).where(eq(schema.user.id, targetId))
  const sessions = await db.delete(schema.session).where(eq(schema.session.userId, targetId)).returning({ id: schema.session.id })
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'user.disabled', targetType: 'user', targetId, metadata: { reason: body.reason, sessionsRevoked: sessions.length } })
  return jsonSuccess(c, { disabledAt: now, sessionsRevoked: sessions.length })
})

app.post('/users/:id/enable', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const targetId = c.req.param('id')
  const db = drizzle(c.env.DB, { schema })
  const target = await db.query.user.findFirst({ where: eq(schema.user.id, targetId) })
  if (!target) return jsonError(c, 404, 'VALIDATION_ERROR', 'User not found.')
  if (!target.disabledAt) return jsonError(c, 409, 'VALIDATION_ERROR', 'This account is already enabled.')
  await db.update(schema.user).set({ disabledAt: null, disabledReason: null, updatedAt: new Date() }).where(eq(schema.user.id, targetId))
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'user.enabled', targetType: 'user', targetId })
  return jsonSuccess(c, { message: 'Account enabled.' })
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

app.patch('/invitations/:code', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, updateInvitationSchema)
  if (isResponse(body)) return body
  if (body.expiresAt && body.expiresAt <= new Date()) return jsonError(c, 400, 'VALIDATION_ERROR', 'Invitation expiry must be in the future.')
  const code = c.req.param('code').trim().toUpperCase()
  const db = drizzle(c.env.DB, { schema })
  const invite = await db.query.invitations.findFirst({ where: eq(schema.invitations.code, code) })
  if (!invite) return jsonError(c, 404, 'VALIDATION_ERROR', 'Invitation not found.')
  if (invite.used || invite.revokedAt) return jsonError(c, 409, 'VALIDATION_ERROR', 'Only active unused invitations can be edited.')
  const updated = await db.update(schema.invitations).set({ expiresAt: body.expiresAt === undefined ? invite.expiresAt : body.expiresAt }).where(eq(schema.invitations.code, code)).returning()
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'invitation.updated', targetType: 'invitation', targetId: code, metadata: { expiresAt: updated[0].expiresAt?.toISOString() ?? null } })
  return jsonSuccess(c, { invitation: invitationDto(updated[0]) })
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

app.post('/matches', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, createAdminMatchSchema)
  if (isResponse(body)) return body
  const db = drizzle(c.env.DB, { schema })
  const host = await db.query.user.findFirst({ where: eq(schema.user.id, body.hostId) })
  if (!host || host.disabledAt) return jsonError(c, 400, 'VALIDATION_ERROR', 'Select an enabled match host.')
  const now = new Date()
  const match = { id: crypto.randomUUID(), hostId: body.hostId, gamePackId: body.gamePackId, status: body.status, maxPlayers: body.maxPlayers, createdAt: now, updatedAt: now, cancelledAt: body.status === 'CANCELLED' ? now : null, cancelReason: null }
  await db.insert(schema.matches).values(match)
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'match.created', targetType: 'match', targetId: match.id, metadata: { hostId: body.hostId, status: body.status, maxPlayers: body.maxPlayers } })
  return jsonSuccess(c, { match }, 201)
})

app.patch('/matches/:id', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, updateAdminMatchSchema)
  if (isResponse(body)) return body
  const matchId = c.req.param('id')
  const db = drizzle(c.env.DB, { schema })
  const match = await db.query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  const host = await db.query.user.findFirst({ where: eq(schema.user.id, body.hostId) })
  if (!host || host.disabledAt) return jsonError(c, 400, 'VALIDATION_ERROR', 'Select an enabled match host.')
  const now = new Date()
  const updated = await db.update(schema.matches).set({ hostId: body.hostId, gamePackId: body.gamePackId, maxPlayers: body.maxPlayers, status: body.status, updatedAt: now, cancelledAt: body.status === 'CANCELLED' ? (match.cancelledAt ?? now) : null, cancelReason: body.status === 'CANCELLED' ? (body.cancelReason ?? match.cancelReason) : null }).where(eq(schema.matches.id, matchId)).returning()
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'match.updated', targetType: 'match', targetId: matchId, metadata: { status: body.status, maxPlayers: body.maxPlayers, hostId: body.hostId } })
  return jsonSuccess(c, { match: updated[0] })
})

app.post('/matches/:id/cancel', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, cancelMatchSchema)
  if (isResponse(body)) return body
  const matchId = c.req.param('id')
  const db = drizzle(c.env.DB, { schema })
  const match = await db.query.matches.findFirst({ where: eq(schema.matches.id, matchId) })
  if (!match) return jsonError(c, 404, 'MATCH_NOT_FOUND', 'Match not found.')
  if (match.status === 'CANCELLED') return jsonError(c, 409, 'VALIDATION_ERROR', 'Match is already cancelled.')
  const now = new Date()
  const updated = await db.update(schema.matches).set({ status: 'CANCELLED', cancelledAt: now, cancelReason: body.reason, updatedAt: now }).where(eq(schema.matches.id, matchId)).returning()
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'match.cancelled', targetType: 'match', targetId: matchId, metadata: { reason: body.reason } })
  return jsonSuccess(c, { match: updated[0] })
})

app.get('/matches', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const query = pagination(c.req.query())
  const db = drizzle(c.env.DB, { schema })
  const [totalResult, matches] = await Promise.all([
    db.select({ value: count() }).from(schema.matches),
    db.select({ id: schema.matches.id, status: schema.matches.status, gamePackId: schema.matches.gamePackId, maxPlayers: schema.matches.maxPlayers, createdAt: schema.matches.createdAt, updatedAt: schema.matches.updatedAt, cancelledAt: schema.matches.cancelledAt, cancelReason: schema.matches.cancelReason, hostId: schema.user.id, hostName: schema.user.name, hostEmail: schema.user.email }).from(schema.matches).innerJoin(schema.user, eq(schema.matches.hostId, schema.user.id)).orderBy(desc(schema.matches.createdAt)).limit(query.pageSize).offset((query.page - 1) * query.pageSize)
  ])
  return jsonSuccess(c, { matches: pageResult(matches, query.page, query.pageSize, totalResult[0]?.value ?? 0) })
})

app.get('/game-packs', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  return jsonSuccess(c, { packs: [{ id: 'fgc-2026', name: 'Igniting Innovation', version: '1.0.0', status: 'shipped', engineCompatibility: '>=0.1.0' }] })
})

app.get('/game-servers', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const db = drizzle(c.env.DB, { schema })
  const servers = await db.select().from(schema.gameServers).orderBy(desc(schema.gameServers.createdAt))
  return jsonSuccess(c, { servers: servers.map(gameServerDto) })
})

app.post('/game-servers', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, createGameServerSchema)
  if (isResponse(body)) return body
  const key = `fgc_${crypto.randomUUID().replaceAll('-', '')}${crypto.randomUUID().replaceAll('-', '')}`
  const now = new Date()
  const origin = body.origin.replace(/\/$/, '')
  const server = { id: crypto.randomUUID(), name: new URL(origin).hostname, origin, keyHash: await hashKey(key), maxUsers: 1, maxMatches: 1, slots: 1, activeUsers: 0, activeMatches: 0, status: 'provisioning', lastHeartbeatAt: null, createdAt: now, updatedAt: now, disabledAt: null }
  const db = drizzle(c.env.DB, { schema })
  await db.insert(schema.gameServers).values(server)
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'game_server.created', targetType: 'game_server', targetId: server.id, metadata: { name: server.name, origin: server.origin } })
  return jsonSuccess(c, { server: gameServerDto(server), key }, 201)
})

app.patch('/game-servers/:id', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, updateGameServerSchema)
  if (isResponse(body)) return body
  const id = c.req.param('id')
  const db = drizzle(c.env.DB, { schema })
  const current = await db.query.gameServers.findFirst({ where: eq(schema.gameServers.id, id) })
  if (!current) return jsonError(c, 404, 'VALIDATION_ERROR', 'Game server not found.')
  const now = new Date()
  const updated = await db.update(schema.gameServers).set({ ...body, origin: body.origin?.replace(/\/$/, ''), disabledAt: body.status === 'disabled' ? (current.disabledAt ?? now) : body.status ? null : current.disabledAt, updatedAt: now }).where(eq(schema.gameServers.id, id)).returning()
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: body.status === 'disabled' ? 'game_server.disabled' : 'game_server.updated', targetType: 'game_server', targetId: id, metadata: { name: current.name, status: body.status ?? current.status } })
  return jsonSuccess(c, { server: gameServerDto(updated[0]) })
})

app.get('/game-servers/:id', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const db = drizzle(c.env.DB, { schema })
  const id = c.req.param('id')
  const server = await db.query.gameServers.findFirst({ where: eq(schema.gameServers.id, id) })
  if (!server) return jsonError(c, 404, 'VALIDATION_ERROR', 'Game server not found.')
  const [instances, matches, commands] = await Promise.all([
    db.select().from(schema.gameServerInstances).where(eq(schema.gameServerInstances.serverId, id)).orderBy(desc(schema.gameServerInstances.lastSeenAt)),
    db.select().from(schema.gameServerRuntimeMatches).where(eq(schema.gameServerRuntimeMatches.serverId, id)).orderBy(desc(schema.gameServerRuntimeMatches.updatedAt)),
    db.select().from(schema.gameServerCommands).where(eq(schema.gameServerCommands.serverId, id)).orderBy(desc(schema.gameServerCommands.createdAt)).limit(30)
  ])
  return jsonSuccess(c, { server: gameServerDto(server), instances, matches, commands: commands.map((command) => ({ ...command, payload: JSON.parse(command.payload) })) })
})

app.post('/game-servers/:id/commands', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const body = await parseJson(c, gameServerCommandSchema)
  if (isResponse(body)) return body
  const id = c.req.param('id')
  const db = drizzle(c.env.DB, { schema })
  const server = await db.query.gameServers.findFirst({ where: eq(schema.gameServers.id, id) })
  if (!server) return jsonError(c, 404, 'VALIDATION_ERROR', 'Game server not found.')
  if (server.disabledAt) return jsonError(c, 409, 'VALIDATION_ERROR', 'Enable this game server before sending control commands.')
  const now = new Date()
  const command = { id: crypto.randomUUID(), serverId: id, type: body.type, payload: JSON.stringify({ matchId: body.matchId, userId: body.userId }), status: 'pending', error: null, createdAt: now, deliveredAt: null, completedAt: null }
  await db.insert(schema.gameServerCommands).values(command)
  if (body.type === 'stop_match' && body.matchId) {
    await db.update(schema.matches).set({ status: 'CANCELLED', cancelledAt: now, cancelReason: 'Stopped by an administrator.', updatedAt: now })
      .where(and(eq(schema.matches.id, body.matchId), eq(schema.matches.gameServerId, id)))
  }
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'game_server.commanded', targetType: 'game_server', targetId: id, metadata: { type: body.type, matchId: body.matchId ?? null, userId: body.userId ?? null } })
  return jsonSuccess(c, { command: { ...command, payload: JSON.parse(command.payload) } }, 202)
})

app.delete('/game-servers/:id', async (c) => {
  const session = await requireAdmin(c)
  if (!session) return jsonError(c, 403, 'AUTH_FAILED', 'Administrator access is required.')
  const id = c.req.param('id')
  const db = drizzle(c.env.DB, { schema })
  const server = await db.query.gameServers.findFirst({ where: eq(schema.gameServers.id, id) })
  if (!server) return jsonError(c, 404, 'VALIDATION_ERROR', 'Game server not found.')
  const now = new Date()
  await db.update(schema.matches).set({ gameServerId: null, status: 'CANCELLED', cancelledAt: now, cancelReason: 'Assigned game server was deleted.', updatedAt: now }).where(eq(schema.matches.gameServerId, id))
  await db.delete(schema.gameServerRuntimeMatches).where(eq(schema.gameServerRuntimeMatches.serverId, id))
  await db.delete(schema.gameServerInstances).where(eq(schema.gameServerInstances.serverId, id))
  await db.delete(schema.gameServerCommands).where(eq(schema.gameServerCommands.serverId, id))
  await db.delete(schema.gameServers).where(eq(schema.gameServers.id, id))
  await writeAdminAudit(c.env, { actorUserId: session.user.id, action: 'game_server.deleted', targetType: 'game_server', targetId: id, metadata: { name: server.name, origin: server.origin } })
  return jsonSuccess(c, { deleted: true })
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
