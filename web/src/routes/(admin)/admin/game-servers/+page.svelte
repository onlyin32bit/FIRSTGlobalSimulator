<script lang="ts">
	import { onMount } from 'svelte';
	import { ApiError, api, type GameServer } from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';

	let servers = $state<GameServer[]>([]);
	let error = $state('');
	let createdKey = $state('');
	let submitting = $state(false);
	let form = $state({
		name: '',
		origin: 'http://localhost:3000',
		maxUsers: 50,
		maxMatches: 10,
		slots: 10
	});

	async function load() {
		try {
			servers = (await api.listGameServers()).servers;
			error = '';
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to load game servers.';
		}
	}

	async function create() {
		submitting = true;
		createdKey = '';
		try {
			const result = await api.createGameServer(form);
			createdKey = result.key;
			form = { name: '', origin: 'http://localhost:3000', maxUsers: 50, maxMatches: 10, slots: 10 };
			await load();
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to create game server.';
		} finally {
			submitting = false;
		}
	}

	async function disable(server: GameServer) {
		try {
			await api.updateGameServer(server.id, {
				status: server.status === 'disabled' ? 'provisioning' : 'disabled'
			});
			await load();
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to update game server.';
		}
	}

	onMount(() => {
		void load();
		const timer = window.setInterval(() => void load(), 5000);
		return () => window.clearInterval(timer);
	});
</script>

<div class="mx-auto max-w-[104rem] space-y-7">
	<div>
		<h1 class="text-3xl font-semibold tracking-tight">Game servers</h1>
		<p class="mt-2 text-sm text-muted-foreground">
			Provision remote Rust hosts and watch their capacity in real time.
		</p>
	</div>
	{#if error}<p
			class="rounded-lg border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive"
		>
			{error}
		</p>{/if}
	{#if createdKey}<div
			class="rounded-xl border border-amber-400/40 bg-amber-50 p-4 text-sm text-amber-950"
		>
			<p class="font-semibold">Copy this server key now — it will not be shown again.</p>
			<code class="mt-2 block overflow-x-auto rounded bg-white/70 p-3">{createdKey}</code>
			<p class="mt-2 text-xs">
				Set <code>CONTROL_PLANE_URL</code> and <code>GAME_SERVER_KEY</code> on the Rust host.
			</p>
		</div>{/if}
	<section class="rounded-xl border border-border bg-card p-5">
		<h2 class="font-semibold">Create a host</h2>
		<div class="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-5">
			<Input placeholder="Name" bind:value={form.name} /><Input
				placeholder="Public origin (http[s]://host:port)"
				bind:value={form.origin}
			/><Input type="number" min="1" placeholder="Max users" bind:value={form.maxUsers} /><Input
				type="number"
				min="1"
				placeholder="Max matches"
				bind:value={form.maxMatches}
			/><Input type="number" min="1" placeholder="Slots" bind:value={form.slots} />
		</div>
		<Button class="mt-4" disabled={submitting || !form.name || !form.origin} onclick={create}
			>{submitting ? 'Creating…' : 'Generate server key'}</Button
		>
	</section>
	<section class="overflow-hidden rounded-xl border border-border bg-card">
		<div class="grid divide-y divide-border md:grid-cols-2 md:divide-y-0 xl:grid-cols-3">
			{#each servers as server}<article class="border-border p-5 md:border-r">
					<div class="flex items-start justify-between gap-3">
						<div>
							<h2 class="font-semibold">{server.name}</h2>
							<p class="mt-1 text-xs text-muted-foreground">{server.origin}</p>
						</div>
						<span
							class="rounded-full px-2 py-1 text-[11px] {server.health === 'online'
								? 'bg-emerald-100 text-emerald-800'
								: 'bg-muted text-muted-foreground'}">{server.health}</span
						>
					</div>
					<div class="mt-5 grid grid-cols-3 gap-3 text-sm">
						<div>
							<p class="text-xs text-muted-foreground">Users</p>
							<p class="mt-1 font-semibold">{server.activeUsers} / {server.maxUsers}</p>
						</div>
						<div>
							<p class="text-xs text-muted-foreground">Matches</p>
							<p class="mt-1 font-semibold">{server.activeMatches} / {server.maxMatches}</p>
						</div>
						<div>
							<p class="text-xs text-muted-foreground">Slots</p>
							<p class="mt-1 font-semibold">{server.slots}</p>
						</div>
					</div>
					<div class="mt-5 flex items-center justify-between">
						<p class="text-xs text-muted-foreground">
							{server.lastHeartbeatAt
								? `Heartbeat ${new Date(server.lastHeartbeatAt).toLocaleTimeString()}`
								: 'Waiting for heartbeat'}
						</p>
						<Button size="sm" variant="outline" onclick={() => disable(server)}
							>{server.status === 'disabled' ? 'Enable' : 'Disable'}</Button
						>
					</div>
				</article>{:else}<div class="p-8 text-sm text-muted-foreground">
					No game servers provisioned yet.
				</div>{/each}
		</div>
	</section>
</div>
