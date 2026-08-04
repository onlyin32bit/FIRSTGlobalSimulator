<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { ApiError, api, type LobbySlotId, type MatchLobby, type Robot } from '$lib/api';
	import { Button } from '$lib/components/ui/button';

	const matchId = $derived(page.params.matchId ?? '');
	let lobby = $state<MatchLobby | null>(null);
	let robots = $state<Robot[]>([]);
	let userId = $state('');
	let selectedRobotId = $state('');
	let error = $state('');
	let working = $state<null | string>(null);
	let socket: WebSocket | undefined;

	const occupied = $derived(lobby?.slots.filter((slot) => slot.occupant).length ?? 0);
	const ready = $derived(lobby?.slots.filter((slot) => slot.occupant?.ready).length ?? 0);
	const mySlot = $derived(lobby?.slots.find((slot) => slot.occupant?.userId === userId));
	const canStart = $derived(lobby?.hostId === userId && occupied === 8 && ready === 8 && lobby.status === 'LOBBY');

	function accept(next: MatchLobby) {
		lobby = next;
		error = next.error || '';
		if (next.status === 'IN_PROGRESS') void goto(`/match/${matchId}`);
	}

	async function claim(slotId: LobbySlotId) {
		const isDriver = slotId.includes('driver');
		if (isDriver && !selectedRobotId) {
			error = 'Choose one of your robots before taking a driver station.';
			return;
		}
		working = slotId;
		try {
			accept((await api.claimLobbySlot(matchId, { slotId, robotId: isDriver ? selectedRobotId : null })).lobby);
		} catch (cause) {
			error = cause instanceof ApiError ? cause.message : 'Unable to claim that station.';
		} finally { working = null; }
	}

	async function setReady() {
		if (!mySlot?.occupant) return;
		working = 'ready';
		try { accept((await api.setLobbyReady(matchId, !mySlot.occupant.ready)).lobby); }
		catch (cause) { error = cause instanceof ApiError ? cause.message : 'Unable to update ready state.'; }
		finally { working = null; }
	}

	async function leave() {
		working = 'leave';
		try { accept((await api.leaveLobby(matchId)).lobby); }
		catch (cause) { error = cause instanceof ApiError ? cause.message : 'Unable to leave the lobby.'; }
		finally { working = null; }
	}

	async function start() {
		working = 'start';
		try { accept((await api.startLobbyMatch(matchId)).lobby); }
		catch (cause) { error = cause instanceof ApiError ? cause.message : 'Unable to start the match.'; }
		finally { working = null; }
	}

	onMount(() => {
		let disposed = false;
		const refresh = async () => {
			try {
				const state = await api.getMatchLobby(matchId);
				if (!disposed) accept(state.lobby);
			} catch { /* The connected Durable Object socket will retry on the next page load. */ }
		};
		const load = async () => {
			try {
				const [state, currentUser, robotList] = await Promise.all([api.getMatchLobby(matchId), api.getCurrentUser(), api.listRobots()]);
				if (disposed) return;
				userId = currentUser.user.id;
				robots = robotList.robots;
				selectedRobotId = robots[0]?.id || '';
				accept(state.lobby);
				socket = new WebSocket(api.lobbyWebSocketUrl(matchId));
				socket.onmessage = (event) => {
					try {
						const message = JSON.parse(event.data);
						if (message.type === 'lobby_state') accept(message.state as MatchLobby);
					} catch { /* ignore malformed lobby broadcast */ }
				};
			} catch (cause) {
				error = cause instanceof ApiError ? cause.message : 'Unable to load lobby.';
			}
		};
		void load();
		const poller = window.setInterval(refresh, 5_000);
		return () => { disposed = true; window.clearInterval(poller); socket?.close(); };
	});
</script>

<main class="mx-auto max-w-6xl px-4 py-8 sm:px-6">
	<div class="flex flex-wrap items-start justify-between gap-4">
		<div>
			<p class="text-sm font-medium text-primary">FGC 2026 · Pre-match lobby</p>
			<h1 class="mt-1 text-3xl font-bold tracking-tight">Choose your alliance station</h1>
			<p class="mt-2 text-sm text-muted-foreground">{ready}/8 ready · {occupied}/8 stations filled · Match {matchId}</p>
		</div>
		{#if mySlot?.role === 'driver'}
			<label class="grid gap-1 text-sm font-medium">Robot
				<select class="h-9 rounded-md border border-input bg-background px-3" bind:value={selectedRobotId} disabled={lobby?.status !== 'LOBBY'}>
					{#if robots.length === 0}<option value="">No saved robot</option>{/if}
					{#each robots as robot}<option value={robot.id}>{robot.name}</option>{/each}
				</select>
			</label>
		{/if}
	</div>

	{#if error}<p class="mt-5 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>{/if}
	{#if !lobby}
		<p class="mt-10 text-muted-foreground">Loading lobby…</p>
	{:else}
		<div class="mt-8 grid gap-5 lg:grid-cols-2">
			{#each ['red', 'blue'] as alliance}
				<section class={`rounded-xl border p-5 ${alliance === 'red' ? 'border-rose-500/40 bg-rose-500/5' : 'border-sky-500/40 bg-sky-500/5'}`}>
					<h2 class="text-lg font-semibold capitalize">{alliance} alliance</h2>
					<div class="mt-4 grid gap-3">
						{#each lobby.slots.filter((slot) => slot.alliance === alliance) as station}
							<div class="flex min-h-20 items-center justify-between gap-3 rounded-lg border bg-background/80 p-3">
								<div>
									<p class="font-medium">{station.label}</p>
									{#if station.occupant}
										<p class="text-sm text-muted-foreground">{station.occupant.name}{station.occupant.robotId ? ` · ${robots.find((robot) => robot.id === station.occupant?.robotId)?.name || 'Robot selected'}` : ''}</p>
										<p class={`text-xs font-medium ${station.occupant.ready ? 'text-emerald-600' : 'text-amber-600'}`}>{station.occupant.ready ? 'Ready' : 'Not ready'}</p>
									{:else}<p class="text-sm text-muted-foreground">Open</p>{/if}
								</div>
								{#if !station.occupant || station.occupant.userId === userId}
									<Button size="sm" variant={station.occupant?.userId === userId ? 'outline' : 'default'} disabled={lobby.status !== 'LOBBY' || working !== null} onclick={() => claim(station.id)}>
										{station.occupant?.userId === userId ? 'Your station' : working === station.id ? 'Joining…' : 'Join'}
									</Button>
								{/if}
							</div>
						{/each}
					</div>
				</section>
			{/each}
		</div>

		<div class="mt-6 flex flex-wrap items-center gap-3 rounded-xl border bg-card p-4">
			{#if mySlot}
				<Button onclick={setReady} disabled={lobby.status !== 'LOBBY' || working !== null}>{mySlot.occupant?.ready ? 'Mark not ready' : working === 'ready' ? 'Updating…' : 'Ready'}</Button>
				<Button variant="outline" onclick={leave} disabled={lobby.status !== 'LOBBY' || working !== null}>{working === 'leave' ? 'Leaving…' : 'Leave station'}</Button>
			{:else}<p class="text-sm text-muted-foreground">Select an open station to join this match.</p>{/if}
			{#if lobby.hostId === userId}
				<Button class="ml-auto" disabled={!canStart || working !== null} onclick={start}>{working === 'start' ? 'Starting…' : 'Start match'}</Button>
			{/if}
		</div>
	{/if}
</main>
