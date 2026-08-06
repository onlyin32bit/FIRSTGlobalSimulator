import { drizzle } from 'drizzle-orm/d1'
import * as schema from '../db/schema'
import type { Bindings } from '../types'

export type AdminAuditAction =
  | 'invitation.created' | 'invitation.revoked' | 'invitation.updated'
  | 'user.role_changed' | 'user.updated' | 'user.sessions_revoked' | 'user.disabled' | 'user.enabled'
  | 'match.created' | 'match.updated' | 'match.cancelled'
  | 'game_server.created' | 'game_server.updated' | 'game_server.disabled' | 'game_server.commanded' | 'game_server.deleted'

export async function writeAdminAudit(
  env: Bindings,
  input: {
    actorUserId: string
    action: AdminAuditAction
    targetType: 'invitation' | 'user' | 'game_server' | 'match'
    targetId: string
    metadata?: Record<string, string | number | boolean | null>
  }
) {
  const db = drizzle(env.DB, { schema })
  await db.insert(schema.adminAuditLog).values({
    id: crypto.randomUUID(),
    actorUserId: input.actorUserId,
    action: input.action,
    targetType: input.targetType,
    targetId: input.targetId,
    metadata: input.metadata ? JSON.stringify(input.metadata) : null,
    createdAt: new Date()
  })
}
