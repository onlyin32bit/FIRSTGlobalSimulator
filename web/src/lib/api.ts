export type UserRole = 'user' | 'admin';

export type ApiUser = {
	id: string;
	name: string;
	email: string;
	team: string | null;
	role: UserRole;
};

export type Robot = {
	id: string;
	name: string;
	buildData: Record<string, unknown>;
	createdAt: string | Date;
	updatedAt: string | Date;
};

export type Invitation = {
	code: string;
	createdAt: string | Date;
	expiresAt: string | Date | null;
	revokedAt: string | Date | null;
	redeemedAt: string | Date | null;
	redeemedByUserId: string | null;
	status: 'active' | 'redeemed' | 'revoked' | 'expired';
};

export type AdminUser = ApiUser & {
	disabledAt: string | Date | null;
	disabledReason: string | null;
	createdAt: string | Date;
	updatedAt: string | Date;
	sessionCount: number;
	robotCount: number;
	matchHostCount: number;
};

export type AdminMatch = {
	id: string;
	status: 'LOBBY' | 'IN_PROGRESS' | 'FINISHED' | 'CANCELLED';
	gamePackId: string;
	maxPlayers: number;
	updatedAt?: string | Date;
	cancelledAt?: string | Date | null;
	cancelReason?: string | null;
	createdAt: string | Date;
	hostId: string;
	hostName: string;
	hostEmail: string;
};

export type AdminAuditEntry = {
	id: string;
	action: string;
	targetType: string;
	targetId: string;
	metadata: Record<string, unknown> | null;
	createdAt: string | Date;
	actorName: string;
	actorEmail: string;
};

export type GameServer = {
	id: string;
	name: string;
	origin: string;
	maxUsers: number;
	maxMatches: number;
	slots: number;
	activeUsers: number;
	activeMatches: number;
	status: string;
	lastHeartbeatAt: string | Date | null;
	createdAt: string | Date;
	updatedAt: string | Date;
	disabledAt: string | Date | null;
	health: 'online' | 'offline' | 'disabled';
};

export type LobbySlotId =
	| 'red-driver-1' | 'red-driver-2' | 'red-driver-3' | 'red-human'
	| 'blue-driver-1' | 'blue-driver-2' | 'blue-driver-3' | 'blue-human';

export type MatchLobby = {
	matchId: string;
	hostId: string;
	status: 'LOBBY' | 'STARTING' | 'IN_PROGRESS' | 'FINISHED' | 'CANCELLED';
	error: string | null;
	updatedAt: number;
	slots: Array<{
		id: LobbySlotId;
		alliance: 'red' | 'blue';
		role: 'driver' | 'human-player';
		label: string;
		occupant: { userId: string; name: string; teamName: string | null; robotId: string | null; ready: boolean } | null;
	}>;
};

type Paginated<T> = {
	items: T[];
	page: number;
	pageSize: number;
	total: number;
	pageCount: number;
};

type ApiSuccess<T> = { success: true; data: T };
type ApiFailure = { success: false; error?: { code?: string; message?: string } };
type ApiResponse<T> = ApiSuccess<T> | ApiFailure;

export class ApiError extends Error {
	constructor(
		message: string,
		public readonly status: number,
		public readonly code?: string
	) {
		super(message);
		this.name = 'ApiError';
	}
}

export class APIClient {
	readonly baseUrl: string;

	constructor(baseUrl = '') {
		this.baseUrl = baseUrl.replace(/\/$/, '');
	}

	get authBaseUrl() {
		return `${this.baseUrl}/api/auth`;
	}

	private async request<T>(path: string, init: RequestInit = {}): Promise<T> {
		let response: Response;
		try {
			response = await fetch(`${this.baseUrl}${path}`, {
				...init,
				credentials: 'include',
				headers: {
					Accept: 'application/json',
					...(init.body ? { 'Content-Type': 'application/json' } : {}),
					...init.headers
				}
			});
		} catch {
			throw new ApiError(
				'Unable to reach the API. Check your connection and try again.',
				0,
				'NETWORK_ERROR'
			);
		}

		const body = (await response.json().catch(() => null)) as ApiResponse<T> | null;
		if (!response.ok || !body || body.success === false) {
			const error = body && body.success === false ? body.error : undefined;
			throw new ApiError(
				error?.message || `Request failed (${response.status})`,
				response.status,
				error?.code
			);
		}
		return body.data;
	}

	requestInvite(input: { email: string; name: string; team: string; message?: string }) {
		return this.request<{ message: string }>('/api/auth/request-invite', {
			method: 'POST',
			body: JSON.stringify(input)
		});
	}

