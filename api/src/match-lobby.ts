import { DurableObject } from 'cloudflare:workers'

export const LOBBY_SLOT_IDS = [
  'red-driver-1', 'red-driver-2', 'red-driver-3', 'red-human',
  'blue-driver-1', 'blue-driver-2', 'blue-driver-3', 'blue-human'
] as const

export type LobbySlotId = typeof LOBBY_SLOT_IDS[number]
export type LobbyStatus = 'LOBBY' | 'STARTING' | 'IN_PROGRESS' | 'FINISHED' | 'CANCELLED'
export type LobbyAlliance = 'red' | 'blue'
export type LobbyRole = 'driver' | 'human-player'
export type MatchVisibility = 'private' | 'unlisted' | 'public'

export type LobbyOccupant = { userId: string; name: string; teamName: string | null; robotId: string | null; ready: boolean }
export type LobbySlot = { id: LobbySlotId; alliance: LobbyAlliance; role: LobbyRole; label: string; occupant: LobbyOccupant | null }
export type BootstrapParticipant = {
  userId: string; name: string; teamName: string | null; role: LobbyRole; slotId: LobbySlotId; alliance: LobbyAlliance
  robotId: string | null; robotRevision: number | null; robotData: string | null
}
export type MatchBootstrap = {
  matchId: string; hostId: string; assignedServerId: string; gamePackId: string; gamePackVersion: string
  matchSeed: number; startsAt: number; durationSeconds: number; maxPlayers: number
  scenarioId: string; visibility: MatchVisibility; participants: BootstrapParticipant[]
}
export type MatchAllocation = {
  serverId: string
  leaseId: string
  expiresAt: number
  epoch: number
}
export type MatchCommand = {
  id: string
  serverId: string
  type: string
  payload: Record<string, unknown>
  createdAt: number
  attempts: number
  status: 'pending' | 'leased' | 'completed' | 'failed'
  leaseId: string | null
  leaseExpiresAt: number | null
  error: string | null
}
export type ClaimedMatchCommand = Pick<MatchCommand, 'id' | 'type' | 'payload'> & { deliveryLeaseId: string }
export type LobbyState = {
  matchId: string
  hostId: string
  status: LobbyStatus
  slots: LobbySlot[]
  error: string | null
  allocation?: MatchAllocation
  bootstrap?: MatchBootstrap
  commands?: MatchCommand[]
  updatedAt: number
}
export type LobbyUser = Pick<LobbyOccupant, 'userId' | 'name' | 'teamName'>

const slot = (id: LobbySlotId): LobbySlot => {
  const alliance: LobbyAlliance = id.startsWith('red-') ? 'red' : 'blue'
  const role: LobbyRole = id.endsWith('human') ? 'human-player' : 'driver'
  return { id, alliance, role, label: role === 'driver' ? `Driver ${id.at(-1)}` : 'Human player', occupant: null }
}

const createState = (matchId: string, hostId: string): LobbyState => ({
  matchId, hostId, status: 'LOBBY', slots: LOBBY_SLOT_IDS.map(slot), error: null, updatedAt: Date.now()
})

const boundedLease = (milliseconds: number) => Math.max(5_000, Math.min(milliseconds, 120_000))

export class MatchLobby extends DurableObject<Cloudflare.Env> {
  constructor(ctx: DurableObjectState, env: Cloudflare.Env) {
    super(ctx, env)
    ctx.blockConcurrencyWhile(async () => {
      this.ctx.storage.sql.exec('CREATE TABLE IF NOT EXISTS lobby_state (id INTEGER PRIMARY KEY CHECK (id = 1), state TEXT NOT NULL)')
    })
  }

  private read(): LobbyState | null {
    const row = this.ctx.storage.sql.exec<{ state: string }>('SELECT state FROM lobby_state WHERE id = 1').toArray()[0]
    return row ? JSON.parse(row.state) as LobbyState : null
  }

  private write(state: LobbyState, broadcast = true) {
    state.updatedAt = Date.now()
    this.ctx.storage.sql.exec('INSERT INTO lobby_state (id, state) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET state = excluded.state', JSON.stringify(state))
    if (broadcast) this.broadcast(state)
  }

