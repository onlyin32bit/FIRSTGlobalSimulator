<script lang="ts">
	import { onMount } from 'svelte';
	import { ApiError, api } from '$lib/api';
	let packs = $state<
		Array<{
			id: string;
			name: string;
			version: string;
			status: string;
			engineCompatibility: string;
		}>
	>([]);
	let error = $state('');
	onMount(async () => {
		try {
			packs = (await api.listAdminGamePacks()).packs;
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to load game packs.';
		}
	});
</script>

<div class="space-y-6">
	<div>
		<h1 class="text-3xl font-bold">Game packs</h1>
	</div>
	{#if error}<p
			class="rounded-md border border-destructive/30 bg-destructive/15 p-3 text-sm text-destructive"
		>
			{error}
		</p>{/if}
	<div class="grid gap-4 md:grid-cols-2">
		{#each packs as pack}<article class="rounded-xl border border-border bg-card p-6">
				<div class="flex items-start justify-between">
					<div>
						<h2 class="text-xl font-semibold">{pack.name}</h2>
						<p class="mt-1 font-mono text-sm text-muted-foreground">{pack.id}</p>
					</div>
					<span class="rounded-full bg-primary/15 px-2 py-1 text-xs font-medium text-primary"
						>{pack.status}</span
					>
				</div>
				<dl class="mt-6 grid grid-cols-2 gap-4 text-sm">
					<div>
						<dt class="text-muted-foreground">Version</dt>
						<dd class="mt-1 font-medium">{pack.version}</dd>
					</div>
					<div>
						<dt class="text-muted-foreground">Engine</dt>
						<dd class="mt-1 font-medium">{pack.engineCompatibility}</dd>
					</div>
				</dl>
			</article>{/each}
	</div>
</div>
