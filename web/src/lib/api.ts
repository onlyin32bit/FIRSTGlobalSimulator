import { browser } from '$app/environment';
import { env } from '$env/dynamic/public';

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
	buildData: string;
	createdAt: Date;
	updatedAt: Date;
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

/** The single frontend entry point for simulator API requests. */
export class APIClient {
	readonly baseUrl: string;

	constructor(
		baseUrl = env.PUBLIC_API_URL || (browser ? window.location.origin : 'http://localhost:5173')
	) {
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
			method: 'POST',
			body: JSON.stringify(input)
		});
	}

	listRobots() {
		return this.request<{ robots: Robot[] }>('/api/robot');
	}

	createRobot(input: { name: string; buildData: Record<string, unknown> }) {
		return this.request<{ robot_id: string }>('/api/robot', {
			method: 'POST',
			body: JSON.stringify(input)
		});
	}

	createMatch(input: { gamePackId: string }) {
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

	listGamePacks() {
		return this.request<{ packs: Array<{ id: string; name: string; version: string }> }>(
			'/api/game-packs'
		);
	}

	listUsers() {
		return this.request<{ users: ApiUser[] }>('/api/admin/users');
	}
}

export const api = new APIClient();
