<script lang="ts">
	import { onMount } from 'svelte';
	import { ApiError, api, type GameServer, type GameServerCommand, type GameServerInstance, type GameServerRuntimeMatch } from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';

	let servers = $state<GameServer[]>([]);
	let selected = $state<GameServer | null>(null);
	let instances = $state<GameServerInstance[]>([]);
	let matches = $state<GameServerRuntimeMatch[]>([]);
	let commands = $state<GameServerCommand[]>([]);
	let error = $state('');
	let createdKey = $state('');
	let submitting = $state(false);
	let managing = $state(false);
	let form = $state({ origin: 'https://game.example.com' });

	const bytes = (value?: number) => value ? `${(value / 1024 / 1024).toFixed(0)} MiB` : '—';
	const percent = (value?: number) => value === undefined ? '—' : `${value.toFixed(1)}%`;

	async function load() {
		try {
			servers = (await api.listGameServers()).servers;
			if (selected) await inspect(selected.id, false);
			error = '';
		} catch (e) { error = e instanceof ApiError ? e.message : 'Unable to load game servers.'; }
	}

	async function inspect(id: string, expand = true) {
		try {
			const result = await api.getGameServer(id);
			selected = result.server;
			instances = result.instances;
			matches = result.matches;
			commands = result.commands;
			if (expand) error = '';
		} catch (e) { error = e instanceof ApiError ? e.message : 'Unable to inspect this game server.'; }
	}

	async function create() {
		submitting = true; createdKey = '';
		try { const result = await api.createGameServer(form); createdKey = result.key; form = { origin: 'https://game.example.com' }; await load(); }
		catch (e) { error = e instanceof ApiError ? e.message : 'Unable to create game server.'; }
		finally { submitting = false; }
	}

	async function disable(server: GameServer) {
		try { await api.updateGameServer(server.id, { status: server.status === 'disabled' ? 'provisioning' : 'disabled' }); await load(); }
		catch (e) { error = e instanceof ApiError ? e.message : 'Unable to update game server.'; }
	}

	async function command(server: GameServer, type: 'kick_player' | 'stop_match' | 'clear_match' | 'cleanup_idle' | 'reset_host', matchId?: string) {
		let userId: string | undefined;
		if (type === 'kick_player') { userId = window.prompt('User ID to kick from this match:')?.trim(); if (!userId) return; }
		if (!confirm(`Queue ${type.replaceAll('_', ' ')} on ${server.name}?`)) return;
		managing = true;
		try { await api.commandGameServer(server.id, { type, matchId, userId }); await inspect(server.id); }
		catch (e) { error = e instanceof ApiError ? e.message : 'Unable to queue host command.'; }
		finally { managing = false; }
	}

	async function remove(server: GameServer) {
		if (!confirm(`Delete ${server.name}? Its key is revoked and assigned matches are cancelled.`)) return;
		try { await api.deleteGameServer(server.id); if (selected?.id === server.id) selected = null; await load(); }
		catch (e) { error = e instanceof ApiError ? e.message : 'Unable to delete game server.'; }
	}

	onMount(() => { void load(); const timer = window.setInterval(() => void load(), 5000); return () => window.clearInterval(timer); });
</script>

