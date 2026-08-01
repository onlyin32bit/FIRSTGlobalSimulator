import { Context } from 'hono'
import { StatusCode } from 'hono/utils/http-status'

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

export function jsonError(
  c: Context, 
  status: StatusCode, 
  code: ErrorCode, 
  message: string, 
  details?: any
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

export function jsonSuccess(c: Context, data: any, status: StatusCode = 200) {
  return c.json({
    success: true,
    data
  }, status)
}
