import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import { api, ApiError, type LobbySlotId, type MatchLobby, type Robot } from '$lib/api';

export type LobbyField = {
	imageUrl: string;
	layout: NonNullable<Awaited<ReturnType<typeof api.getGamePackMetadata>>['manifest']['lobby']>;
	anchors: Record<string, [number, number, number]>;
	semanticAreas: Awaited<
		ReturnType<typeof api.getGamePackMetadata>
	>['fieldDefinition']['semanticAreas'];
};

export class LobbyController {
	lobby = $state<MatchLobby | null>(null);
	robots = $state<Robot[]>([]);
	userId = $state('');
	isAdmin = $state(false);
	selectedRobotId = $state('');
	working = $state<string | null>(null);
	error = $state('');
	field = $state<LobbyField | null>(null);
	private socket: WebSocket | null = null;

	get mySlot() {
		return this.lobby?.slots.find((slot) => slot.occupant?.userId === this.userId);
	}

	get isHost() {
		return this.lobby?.hostId === this.userId;
	}

	get canStart() {
		if (!this.lobby || !this.isHost || this.lobby.status !== 'LOBBY') return false;
		const occupied = this.lobby.slots.filter((slot) => slot.occupant);
		return (
			['red', 'blue'].every((alliance) => occupied.some((slot) => slot.alliance === alliance)) &&
			occupied.every((slot) => slot.occupant?.ready)
		);
	}

	accept(lobby: MatchLobby) {
		this.lobby = lobby;
		this.error = lobby.error ?? '';
		if (lobby.status === 'IN_PROGRESS') void goto(resolve(`/match/${lobby.matchId}`));
	}

	async load(matchId: string) {
		try {
			const [state, currentUser, robots, assets, metadata] = await Promise.all([
				api.getMatchLobby(matchId),
				api.getCurrentUser(),
				api.listRobots(),
				api.getGamePackAssets('fgc-2026'),
				api.getGamePackMetadata('fgc-2026')
			]);
			this.userId = currentUser.user.id;
			this.isAdmin = currentUser.user.role === 'admin';
			this.robots = robots.robots;
			this.selectedRobotId = robots.robots[0]?.id ?? '';
			if (assets.ui?.lobbyField && metadata.manifest.lobby) {
				this.field = {
					imageUrl: assets.ui.lobbyField,
					layout: metadata.manifest.lobby,
					anchors: metadata.fieldDefinition.anchors,
					semanticAreas: metadata.fieldDefinition.semanticAreas
				};
			}
			this.accept(state.lobby);
			this.connect(matchId);
		} catch (error) {
			this.error = error instanceof ApiError ? error.message : 'Could not load this lobby.';
		}
	}

	private connect(matchId: string) {
		this.socket?.close();
		this.socket = new WebSocket(api.lobbyWebSocketUrl(matchId));
		this.socket.onmessage = (event) => {
			try {
				const message = JSON.parse(event.data);
				if (message.type === 'lobby_state') this.accept(message.state as MatchLobby);
			} catch {}
		};
	}

	async refresh(matchId: string) {
		try {
			this.accept((await api.getMatchLobby(matchId)).lobby);
		} catch {}
	}

	async claim(matchId: string, slotId: LobbySlotId) {
		const driver = slotId.includes('driver');
		if (driver && !this.selectedRobotId) {
			this.error = 'Choose a robot before taking a driver station.';
			return;
		}
		await this.run(slotId, async () =>
			this.accept(
				(
					await api.claimLobbySlot(matchId, {
						slotId,
						robotId: driver ? this.selectedRobotId : null
					})
				).lobby
			)
		);
	}

	async setReady(matchId: string) {
		if (!this.mySlot?.occupant) return;
		await this.run('ready', async () =>
			this.accept((await api.setLobbyReady(matchId, !this.mySlot?.occupant?.ready)).lobby)
		);
	}

	async leave(matchId: string) {
		await this.run('leave', async () => this.accept((await api.leaveLobby(matchId)).lobby));
	}

	async start(matchId: string, admin = false) {
		await this.run(admin ? 'admin-start' : 'start', async () =>
			this.accept(
				(admin ? await api.adminStartLobbyMatch(matchId) : await api.startLobbyMatch(matchId)).lobby
			)
		);
	}

	private async run(action: string, task: () => Promise<void>) {
		this.working = action;
		this.error = '';
		try {
			await task();
		} catch (error) {
			this.error = error instanceof ApiError ? error.message : 'Could not update the lobby.';
		} finally {
			this.working = null;
		}
	}

	destroy() {
		this.socket?.close();
		this.socket = null;
	}
}
