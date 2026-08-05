import type { AuthEnvironment } from './auth'
import type { MatchLobby } from './match-lobby'

export type Bindings = AuthEnvironment & {
  MATCH_LOBBY: DurableObjectNamespace<MatchLobby>
  JWT_SECRET?: string
  N8N_WEBHOOK_URL?: string
  /** API-owned, versioned game-pack files deployed with this Worker. */
  PACK_ASSETS: Fetcher
}

export type AuthenticatedUser = {
  id: string
  role: 'user' | 'admin'
  team?: string | null
}