	getCurrentUser() {
		return this.request<{ user: ApiUser }>('/api/user/me');
	}

	updateProfile(input: { name: string }) {
		return this.request<{ message: string }>('/api/user/profile', {
			method: 'PATCH',
			body: JSON.stringify(input)
		});
	}

	listRobots() {
		return this.request<{ robots: Robot[] }>('/api/robot');
	}

	createRobot(input: { name: string; buildData: Record<string, unknown> }) {
		return this.request<{ robot: Robot }>('/api/robot', {
			method: 'POST',
			body: JSON.stringify(input)
		});
	}

	createMatch(input: { gamePackId: 'fgc-2026' }) {
		return this.request<{ match_id: string; invite_link: string }>('/api/matches', {
			method: 'POST',
			body: JSON.stringify(input)
		});
	}

	createMatchTicket(matchId: string) {
		return this.request<{ ticket: string; ws_url: string }>(
			`/api/matches/${encodeURIComponent(matchId)}/ticket`,
			{ method: 'POST' }
		);
	}

	getMatchLobby(matchId: string) {
		return this.request<{ lobby: MatchLobby }>(`/api/matches/${encodeURIComponent(matchId)}/lobby`);
	}

	claimLobbySlot(matchId: string, input: { slotId: LobbySlotId; robotId?: string | null }) {
		return this.request<{ lobby: MatchLobby }>(`/api/matches/${encodeURIComponent(matchId)}/lobby/slot`, {
			method: 'POST', body: JSON.stringify(input)
		});
	}

	leaveLobby(matchId: string) {
		return this.request<{ lobby: MatchLobby }>(`/api/matches/${encodeURIComponent(matchId)}/lobby/leave`, { method: 'POST' });
	}

	setLobbyReady(matchId: string, ready: boolean) {
		return this.request<{ lobby: MatchLobby }>(`/api/matches/${encodeURIComponent(matchId)}/lobby/ready`, {
			method: 'POST', body: JSON.stringify({ ready })
		});
	}

	startLobbyMatch(matchId: string) {
		return this.request<{ lobby: MatchLobby; game_server_id: string }>(`/api/matches/${encodeURIComponent(matchId)}/lobby/start`, { method: 'POST' });
	}

	lobbyWebSocketUrl(matchId: string) {
		const apiOrigin = this.baseUrl || window.location.origin;
		return `${apiOrigin.replace(/^http:/, 'ws:').replace(/^https:/, 'wss:')}/api/matches/${encodeURIComponent(matchId)}/lobby/ws`;
	}

	createTestMatchTicket() {
		return this.request<{ match_id: 'test-match'; ticket: string; ws_url: string }>(
			'/api/matches/test-match/ticket',
			{ method: 'POST' }
		);
	}

	listGamePacks() {
		return this.request<{ packs: Array<{ id: 'fgc-2026'; name: string; version: string }> }>(
			'/api/game-packs'
		);
	}

	getGamePackMetadata(id: string) {
		return this.request<{
			manifest: {
				id: string;
				name: string;
				version: string;
				engineVersion: string;
				field: Record<string, unknown>;
				objects: Array<Record<string, unknown>>;
				phases: Array<Record<string, unknown>>;
				scripts: Record<string, string>;
			};
			scripts: Array<{
				path: string;
				functions: Array<{ name: string; parameters: string[] }>;
				engineCalls: string[];
			}>;
		}>(`/api/game-packs/${encodeURIComponent(id)}/metadata`);
	}

	getGamePackAssets(id: 'fgc-2026') {
		return this.request<{ visual: string; physics: string; semantics: string }>(
			`/api/game-packs/${encodeURIComponent(id)}/assets`
		);
	}

	getAdminOverview() {
		return this.request<{
			metrics: {
				users: number;
				admins: number;
				robots: number;
				matches: number;
				activeInvitations: number;
			};
			recentActivity: AdminAuditEntry[];
			recentMatches: Omit<AdminMatch, 'hostId'>[];
		}>('/api/admin/overview');
	}

	listUsers(input: { search?: string; page?: number; pageSize?: number } = {}) {
		const query = new URLSearchParams();
		if (input.search) query.set('search', input.search);
		if (input.page) query.set('page', String(input.page));
		if (input.pageSize) query.set('pageSize', String(input.pageSize));
		return this.request<{ users: Paginated<AdminUser> }>(`/api/admin/users?${query}`);
	}

