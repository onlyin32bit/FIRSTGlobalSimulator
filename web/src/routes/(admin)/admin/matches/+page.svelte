<script lang="ts">
	import { onMount } from 'svelte';
	import { IconEdit, IconPlus, IconX } from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { ApiError, api, type AdminMatch, type AdminUser } from '$lib/api';
	const statuses = ['LOBBY', 'IN_PROGRESS', 'FINISHED', 'CANCELLED'] as const;
	let matches = $state<AdminMatch[]>([]);
	let hosts = $state<AdminUser[]>([]);
	let loading = $state(true);
	let error = $state('');
	let notice = $state('');
	let formOpen = $state(false);
	let cancelOpen = $state(false);
	let selected = $state<AdminMatch | null>(null);
	let hostId = $state('');
	let maxPlayers = $state(6);
	let status = $state<AdminMatch['status']>('LOBBY');
	let cancelReason = $state('');
	let saving = $state(false);
	async function load() {
		loading = true;
		try {
			const [matchesResult, usersResult] = await Promise.all([
				api.listAdminMatches(),
				api.listUsers({ pageSize: 100 })
			]);
			matches = matchesResult.matches.items;
			hosts = usersResult.users.items.filter((user) => !user.disabledAt);
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to load matches.';
		} finally {
			loading = false;
		}
	}
	onMount(load);
	function openCreate() {
		selected = null;
		hostId = hosts[0]?.id || '';
		maxPlayers = 6;
		status = 'LOBBY';
		cancelReason = '';
		formOpen = true;
	}
	function openEdit(match: AdminMatch) {
		selected = match;
		hostId = match.hostId;
		maxPlayers = match.maxPlayers;
		status = match.status;
		cancelReason = match.cancelReason || '';
		formOpen = true;
	}
	async function save() {
		saving = true;
		try {
			const input = {
				hostId,
				gamePackId: 'fgc-2026' as const,
				maxPlayers: Number(maxPlayers),
				status,
				...(status === 'CANCELLED' && cancelReason ? { cancelReason } : {})
			};
			if (selected) await api.updateAdminMatch(selected.id, input);
			else await api.createAdminMatch(input);
			notice = selected ? 'Match updated.' : 'Match created.';
			formOpen = false;
			await load();
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to save match.';
		} finally {
			saving = false;
		}
	}
	async function cancel() {
		if (!selected) return;
		saving = true;
		try {
			await api.cancelAdminMatch(selected.id, cancelReason);
			notice = 'Match cancelled.';
			cancelOpen = false;
			await load();
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to cancel match.';
		} finally {
			saving = false;
		}
	}
</script>

<div class="space-y-6">
	<div class="flex items-center justify-between">
		<h1 class="text-3xl font-semibold">Matches</h1>
		<Button onclick={openCreate}><IconPlus class="mr-1 size-4" /> Create match</Button>
	</div>
	{#if error}<p
			class="rounded-lg border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive"
		>
			{error}
		</p>{/if}{#if notice}<p class="rounded-lg border border-border bg-muted p-3 text-sm">
			{notice}
		</p>{/if}
	<section class="overflow-x-auto rounded-xl border border-border bg-card">
		<table class="w-full min-w-200 text-left text-sm">
			<thead class="border-b border-border text-muted-foreground"
				><tr
					><th class="p-4 font-medium">Match</th><th class="p-4 font-medium">Host</th><th
						class="p-4 font-medium">Status</th
					><th class="p-4 font-medium">Created</th><th></th></tr
				></thead
			><tbody
				>{#if loading}<tr
						><td class="p-6 text-muted-foreground" colspan="5">Loading matches…</td></tr
					>{:else if matches.length === 0}<tr
						><td class="p-6 text-muted-foreground" colspan="5">No matches created.</td></tr
					>{:else}{#each matches as match}<tr class="border-b border-border last:border-0"
							><td class="p-4"
								><code>{match.id}</code>
								<p class="mt-1 text-xs text-muted-foreground">
									{match.gamePackId} · {match.maxPlayers} players
								</p></td
							><td class="p-4"
								>{match.hostName}
								<p class="text-xs text-muted-foreground">{match.hostEmail}</p></td
							><td class="p-4"
								><span class="rounded-full bg-muted px-2 py-1 text-xs">{match.status}</span></td
							><td class="p-4 text-xs text-muted-foreground"
								>{new Date(match.createdAt).toLocaleString()}</td
							><td class="p-4"
								><div class="flex justify-end gap-1">
									<Button variant="ghost" size="icon-sm" onclick={() => openEdit(match)}
										><IconEdit class="size-4" /><span class="sr-only">Edit</span></Button
									>{#if match.status !== 'CANCELLED'}<Button
											variant="ghost"
											size="icon-sm"
											onclick={() => {
												selected = match;
												cancelReason = '';
												cancelOpen = true;
											}}><IconX class="size-4" /><span class="sr-only">Cancel</span></Button
										>{/if}
								</div></td
							></tr
						>{/each}{/if}</tbody
			>
		</table>
	</section>
</div>
<Dialog.Root bind:open={formOpen}
	><Dialog.Content
		><Dialog.Header
			><Dialog.Title>{selected ? 'Edit match' : 'Create match'}</Dialog.Title><Dialog.Description
				>Edits the platform record only; it does not control a running game server.</Dialog.Description
			></Dialog.Header
		>
		<div class="space-y-4 py-4">
			<div>
				<Label for="host">Host</Label><select
					id="host"
					class="mt-1 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
					bind:value={hostId}
					>{#each hosts as host}<option value={host.id}>{host.name} · {host.email}</option
						>{/each}</select
				>
			</div>
			<div>
				<Label for="players">Maximum players</Label><Input
					id="players"
					type="number"
					min="1"
					max="6"
					bind:value={maxPlayers}
				/>
			</div>
			<div>
				<Label for="status">Status</Label><select
					id="status"
					class="mt-1 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
					bind:value={status}
					>{#each statuses as option}<option value={option}>{option}</option>{/each}</select
				>
			</div>
			{#if status === 'CANCELLED'}<div>
					<Label for="cancel-reason">Cancellation reason</Label><Input
						id="cancel-reason"
						bind:value={cancelReason}
					/>
				</div>{/if}
		</div>
		<Dialog.Footer
			><Button variant="outline" onclick={() => (formOpen = false)}>Cancel</Button><Button
				disabled={saving || !hostId || (status === 'CANCELLED' && cancelReason.trim().length < 3)}
				onclick={save}>{saving ? 'Saving…' : 'Save match'}</Button
			></Dialog.Footer
		></Dialog.Content
	></Dialog.Root
>
<Dialog.Root bind:open={cancelOpen}
	><Dialog.Content
		><Dialog.Header
			><Dialog.Title>Cancel match</Dialog.Title><Dialog.Description
				>Cancellation preserves the record and does not stop an already-running game server.</Dialog.Description
			></Dialog.Header
		>
		<div class="py-4">
			<Label for="cancel-note">Reason</Label><Input id="cancel-note" bind:value={cancelReason} />
		</div>
		<Dialog.Footer
			><Button variant="outline" onclick={() => (cancelOpen = false)}>Keep match</Button><Button
				variant="destructive"
				disabled={saving || cancelReason.trim().length < 3}
				onclick={cancel}>{saving ? 'Cancelling…' : 'Cancel match'}</Button
			></Dialog.Footer
		></Dialog.Content
	></Dialog.Root
>