  private broadcast(state: LobbyState) {
    const message = JSON.stringify({ type: 'lobby_state', state })
    for (const socket of this.ctx.getWebSockets()) if (socket.readyState === WebSocket.OPEN) socket.send(message)
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

  async getState(): Promise<LobbyState> { return this.requireLobby() }

  /** A match has one host lease at a time. The DO serializes lease takeover. */
  async acquireServerLease(serverId: string, leaseMilliseconds = 30_000): Promise<MatchAllocation> {
    const state = this.requireLobby()
    const now = Date.now()
    const current = state.allocation
    if (current && current.expiresAt > now && current.serverId !== serverId) return current
    if (state.bootstrap && state.bootstrap.assignedServerId !== serverId) {
      throw new Error('The locked bootstrap belongs to another game server.')
    }

    const allocation: MatchAllocation = current?.serverId === serverId && current.expiresAt > now
      ? { ...current, expiresAt: now + boundedLease(leaseMilliseconds) }
      : { serverId, leaseId: crypto.randomUUID(), expiresAt: now + boundedLease(leaseMilliseconds), epoch: (current?.epoch ?? 0) + 1 }
    state.allocation = allocation
    this.write(state)
    return allocation
  }

  async getAllocation(): Promise<MatchAllocation | null> {
    const allocation = this.requireLobby().allocation
    return allocation && allocation.expiresAt > Date.now() ? allocation : null
  }

  async renewServerLease(serverId: string, leaseMilliseconds = 30_000): Promise<boolean> {
    const state = this.requireLobby()
    const allocation = state.allocation
    if (!allocation || allocation.serverId !== serverId || allocation.expiresAt <= Date.now()) return false
    allocation.expiresAt = Date.now() + boundedLease(leaseMilliseconds)
    this.write(state, false)
    return true
  }

  async claimSlot(user: LobbyUser, slotId: LobbySlotId, robotId: string | null): Promise<LobbyState> {
    const state = this.requireLobby()
    this.assertMutable(state)
    const target = state.slots.find((candidate) => candidate.id === slotId)
    if (!target) throw new Error('Unknown lobby slot.')
    if (target.occupant && target.occupant.userId !== user.userId) throw new Error('That slot is already occupied.')
    if (target.role === 'driver' && !robotId) throw new Error('Choose a robot before taking a driver slot.')
    if (target.role === 'human-player' && robotId) throw new Error('Human-player slots cannot use a robot.')
    for (const candidate of state.slots) if (candidate.id !== target.id && candidate.occupant?.userId === user.userId) candidate.occupant = null
    target.occupant = { ...user, robotId: target.role === 'driver' ? robotId : null, ready: false }
    state.error = null
    this.write(state)
    return state
  }

  async leave(userId: string): Promise<LobbyState> {
    const state = this.requireLobby()
    this.assertMutable(state)
    for (const candidate of state.slots) if (candidate.occupant?.userId === userId) candidate.occupant = null
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
    const occupied = state.slots.filter((slot) => slot.occupant)
    const alliancesReady = ['red', 'blue'].every((alliance) => occupied.some((slot) => slot.alliance === alliance && slot.occupant?.ready))
    if (!alliancesReady || occupied.some((slot) => !slot.occupant?.ready)) throw new Error('One ready player per alliance is required to start.')
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

  /** Locks the roster and each selected robot revision once. */
  async lockBootstrap(input: Omit<MatchBootstrap, 'matchId' | 'hostId'> & { bootstrapCommandId: string }): Promise<MatchBootstrap> {
    const state = this.requireLobby()
    if (state.bootstrap) return state.bootstrap
    if (state.status !== 'STARTING' && state.status !== 'IN_PROGRESS') throw new Error('Lobby is not ready to lock.')
    if (!state.allocation || state.allocation.serverId !== input.assignedServerId || state.allocation.expiresAt <= Date.now()) {
      throw new Error('The game-server allocation lease is no longer valid.')
    }
    const roster = state.slots.flatMap((slot) => slot.occupant ? [{
      userId: slot.occupant.userId, name: slot.occupant.name, teamName: slot.occupant.teamName,
      role: slot.role, slotId: slot.id, alliance: slot.alliance, robotId: slot.occupant.robotId
    }] : [])
    if (roster.length > input.maxPlayers) throw new Error('The roster exceeds the match player limit.')
    if (input.participants.length !== roster.length || input.participants.some((participant, index) => {
      const slot = roster[index]
      return !slot || participant.userId !== slot.userId || participant.role !== slot.role || participant.slotId !== slot.slotId || participant.alliance !== slot.alliance || participant.robotId !== slot.robotId
    })) throw new Error('The robot revision snapshot does not match the locked roster.')
    const { bootstrapCommandId, ...bootstrapInput } = input
    state.bootstrap = { ...bootstrapInput, matchId: state.matchId, hostId: state.hostId }
    const commands = state.commands ??= []
    commands.push({
      id: bootstrapCommandId,
      serverId: input.assignedServerId,
      type: 'bootstrap_match',
      payload: { matchId: state.matchId },
      createdAt: Date.now(),
      attempts: 0,
      status: 'pending',
      leaseId: null,
      leaseExpiresAt: null,
      error: null
    })
    state.status = 'IN_PROGRESS'
    state.error = null
    this.write(state)
    return state.bootstrap
  }

  async getBootstrap(): Promise<MatchBootstrap> {
    const bootstrap = this.requireLobby().bootstrap
    if (!bootstrap) throw new Error('This match has not been assigned to a game server.')
    return bootstrap
  }

  /** Persists a control command before it can be exposed to a game server. */
  async enqueueCommand(command: Omit<MatchCommand, 'attempts' | 'status' | 'leaseId' | 'leaseExpiresAt' | 'error'>): Promise<MatchCommand> {
    const state = this.requireLobby()
    const allocation = state.allocation
    if (!allocation || allocation.serverId !== command.serverId || allocation.expiresAt <= Date.now()) {
      throw new Error('This game server does not hold the match allocation lease.')
    }
    const commands = state.commands ??= []
    const existing = commands.find((candidate) => candidate.id === command.id)
    if (existing) return existing
    const queued: MatchCommand = {
      ...command,
      attempts: 0,
      status: 'pending',
      leaseId: null,
      leaseExpiresAt: null,
      error: null
    }
    commands.push(queued)
    this.write(state)
    return queued
  }

  /** Claims work atomically. Expired delivery leases are safe to retry. */
  async claimCommands(serverId: string, limit = 50, leaseMilliseconds = 15_000): Promise<ClaimedMatchCommand[]> {
    const state = this.requireLobby()
    const allocation = state.allocation
    const now = Date.now()
    if (!allocation || allocation.serverId !== serverId || allocation.expiresAt <= now) return []
    allocation.expiresAt = now + boundedLease(leaseMilliseconds * 2)
    const claimed: ClaimedMatchCommand[] = []
    for (const command of state.commands ?? []) {
      if (claimed.length >= Math.max(1, Math.min(limit, 50))) break
      const canClaim = command.serverId === serverId && (
        command.status === 'pending' ||
        (command.status === 'leased' && (command.leaseExpiresAt ?? 0) <= now)
      )
      if (!canClaim) continue
      const deliveryLeaseId = crypto.randomUUID()
      command.status = 'leased'
      command.leaseId = deliveryLeaseId
      command.leaseExpiresAt = now + boundedLease(leaseMilliseconds)
      command.attempts += 1
      claimed.push({ id: command.id, type: command.type, payload: command.payload, deliveryLeaseId })
    }
    this.write(state, claimed.length > 0)
    return claimed
  }

  /** Only the current delivery lease may settle a command. */
  async settleCommand(serverId: string, commandId: string, deliveryLeaseId: string, ok: boolean, error: string | null): Promise<boolean> {
    const state = this.requireLobby()
    const command = state.commands?.find((candidate) => candidate.id === commandId)
    if (!command || command.serverId !== serverId || command.status !== 'leased' || command.leaseId !== deliveryLeaseId) return false
    command.status = ok ? 'completed' : 'failed'
    command.leaseId = null
    command.leaseExpiresAt = null
    command.error = ok ? null : error || 'Host rejected the command.'
    this.write(state)
    return true
  }

  async complete(reason: string, cancelled = false): Promise<LobbyState> {
    const state = this.requireLobby()
    state.status = cancelled ? 'CANCELLED' : 'FINISHED'
    state.error = reason
    this.write(state)
    return state
  }

  async forceStart(): Promise<LobbyState> {
    const state = this.requireLobby()
    if (state.status === 'IN_PROGRESS') return state
    if (state.status !== 'LOBBY') throw new Error('Lobby cannot be entered immediately in its current state.')
    state.status = 'IN_PROGRESS'
    state.error = null
    this.write(state)
    return state
  }

  /** Admin-start gets a real locked test driver, never a synthetic ticket. */
  async assignAdminDriver(user: LobbyUser, robotId: string): Promise<LobbyState> {
    const state = this.requireLobby()
    this.assertMutable(state)
    const existing = state.slots.find((slot) => slot.occupant?.userId === user.userId)
    if (existing) return state
    const target = state.slots.find((slot) => slot.id === 'red-driver-1')
    if (!target || target.occupant) throw new Error('The admin test station is unavailable.')
    target.occupant = { ...user, robotId, ready: true }
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
    if (new URL(request.url).pathname !== '/ws' || request.headers.get('Upgrade') !== 'websocket') return new Response('Not found', { status: 404 })
    if (!request.headers.get('X-Lobby-User-Id')) return new Response('Unauthorized', { status: 401 })
    const pair = new WebSocketPair()
    const [client, server] = Object.values(pair)
    this.ctx.acceptWebSocket(server)
    server.serializeAttachment({ userId: request.headers.get('X-Lobby-User-Id') })
    server.send(JSON.stringify({ type: 'lobby_state', state: this.requireLobby() }))
    return new Response(null, { status: 101, webSocket: client })
  }

  async webSocketMessage(socket: WebSocket) {
    socket.send(JSON.stringify({ type: 'lobby_state', state: this.requireLobby() }))
  }
}
