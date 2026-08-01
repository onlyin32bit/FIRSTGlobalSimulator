import { drizzle } from 'drizzle-orm/d1'
import * as schema from '../db/schema'
import type { Bindings } from '../types'

export type AdminAuditAction =
  | 'invitation.created'
  | 'invitation.revoked'
  | 'user.role_changed'
  | 'user.sessions_revoked'

export async function writeAdminAudit(
  env: Bindings,
  input: {
    actorUserId: string
    action: AdminAuditAction
    targetType: 'invitation' | 'user'
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
