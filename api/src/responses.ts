import type { Context } from 'hono'
import type { ContentfulStatusCode } from 'hono/utils/http-status'

export type ErrorCode = 
  | 'AUTH_FAILED'
  | 'AUTH_INVALID_TOKEN'
  | 'ROBOT_NOT_FOUND'
  | 'ROBOT_INVALID_BUILD'
  | 'MATCH_NOT_FOUND'
  | 'MATCH_FULL'
  | 'STORAGE_UPLOAD_FAILED'
  | 'VALIDATION_ERROR'
  | 'INTERNAL_ERROR'
  | 'GAME_SERVER_UNAVAILABLE'
  | 'LOBBY_INVALID_STATE'
  | 'LOBBY_SLOT_UNAVAILABLE'

export function jsonError(
  c: Context, 
  status: ContentfulStatusCode,
  code: ErrorCode, 
  message: string, 
  details?: unknown
) {
  return c.json({
    success: false,
    error: {
      code,
      message,
      details
    }
  }, status)
}

export function jsonSuccess<T>(c: Context, data: T, status: ContentfulStatusCode = 200) {
  return c.json({
    success: true,
    data
  }, status)
}
