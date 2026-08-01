import type { AuthEnvironment } from './auth'

export type Bindings = AuthEnvironment & {
  JWT_SECRET?: string
  N8N_WEBHOOK_URL?: string
  GAME_SERVER_ORIGIN?: string
}

export type AuthenticatedUser = {
  id: string
  role: 'user' | 'admin'
  team?: string | null
}
