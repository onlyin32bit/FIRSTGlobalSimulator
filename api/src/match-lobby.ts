import { DurableObject } from 'cloudflare:workers'

export const LOBBY_SLOT_IDS = [
  'red-driver-1',
  'red-driver-2',
  'red-driver-3',
  'red-human',
  'blue-driver-1',
  'blue-driver-2',
  'blue-driver-3',
  'blue-human'
] as const

export type LobbySlotId = typeof LOBBY_SLOT_IDS[number]
export type LobbyStatus = 'LOBBY' | 'STARTING' | 'IN_PROGRESS' | 'FINISHED' | 'CANCELLED'
export type LobbyAlliance = 'red' | 'blue'
export type LobbyRole = 'driver' | 'human-player'

export type LobbyOccupant = {
  userId: string
  name: string
  teamName: string | null
  robotId: string | null
  ready: boolean
}

export type LobbySlot = {
  id: LobbySlotId
  alliance: LobbyAlliance
  role: LobbyRole
  label: string
  occupant: LobbyOccupant | null
}

export type LobbyState = {
  matchId: string
  hostId: string
  status: LobbyStatus
  slots: LobbySlot[]
  error: string | null
  updatedAt: number
}

export type LobbyUser = Pick<LobbyOccupant, 'userId' | 'name' | 'teamName'>

const slot = (id: LobbySlotId): LobbySlot => {
  const alliance: LobbyAlliance = id.startsWith('red-') ? 'red' : 'blue'
  const role: LobbyRole = id.endsWith('human') ? 'human-player' : 'driver'
  const index = role === 'driver' ? id.at(-1) : undefined
  return {
    id,
    alliance,
    role,
    label: role === 'driver' ? `Driver ${index}` : 'Human player',
    occupant: null
  }
}

const createState = (matchId: string, hostId: string): LobbyState => ({
  matchId,
  hostId,
  status: 'LOBBY',
  slots: LOBBY_SLOT_IDS.map(slot),
  error: null,
  updatedAt: Date.now()
})

/** One durable, strongly-consistent pre-match lobby per database match. */
export class MatchLobby extends DurableObject<Cloudflare.Env> {
  constructor(ctx: DurableObjectState, env: Cloudflare.Env) {
    super(ctx, env)
    ctx.blockConcurrencyWhile(async () => {
      this.ctx.storage.sql.exec(
        'CREATE TABLE IF NOT EXISTS lobby_state (id INTEGER PRIMARY KEY CHECK (id = 1), state TEXT NOT NULL)'
      )
    })
  }

  private read(): LobbyState | null {
    // `.one()` throws when a newly created Durable Object has no row yet.
    // Initialization intentionally reads before writing, so use a safe
    // zero-or-one row query here.
    const row = this.ctx.storage.sql
      .exec<{ state: string }>('SELECT state FROM lobby_state WHERE id = 1')
      .toArray()[0]
    return row ? JSON.parse(row.state) as LobbyState : null
  }

  private write(state: LobbyState) {
    state.updatedAt = Date.now()
    this.ctx.storage.sql.exec(
      'INSERT INTO lobby_state (id, state) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET state = excluded.state',
      JSON.stringify(state)
    )
    this.broadcast(state)
  }

  private broadcast(state: LobbyState) {
    const message = JSON.stringify({ type: 'lobby_state', state })
    for (const socket of this.ctx.getWebSockets()) {
      if (socket.readyState === WebSocket.OPEN) socket.send(message)
    }
  }

  private requireLobby(): LobbyState {
    const state = this.read()
    if (!state) throw new Error('Lobby has not been initialized.')
    return state
  }

  private assertMutable(state: LobbyState) {
    if (state.status !== 'LOBBY') throw new Error('This lobby is no longer accepting changes.')
  }

  async initialize(matchId: string, hostId: string): Promise<LobbyState> {
    const state = this.read()
    if (state) return state
    const created = createState(matchId, hostId)
    this.write(created)
    return created
  }

