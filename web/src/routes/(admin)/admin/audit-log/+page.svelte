<script lang="ts">
	import { onMount } from 'svelte';
	import { ApiError, api, type AdminAuditEntry } from '$lib/api';
	let entries = $state<AdminAuditEntry[]>([]);
	let loading = $state(true);
	let error = $state('');
	onMount(async () => {
		try {
			entries = (await api.listAdminAuditLog()).auditLog.items;
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to load audit history.';
		} finally {
			loading = false;
		}
	});
</script>

<div class="space-y-6">
	<div>
		<h1 class="text-3xl font-bold">Audit log</h1>
	</div>
	{#if error}<p
			class="rounded-md border border-destructive/30 bg-destructive/15 p-3 text-sm text-destructive"
		>
			{error}
		</p>{/if}
	<section class="overflow-x-auto rounded-xl border border-border bg-card">
		<table class="w-full min-w-180 text-left text-sm">
			<thead class="border-b border-border text-muted-foreground"
				><tr
					><th class="p-4">Action</th><th class="p-4">Actor</th><th class="p-4">Target</th><th
						class="p-4">Time</th
					></tr
				></thead
			><tbody
				>{#if loading}<tr
						><td class="p-5 text-muted-foreground" colspan="4">Loading audit history…</td></tr
					>{:else if entries.length === 0}<tr
						><td class="p-5 text-muted-foreground" colspan="4"
							>No administrative actions have been recorded yet.</td
						></tr
					>					{:else}{#each entries as entry (entry.id)}<tr class="border-b border-border last:border-0"
							><td class="p-4"
								><p class="font-medium">{entry.action}</p>
								{#if entry.metadata}<p class="mt-1 font-mono text-xs text-muted-foreground">
										{JSON.stringify(entry.metadata)}
									</p>{/if}</td
							><td class="p-4"
								>{entry.actorName}
								<p class="text-muted-foreground">{entry.actorEmail}</p></td
							><td class="p-4">{entry.targetType}: <code>{entry.targetId}</code></td><td
								class="p-4 text-muted-foreground">{new Date(entry.createdAt).toLocaleString()}</td
							></tr
						>{/each}{/if}</tbody
			>
		</table>
	</section>
</div>
