import type { Context } from 'hono'
import { z } from 'zod'
import { jsonError } from '../responses'

const trimmedText = (minimum: number, maximum: number) =>
  z.string().trim().min(minimum).max(maximum)

export const inviteCodeSchema = z
  .string()
  .trim()
  .toUpperCase()
  .regex(/^[A-Z0-9]{6}$/, 'Invitation code must contain exactly six letters or digits.')

export const inviteRequestSchema = z.object({
  email: z.string().trim().toLowerCase().email().max(254),
  name: trimmedText(2, 100),
  team: trimmedText(2, 100),
  message: z.string().trim().max(1_000).optional()
}).strict()

export const profileSchema = z.object({
  name: trimmedText(2, 100)
}).strict()

export const robotSchema = z.object({
  name: trimmedText(2, 100),
  buildData: z.record(z.string(), z.unknown()).refine(
    (value) => JSON.stringify(value).length <= 20_000,
    'Robot build data must be 20 KB or smaller.'
  )
}).strict()

export const matchSchema = z.object({
  gamePackId: z.literal('fgc-2026')
}).strict()

export const lobbySlotSchema = z.object({
  slotId: z.enum([
    'red-driver-1', 'red-driver-2', 'red-driver-3', 'red-human',
    'blue-driver-1', 'blue-driver-2', 'blue-driver-3', 'blue-human'
  ]),
  robotId: z.string().trim().min(1).max(255).nullable().optional()
}).strict()

export const lobbyReadySchema = z.object({ ready: z.boolean() }).strict()

export const createInvitationSchema = z.object({
  expiresAt: z.coerce.date().optional()
}).strict()

export const userRoleSchema = z.object({
  role: z.enum(['user', 'admin'])
}).strict()

export const adminListQuerySchema = z.object({
  search: z.string().trim().max(100).optional(),
  page: z.coerce.number().int().min(1).max(100_000).default(1),
  pageSize: z.coerce.number().int().min(1).max(100).default(25)
})

export const updateAdminUserSchema = z.object({
  name: trimmedText(2, 100),
  team: trimmedText(2, 100),
  role: z.enum(['user', 'admin'])
}).strict()

export const disableUserSchema = z.object({
  reason: trimmedText(3, 500)
}).strict()

export const matchStatusSchema = z.enum(['LOBBY', 'IN_PROGRESS', 'FINISHED', 'CANCELLED'])

export const createAdminMatchSchema = z.object({
  hostId: z.string().trim().min(1).max(255),
  gamePackId: z.literal('fgc-2026'),
  maxPlayers: z.coerce.number().int().min(1).max(8),
  status: matchStatusSchema.default('LOBBY')
}).strict()

export const updateAdminMatchSchema = createAdminMatchSchema.extend({
  cancelReason: z.string().trim().min(3).max(500).optional()
}).strict()

export const cancelMatchSchema = z.object({
  reason: trimmedText(3, 500)
}).strict()

export const createGameServerSchema = z.object({
  name: trimmedText(2, 100),
  origin: z.string().trim().url().max(500),
  maxUsers: z.coerce.number().int().min(1).max(100_000).default(50),
  maxMatches: z.coerce.number().int().min(1).max(10_000).default(10),
  slots: z.coerce.number().int().min(1).max(10_000).default(10)
}).strict()

export const updateGameServerSchema = createGameServerSchema.partial().extend({
  status: z.enum(['provisioning', 'online', 'offline', 'disabled']).optional()
}).strict()

export const gameServerHeartbeatSchema = z.object({
  activeUsers: z.coerce.number().int().min(0).max(100_000),
  activeMatches: z.coerce.number().int().min(0).max(10_000),
  slots: z.coerce.number().int().min(1).max(10_000).optional(),
  version: z.string().trim().max(100).optional()
}).strict()

export const updateInvitationSchema = z.object({
  expiresAt: z.coerce.date().nullable().optional()
}).strict()

export async function parseJson<T extends z.ZodType>(
  c: Context,
  schema: T
): Promise<z.output<T> | Response> {
  const body = await c.req.json().catch(() => null)
  const parsed = schema.safeParse(body)
  if (parsed.success) return parsed.data

  return jsonError(
    c,
    400,
    'VALIDATION_ERROR',
    parsed.error.issues[0]?.message || 'The request is invalid.'
  )
}

export function isResponse(value: unknown): value is Response {
  return value instanceof Response
}
