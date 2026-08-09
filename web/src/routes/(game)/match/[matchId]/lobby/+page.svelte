<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { ApiError, api, type LobbySlotId, type MatchLobby, type Robot } from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { IconCopy, IconCheck } from '@tabler/icons-svelte';

	const matchId = $derived(page.params.matchId ?? '');
	let lobby = $state<MatchLobby | null>(null);
	let robots = $state<Robot[]>([]);
	let userId = $state('');
	let isAdmin = $state(false);
	let selectedRobotId = $state('');
	let error = $state('');
	let working = $state<null | string>(null);
	let socket: WebSocket | undefined;

	let restrictedAlliance = $derived(page.url.searchParams.get('alliance')); // 'red' or 'blue' or null

	const redOccupied = $derived(lobby?.slots.filter((s) => s.alliance === 'red' && s.occupant).length ?? 0);
	const blueOccupied = $derived(lobby?.slots.filter((s) => s.alliance === 'blue' && s.occupant).length ?? 0);
	const allReady = $derived(lobby?.slots.filter((s) => s.occupant).every((s) => s.occupant?.ready) ?? false);
	const mySlot = $derived(lobby?.slots.find((slot) => slot.occupant?.userId === userId));
	const isHost = $derived(lobby?.hostId === userId);
	
	// Start requirement: at least 1 per alliance, and all joined are ready
	const canStart = $derived(isHost && redOccupied >= 1 && blueOccupied >= 1 && allReady && lobby?.status === 'LOBBY');

	function accept(next: MatchLobby) {
		lobby = next;
		error = next.error || '';
		if (next.status === 'IN_PROGRESS') void goto(resolve(`/match/${matchId}`));
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

	async function adminStart() {
		working = 'admin-start';
		try { accept((await api.adminStartLobbyMatch(matchId)).lobby); }
		catch (cause) { error = cause instanceof ApiError ? cause.message : 'Unable to enter the match immediately.'; }
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
				isAdmin = currentUser.user.role === 'admin';
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

	let copiedLink = $state<string | null>(null);
	function copyLink(type: 'any' | 'red' | 'blue') {
		const url = new URL(window.location.href);
		url.searchParams.delete('alliance');
		if (type !== 'any') {
			url.searchParams.set('alliance', type);
		}
		navigator.clipboard.writeText(url.toString());
		copiedLink = type;
		setTimeout(() => { copiedLink = null; }, 2000);
	}
</script>

<main class="mx-auto max-w-7xl px-4 py-8 sm:px-6">
	<div class="flex flex-col gap-6 lg:flex-row lg:items-start lg:justify-between">
		<div>
			<p class="text-sm font-medium text-primary uppercase tracking-widest font-mono">Pre-match lobby</p>
			<h1 class="mt-1 font-daybreaker text-4xl tracking-wide">FGC 2026 COMMAND</h1>
			<p class="mt-2 text-sm text-muted-foreground">Match ID: <code class="bg-card px-1 py-0.5 rounded border border-border">{matchId}</code></p>
		</div>

		<div class="flex flex-col gap-4 bg-card/60 p-4 rounded-xl border border-border/80 backdrop-blur-md">
			{#if isHost}
				<div>
					<p class="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">INVITE PLAYERS</p>
					<div class="flex flex-wrap gap-2">
						<Button size="sm" variant="outline" class="font-mono text-xs gap-1.5 border-primary/30 hover:bg-primary/10" onclick={() => copyLink('any')}>
							{#if copiedLink === 'any'}<IconCheck class="size-3.5 text-emerald-500" />{:else}<IconCopy class="size-3.5" />{/if} Any Alliance
						</Button>
						<Button size="sm" variant="outline" class="font-mono text-xs gap-1.5 border-rose-500/30 hover:bg-rose-500/10 text-rose-500" onclick={() => copyLink('red')}>
							{#if copiedLink === 'red'}<IconCheck class="size-3.5 text-emerald-500" />{:else}<IconCopy class="size-3.5" />{/if} Red Only
						</Button>
						<Button size="sm" variant="outline" class="font-mono text-xs gap-1.5 border-sky-500/30 hover:bg-sky-500/10 text-sky-500" onclick={() => copyLink('blue')}>
							{#if copiedLink === 'blue'}<IconCheck class="size-3.5 text-emerald-500" />{:else}<IconCopy class="size-3.5" />{/if} Blue Only
						</Button>
					</div>
				</div>
			{:else}
				<div class="text-sm font-mono text-muted-foreground">
					{#if restrictedAlliance}
						Waiting for match to start. You are restricted to the <strong class={restrictedAlliance === 'red' ? 'text-rose-500' : 'text-sky-500'}>{restrictedAlliance.toUpperCase()}</strong> alliance.
					{:else}
						Waiting for host to start match.
					{/if}
				</div>
			{/if}
		</div>
	</div>

	{#if error}<p class="mt-5 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>{/if}
	
	{#if !lobby}
		<div class="mt-10 flex flex-col items-center justify-center p-20 border border-border/50 rounded-2xl bg-card/30">
			<div class="size-8 rounded-full border-2 border-primary border-t-transparent animate-spin"></div>
			<p class="mt-4 text-muted-foreground font-mono text-sm">INITIALIZING LOBBY…</p>
		</div>
	{:else}
		<div class="mt-8 flex flex-col gap-6 xl:flex-row">
			<!-- Field Map Overlay -->
			<div class="relative w-full flex-1 overflow-hidden rounded-2xl border-2 border-border/60 bg-black shadow-2xl aspect-[1.8/1] lg:aspect-[2.2/1]">
				<!-- Background Map -->
				<img src="/field.png" alt="Field Map" class="absolute inset-0 w-full h-full object-cover opacity-60" />
				
				<!-- Grid overlay -->
				<div class="pointer-events-none absolute inset-0 bg-[linear-gradient(to_right,#ffffff08_1px,transparent_1px),linear-gradient(to_bottom,#ffffff08_1px,transparent_1px)] bg-[size:2rem_2rem]"></div>
				
				<div class="absolute inset-0 flex justify-between p-2 sm:p-6 lg:p-8">
					<!-- Red Alliance Stations -->
					<div class="flex flex-col justify-around h-full w-40 sm:w-56 gap-2">
						{#each lobby.slots.filter((slot) => slot.alliance === 'red') as station (station.id)}
							{@const isRestricted = restrictedAlliance && restrictedAlliance !== 'red'}
							<div class="group relative flex flex-col overflow-hidden rounded-lg border bg-background/85 p-2 backdrop-blur-md transition-all sm:p-3 {station.occupant ? (station.occupant.ready ? 'border-emerald-500/50 shadow-[0_0_15px_rgba(16,185,129,0.2)]' : 'border-rose-500/50') : 'border-rose-500/20 hover:border-rose-500/60'}">
								<p class="font-mono text-[10px] sm:text-xs font-bold text-rose-500 uppercase tracking-wider">{station.label}</p>
								
								{#if station.occupant}
									<p class="mt-1 truncate text-xs sm:text-sm font-semibold text-white">{station.occupant.name}</p>
									<p class="truncate text-[10px] text-muted-foreground">{station.occupant.robotId ? (robots.find(r => r.id === station.occupant?.robotId)?.name || 'Robot Selected') : 'No Robot'}</p>
									<div class="absolute right-2 top-2">
										<div class="size-2 rounded-full {station.occupant.ready ? 'bg-emerald-500 shadow-[0_0_5px_#10b981]' : 'bg-amber-500 animate-pulse'}"></div>
									</div>
								{:else}
									<p class="mt-1 text-[10px] sm:text-xs text-muted-foreground uppercase">Awaiting Driver</p>
								{/if}

								{#if (!station.occupant || station.occupant.userId === userId) && !isRestricted}
									<div class="mt-2">
										<Button 
											size="sm" 
											variant={station.occupant?.userId === userId ? 'outline' : 'secondary'} 
											class="h-6 sm:h-7 w-full text-[10px] font-mono hover:bg-rose-500 hover:text-white"
											disabled={lobby.status !== 'LOBBY' || working !== null} 
											onclick={() => claim(station.id)}
										>
											{station.occupant?.userId === userId ? 'SELECTED' : working === station.id ? '...' : 'CLAIM'}
										</Button>
									</div>
								{/if}
							</div>
						{/each}
					</div>

					<!-- Blue Alliance Stations -->
					<div class="flex flex-col justify-around h-full w-40 sm:w-56 gap-2 text-right">
						{#each lobby.slots.filter((slot) => slot.alliance === 'blue') as station (station.id)}
							{@const isRestricted = restrictedAlliance && restrictedAlliance !== 'blue'}
							<div class="group relative flex flex-col items-end overflow-hidden rounded-lg border bg-background/85 p-2 backdrop-blur-md transition-all sm:p-3 {station.occupant ? (station.occupant.ready ? 'border-emerald-500/50 shadow-[0_0_15px_rgba(16,185,129,0.2)]' : 'border-sky-500/50') : 'border-sky-500/20 hover:border-sky-500/60'}">
								<p class="font-mono text-[10px] sm:text-xs font-bold text-sky-500 uppercase tracking-wider">{station.label}</p>
								
								{#if station.occupant}
									<p class="mt-1 truncate text-xs sm:text-sm font-semibold text-white">{station.occupant.name}</p>
									<p class="truncate text-[10px] text-muted-foreground">{station.occupant.robotId ? (robots.find(r => r.id === station.occupant?.robotId)?.name || 'Robot Selected') : 'No Robot'}</p>
									<div class="absolute left-2 top-2">
										<div class="size-2 rounded-full {station.occupant.ready ? 'bg-emerald-500 shadow-[0_0_5px_#10b981]' : 'bg-amber-500 animate-pulse'}"></div>
									</div>
								{:else}
									<p class="mt-1 text-[10px] sm:text-xs text-muted-foreground uppercase">Awaiting Driver</p>
								{/if}

								{#if (!station.occupant || station.occupant.userId === userId) && !isRestricted}
									<div class="mt-2 w-full">
										<Button 
											size="sm" 
											variant={station.occupant?.userId === userId ? 'outline' : 'secondary'} 
											class="h-6 sm:h-7 w-full text-[10px] font-mono hover:bg-sky-500 hover:text-white"
											disabled={lobby.status !== 'LOBBY' || working !== null} 
											onclick={() => claim(station.id)}
										>
											{station.occupant?.userId === userId ? 'SELECTED' : working === station.id ? '...' : 'CLAIM'}
										</Button>
									</div>
								{/if}
							</div>
						{/each}
					</div>
				</div>
			</div>

			<!-- Control Panel -->
			<div class="flex w-full flex-col gap-6 xl:w-80">
				<!-- Setup Section -->
				<div class="rounded-2xl border border-border/80 bg-card/60 p-6 shadow-lg backdrop-blur-xl">
					<h3 class="font-daybreaker text-xl tracking-wide mb-4">DEPLOYMENT SETUP</h3>
					
					<div class="space-y-4">
						<div class="flex flex-col gap-2">
							<label class="font-mono text-xs text-muted-foreground uppercase tracking-widest">Selected Robot</label>
							<select class="h-11 rounded-md border border-input bg-background px-3 font-mono text-sm shadow-inner" bind:value={selectedRobotId} disabled={lobby?.status !== 'LOBBY'}>
								{#if robots.length === 0}<option value="">No saved robot</option>{/if}
								{#each robots as robot (robot.id)}<option value={robot.id}>{robot.name}</option>{/each}
							</select>
							<p class="text-[10px] text-muted-foreground mt-1">Select before claiming a driver station.</p>
						</div>

						<div class="pt-4 border-t border-border/40">
							{#if mySlot}
								<Button 
									onclick={setReady} 
									disabled={lobby.status !== 'LOBBY' || working !== null}
									class="w-full h-12 font-mono font-bold text-sm uppercase tracking-widest shadow-[0_0_15px_rgba(234,88,12,0.2)] {mySlot.occupant?.ready ? 'bg-emerald-600 hover:bg-emerald-500' : 'bg-primary hover:bg-primary/90'}"
								>
									{mySlot.occupant?.ready ? 'READY FOR DEPLOY' : working === 'ready' ? 'UPDATING...' : 'LOCK IN & READY'}
								</Button>
								<Button variant="ghost" class="mt-2 w-full font-mono text-xs hover:bg-destructive/10 hover:text-destructive" onclick={leave} disabled={lobby.status !== 'LOBBY' || working !== null}>
									{working === 'leave' ? 'LEAVING...' : 'VACATE STATION'}
								</Button>
							{:else}
								<div class="rounded border border-primary/20 bg-primary/5 p-4 text-center font-mono text-xs text-primary/80">
									Claim a station on the field map to deploy into this match.
								</div>
							{/if}
						</div>
					</div>
				</div>

				<!-- Match Status -->
				<div class="rounded-2xl border border-border/80 bg-card/60 p-6 shadow-lg backdrop-blur-xl">
					<div class="mb-4 flex items-center justify-between">
						<h3 class="font-daybreaker text-xl tracking-wide">STATUS</h3>
						<span class="rounded bg-background px-2 py-1 font-mono text-[10px] uppercase text-primary border border-primary/20">{lobby.status}</span>
					</div>
					
					<div class="font-mono text-xs space-y-2 mb-6">
						<div class="flex justify-between">
							<span class="text-muted-foreground">Red Alliance:</span>
							<span class={redOccupied > 0 ? 'text-rose-500 font-bold' : ''}>{redOccupied} / 4</span>
						</div>
						<div class="flex justify-between">
							<span class="text-muted-foreground">Blue Alliance:</span>
							<span class={blueOccupied > 0 ? 'text-sky-500 font-bold' : ''}>{blueOccupied} / 4</span>
						</div>
						<div class="flex justify-between">
							<span class="text-muted-foreground">Players Ready:</span>
							<span class={allReady && (redOccupied + blueOccupied) > 0 ? 'text-emerald-500 font-bold' : ''}>
								{lobby.slots.filter(s => s.occupant?.ready).length} / {redOccupied + blueOccupied}
							</span>
						</div>
					</div>

					{#if isHost}
						<Button 
							class="w-full h-12 font-mono font-bold text-sm uppercase tracking-widest shadow-[0_0_15px_rgba(234,88,12,0.2)] disabled:opacity-50"
							disabled={!canStart || working !== null} 
							onclick={start}
						>
							{working === 'start' ? 'INITIALIZING...' : 'START MATCH'}
						</Button>
						{#if !canStart && lobby.status === 'LOBBY'}
							<p class="mt-2 text-center text-[10px] text-muted-foreground leading-tight">
								Requires at least 1 player per alliance and all joined players must be Ready.
							</p>
						{/if}
					{/if}

					{#if isAdmin && lobby.status === 'LOBBY'}
						<div class="mt-4 pt-4 border-t border-border/40">
							<div class="flex items-center gap-2">
								<span class="text-[10px] text-amber-500 leading-tight flex-1">Admin bypass: Force start</span>
								<Button size="sm" variant="outline" class="font-mono text-[10px]" disabled={working !== null} onclick={adminStart}>FORCE START</Button>
							</div>
						</div>
					{/if}
				</div>
			</div>
		</div>
	{/if}
</main>