  async getState(): Promise<LobbyState> {
    return this.requireLobby()
  }

  async claimSlot(user: LobbyUser, slotId: LobbySlotId, robotId: string | null): Promise<LobbyState> {
    const state = this.requireLobby()
    this.assertMutable(state)
    const target = state.slots.find((candidate) => candidate.id === slotId)
    if (!target) throw new Error('Unknown lobby slot.')
    if (target.occupant && target.occupant.userId !== user.userId) throw new Error('That slot is already occupied.')
    if (target.role === 'driver' && !robotId) throw new Error('Choose a robot before taking a driver slot.')
    if (target.role === 'human-player' && robotId) throw new Error('Human-player slots cannot use a robot.')

    for (const candidate of state.slots) {
      if (candidate.id !== target.id && candidate.occupant?.userId === user.userId) candidate.occupant = null
    }
    target.occupant = { ...user, robotId: target.role === 'driver' ? robotId : null, ready: false }
    state.error = null
    this.write(state)
    return state
  }

  async leave(userId: string): Promise<LobbyState> {
    const state = this.requireLobby()
    this.assertMutable(state)
    for (const candidate of state.slots) {
      if (candidate.occupant?.userId === userId) candidate.occupant = null
    }
    this.write(state)
    return state
  }

  async setReady(userId: string, ready: boolean): Promise<LobbyState> {
    const state = this.requireLobby()
    this.assertMutable(state)
    const slot = state.slots.find((candidate) => candidate.occupant?.userId === userId)
    if (!slot?.occupant) throw new Error('Choose a slot before setting ready.')
    slot.occupant.ready = ready
    state.error = null
    this.write(state)
    return state
  }

  async beginStart(hostId: string): Promise<LobbyState> {
    const state = this.requireLobby()
    if (state.hostId !== hostId) throw new Error('Only the host can start this match.')
    this.assertMutable(state)
    if (state.slots.some((slot) => !slot.occupant || !slot.occupant.ready)) {
      throw new Error('All eight players must choose a slot and be ready before starting.')
    }
    state.status = 'STARTING'
    state.error = null
    this.write(state)
    return state
  }

  async markStarted(): Promise<LobbyState> {
    const state = this.requireLobby()
    if (state.status !== 'STARTING') throw new Error('Lobby is not starting.')
    state.status = 'IN_PROGRESS'
    this.write(state)
    return state
  }

  /** Development/admin escape hatch; normal players must use the ready check. */
  async forceStart(): Promise<LobbyState> {
    const state = this.requireLobby()
    if (state.status === 'IN_PROGRESS') return state
    if (state.status !== 'LOBBY') throw new Error('Lobby cannot be entered immediately in its current state.')
    state.status = 'IN_PROGRESS'
    state.error = null
    this.write(state)
    return state
  }

  async reopen(message: string): Promise<LobbyState> {
    const state = this.requireLobby()
    if (state.status !== 'STARTING') return state
    state.status = 'LOBBY'
    state.error = message
    this.write(state)
    return state
  }

  async fetch(request: Request): Promise<Response> {
    if (new URL(request.url).pathname !== '/ws' || request.headers.get('Upgrade') !== 'websocket') {
      return new Response('Not found', { status: 404 })
    }
    if (!request.headers.get('X-Lobby-User-Id')) return new Response('Unauthorized', { status: 401 })
    const pair = new WebSocketPair()
    const [client, server] = Object.values(pair)
    this.ctx.acceptWebSocket(server)
    server.serializeAttachment({ userId: request.headers.get('X-Lobby-User-Id') })
    server.send(JSON.stringify({ type: 'lobby_state', state: this.requireLobby() }))
    return new Response(null, { status: 101, webSocket: client })
  }

  async webSocketMessage(socket: WebSocket) {
    // Lobby mutations stay on authenticated HTTP routes. The socket is a state feed only.
    socket.send(JSON.stringify({ type: 'lobby_state', state: this.requireLobby() }))
  }
}