	updateUser(userId: string, input: { name: string; team: string; role: UserRole }) {
		return this.request<{ user: ApiUser }>(`/api/admin/users/${encodeURIComponent(userId)}`, {
			method: 'PATCH',
			body: JSON.stringify(input)
		});
	}

	disableUser(userId: string, reason: string) {
		return this.request<{ disabledAt: string | Date; sessionsRevoked: number }>(
			`/api/admin/users/${encodeURIComponent(userId)}/disable`,
			{
				method: 'POST',
				body: JSON.stringify({ reason })
			}
		);
	}

	enableUser(userId: string) {
		return this.request<{ message: string }>(
			`/api/admin/users/${encodeURIComponent(userId)}/enable`,
			{ method: 'POST' }
		);
	}

	revokeUserSessions(userId: string) {
		return this.request<{ sessionsRevoked: number }>(
			`/api/admin/users/${encodeURIComponent(userId)}/revoke-sessions`,
			{
				method: 'POST'
			}
		);
	}

	listInvitations() {
		return this.request<{ invitations: Invitation[] }>('/api/admin/invitations');
	}

	createInvitation(input: { expiresAt?: Date }) {
		return this.request<{ invitation: Invitation }>('/api/admin/invitations', {
			method: 'POST',
			body: JSON.stringify(input)
		});
	}

	updateInvitation(code: string, input: { expiresAt: Date | null }) {
		return this.request<{ invitation: Invitation }>(
			`/api/admin/invitations/${encodeURIComponent(code)}`,
			{
				method: 'PATCH',
				body: JSON.stringify(input)
			}
		);
	}

	revokeInvitation(code: string) {
		return this.request<{ invitation: Invitation }>(
			`/api/admin/invitations/${encodeURIComponent(code)}/revoke`,
			{ method: 'POST' }
		);
	}

	createAdminMatch(input: {
		hostId: string;
		gamePackId: 'fgc-2026';
		maxPlayers: number;
		status: AdminMatch['status'];
	}) {
		return this.request<{ match: AdminMatch }>('/api/admin/matches', {
			method: 'POST',
			body: JSON.stringify(input)
		});
	}

	updateAdminMatch(
		id: string,
		input: {
			hostId: string;
			gamePackId: 'fgc-2026';
			maxPlayers: number;
			status: AdminMatch['status'];
			cancelReason?: string;
		}
	) {
		return this.request<{ match: AdminMatch }>(`/api/admin/matches/${encodeURIComponent(id)}`, {
			method: 'PATCH',
			body: JSON.stringify(input)
		});
	}

	cancelAdminMatch(id: string, reason: string) {
		return this.request<{ match: AdminMatch }>(
			`/api/admin/matches/${encodeURIComponent(id)}/cancel`,
			{ method: 'POST', body: JSON.stringify({ reason }) }
		);
	}

	listAdminMatches(input: { page?: number; pageSize?: number } = {}) {
		const query = new URLSearchParams();
		if (input.page) query.set('page', String(input.page));
		if (input.pageSize) query.set('pageSize', String(input.pageSize));
		return this.request<{ matches: Paginated<AdminMatch> }>(`/api/admin/matches?${query}`);
	}

	listAdminGamePacks() {
		return this.request<{
			packs: Array<{
				id: string;
				name: string;
				version: string;
				status: string;
				engineCompatibility: string;
			}>;
		}>('/api/admin/game-packs');
	}

	listAdminAuditLog(input: { page?: number; pageSize?: number } = {}) {
		const query = new URLSearchParams();
		if (input.page) query.set('page', String(input.page));
		if (input.pageSize) query.set('pageSize', String(input.pageSize));
		return this.request<{ auditLog: Paginated<AdminAuditEntry> }>(`/api/admin/audit-log?${query}`);
	}

	listGameServers() {
		return this.request<{ servers: GameServer[] }>('/api/admin/game-servers');
	}

	createGameServer(input: {
		name: string;
		origin: string;
		maxUsers: number;
		maxMatches: number;
		slots: number;
	}) {
		return this.request<{ server: GameServer; key: string }>('/api/admin/game-servers', {
			method: 'POST',
			body: JSON.stringify(input)
		});
	}

	updateGameServer(
		id: string,
		input: Partial<
			Pick<GameServer, 'name' | 'origin' | 'maxUsers' | 'maxMatches' | 'slots' | 'status'>
		>
	) {
		return this.request<{ server: GameServer }>(
			`/api/admin/game-servers/${encodeURIComponent(id)}`,
			{
				method: 'PATCH',
				body: JSON.stringify(input)
			}
		);
	}
}

export const api = new APIClient();