<div class="mx-auto max-w-[104rem] space-y-7">
	<div><h1 class="text-3xl font-semibold tracking-tight">Game servers</h1><p class="mt-2 text-sm text-muted-foreground">Live host inventory, capacity, machine discovery, telemetry, and match control.</p></div>
	{#if error}<p class="rounded-lg border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive">{error}</p>{/if}
	{#if createdKey}<div class="rounded-xl border border-amber-400/40 bg-amber-50 p-4 text-sm text-amber-950"><p class="font-semibold">Copy this server key now — it will not be shown again.</p><code class="mt-2 block overflow-x-auto rounded bg-white/70 p-3">{createdKey}</code><p class="mt-2 text-xs">Set <code>API_URL</code> and <code>GAME_SERVER_KEY</code> on the Rust host.</p></div>{/if}
	<section class="rounded-xl border border-border bg-card p-5"><h2 class="font-semibold">Create a host</h2><div class="mt-4 max-w-2xl"><Input placeholder="Public game-server URL (https://host)" bind:value={form.origin} /></div><Button class="mt-4" disabled={submitting || !form.origin} onclick={create}>{submitting ? 'Creating…' : 'Generate server key'}</Button></section>
	<section class="overflow-hidden rounded-xl border border-border bg-card"><div class="grid divide-y divide-border md:grid-cols-2 md:divide-y-0 xl:grid-cols-3">
		{#each servers as server}<article class="border-border p-5 md:border-r"><div class="flex items-start justify-between gap-3"><div><h2 class="font-semibold">{server.name}</h2><p class="mt-1 truncate text-xs text-muted-foreground">{server.origin}</p></div><span class="rounded-full px-2 py-1 text-[11px] {server.health === 'online' ? 'bg-emerald-100 text-emerald-800' : 'bg-muted text-muted-foreground'}">{server.health}</span></div>
			<div class="mt-5 grid grid-cols-3 gap-3 text-sm"><div><p class="text-xs text-muted-foreground">Users</p><p class="mt-1 font-semibold">{server.activeUsers} / {server.maxUsers}</p></div><div><p class="text-xs text-muted-foreground">Matches</p><p class="mt-1 font-semibold">{server.activeMatches} / {server.maxMatches}</p></div><div><p class="text-xs text-muted-foreground">Host CPU</p><p class="mt-1 font-semibold">{percent(server.runtime?.cpuPercent)}</p></div></div>
			<p class="mt-4 text-xs text-muted-foreground">{server.runtime?.platform === 'fly' ? `${server.runtime.appName ?? 'Fly'} · ${server.runtime.region ?? 'unknown region'} · ${server.runtime.machineId ?? 'machine pending'}` : server.lastHeartbeatAt ? `Heartbeat ${new Date(server.lastHeartbeatAt).toLocaleTimeString()}` : 'Waiting for heartbeat'}</p>
			<div class="mt-5 flex flex-wrap gap-2"><Button size="sm" variant="outline" onclick={() => inspect(server.id)}>Manage</Button><Button size="sm" variant="outline" onclick={() => disable(server)}>{server.status === 'disabled' ? 'Enable' : 'Disable'}</Button><Button size="sm" variant="destructive" onclick={() => remove(server)}>Delete</Button></div></article>{:else}<div class="p-8 text-sm text-muted-foreground">No game servers provisioned yet.</div>{/each}
	</div></section>
	{#if selected}<section class="rounded-xl border border-border bg-card p-5"><div class="flex flex-wrap items-start justify-between gap-4"><div><h2 class="text-lg font-semibold">Manage {selected.name}</h2><p class="text-sm text-muted-foreground">Commands are delivered on the host’s next authenticated heartbeat.</p></div><div class="flex gap-2"><Button size="sm" variant="outline" disabled={managing} onclick={() => command(selected!, 'cleanup_idle')}>Cleanup idle</Button><Button size="sm" variant="outline" disabled={managing} onclick={() => command(selected!, 'reset_host')}>Reset host</Button></div></div>
		<div class="mt-5 grid gap-3 text-sm sm:grid-cols-2 lg:grid-cols-4"><div class="rounded-lg bg-muted/50 p-3"><p class="text-xs text-muted-foreground">CPU / memory</p><p class="mt-1 font-medium">{percent(selected.runtime?.cpuPercent)} · {bytes(selected.runtime?.rssBytes)}</p></div><div class="rounded-lg bg-muted/50 p-3"><p class="text-xs text-muted-foreground">Host spec</p><p class="mt-1 font-medium">{selected.runtime?.cpuCores ?? '—'} cores · {bytes(selected.runtime?.memoryTotalBytes)}</p></div><div class="rounded-lg bg-muted/50 p-3"><p class="text-xs text-muted-foreground">Runtime</p><p class="mt-1 font-medium">{selected.runtime?.os ?? '—'} / {selected.runtime?.arch ?? '—'}</p></div><div class="rounded-lg bg-muted/50 p-3"><p class="text-xs text-muted-foreground">Uptime</p><p class="mt-1 font-medium">{selected.runtime?.uptimeSeconds ? `${Math.floor(selected.runtime.uptimeSeconds / 60)} min` : '—'}</p></div></div>
		<h3 class="mt-7 font-semibold">Running matches</h3><div class="mt-3 overflow-x-auto rounded-lg border border-border"><table class="w-full text-left text-sm"><thead class="bg-muted/50 text-xs text-muted-foreground"><tr><th class="p-3">Match</th><th class="p-3">Players</th><th class="p-3">TPS / load</th><th class="p-3">Physics</th><th class="p-3"></th></tr></thead><tbody>{#each matches as match}<tr class="border-t border-border"><td class="p-3 font-mono text-xs">{match.matchId}</td><td class="p-3">{match.players} · {match.objects} balls</td><td class="p-3">{match.tps.toFixed(1)} · {match.physicsLoadPercent.toFixed(0)}%</td><td class="p-3">{match.physicsTickMs.toFixed(2)} ms</td><td class="p-3"><div class="flex gap-2"><Button size="sm" variant="outline" onclick={() => command(selected!, 'kick_player', match.matchId)}>Kick</Button><Button size="sm" variant="outline" onclick={() => command(selected!, 'clear_match', match.matchId)}>Clear</Button><Button size="sm" variant="destructive" onclick={() => command(selected!, 'stop_match', match.matchId)}>Stop</Button></div></td></tr>{:else}<tr><td class="p-4 text-muted-foreground" colspan="5">No active match telemetry from this host.</td></tr>{/each}</tbody></table></div>
		<h3 class="mt-7 font-semibold">Fly machine inventory</h3><div class="mt-3 grid gap-2 md:grid-cols-2 lg:grid-cols-3">{#each instances as instance}<div class="rounded-lg border border-border p-3 text-sm"><p class="font-mono text-xs">{instance.machineId}</p><p class="mt-1 text-muted-foreground">{instance.appName ?? 'unknown app'} · {instance.region ?? 'unknown region'}</p><p class="text-xs text-muted-foreground">{instance.privateIp ?? 'no private IP'}</p></div>{:else}<p class="text-sm text-muted-foreground">Awaiting Fly DNS inventory.</p>{/each}</div>
		<h3 class="mt-7 font-semibold">Recent control commands</h3><div class="mt-3 space-y-2">{#each commands as item}<div class="flex flex-wrap items-center justify-between rounded-lg border border-border px-3 py-2 text-sm"><span>{item.type.replaceAll('_', ' ')}</span><span class="text-muted-foreground">{item.status}{item.error ? ` · ${item.error}` : ''}</span></div>{:else}<p class="text-sm text-muted-foreground">No commands sent.</p>{/each}</div>
	</section>{/if}
</div>
