import { api, ApiError } from '$lib/api';

export type DashboardDialog = 'create' | 'join' | null;

export class DashboardController {
	dialog = $state<DashboardDialog>(null);
	matchId = $state('');
	isCreating = $state(false);
	error = $state('');

	open(dialog: Exclude<DashboardDialog, null>) {
		this.error = '';
		this.dialog = dialog;
	}

	close() {
		if (!this.isCreating) this.dialog = null;
	}

	joinPath() {
		const matchId = this.matchId.trim();
		if (!matchId) {
			this.error = 'Enter a match ID.';
			return null;
		}
		return matchId === 'test-match'
			? '/match/test-match'
			: `/match/${encodeURIComponent(matchId)}/lobby`;
	}

	async createMatch() {
		this.isCreating = true;
		this.error = '';
		try {
			const { match_id } = await api.createMatch({ gamePackId: 'fgc-2026' });
			return `/match/${match_id}/lobby`;
		} catch (error) {
			this.error =
				error instanceof ApiError ? error.message : 'Could not create a match. Try again.';
			return null;
		} finally {
			this.isCreating = false;
		}
	}
}
