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
	let metadata = $state<Awaited<ReturnType<typeof api.getGamePackMetadata>> | null>(null);
	onMount(async () => {
		try {
			packs = (await api.listAdminGamePacks()).packs;
			metadata = await api.getGamePackMetadata('fgc-2026');
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
		{#each packs as pack (pack.id)}<article class="rounded-xl border border-border bg-card p-6">
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
	{#if metadata}
		<section class="rounded-xl border border-border bg-card p-6">
			<div class="flex flex-col justify-between gap-2 sm:flex-row sm:items-end">
				<div>
					<h2 class="text-xl font-semibold">Loaded rule metadata</h2>
					<p class="mt-1 text-sm text-muted-foreground">
						Parsed from the manifest and compiled Rhai scripts on the game server.
					</p>
				</div>
				<span
					class="rounded-full bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-700"
					>{metadata.scripts.length} scripts validated</span
				>
			</div>
			<div class="mt-5 grid gap-3 sm:grid-cols-3">
				<div class="rounded-lg bg-muted/50 p-4">
					<p class="text-xs text-muted-foreground">Field definitions</p>
					<p class="mt-1 text-2xl font-semibold">{Object.keys(metadata.manifest.field).length}</p>
				</div>
				<div class="rounded-lg bg-muted/50 p-4">
					<p class="text-xs text-muted-foreground">Game objects</p>
					<p class="mt-1 text-2xl font-semibold">{metadata.manifest.objects.length}</p>
				</div>
				<div class="rounded-lg bg-muted/50 p-4">
					<p class="text-xs text-muted-foreground">Match phases</p>
					<p class="mt-1 text-2xl font-semibold">{metadata.manifest.phases.length}</p>
				</div>
			</div>
			<div class="mt-5 divide-y divide-border rounded-lg border border-border">
				{#each metadata.scripts as script (script.path)}
					<div class="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
						<div>
							<p class="font-mono text-sm font-medium">{script.path.split('/').pop()}</p>
							<p class="mt-1 text-xs text-muted-foreground">
								{script.functions.map((fn) => fn.name).join(' · ')}
							</p>
						</div>
						<span class="text-xs text-muted-foreground"
							>{script.engineCalls.length} engine calls</span
						>
					</div>
				{/each}
			</div>
		</section>
	{/if}
</div>
