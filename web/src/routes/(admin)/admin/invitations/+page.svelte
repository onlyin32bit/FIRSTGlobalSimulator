<script lang="ts">
	import { onMount } from 'svelte';
	import { IconCopy, IconEdit, IconPlus, IconTrash } from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { ApiError, api, type Invitation } from '$lib/api';
	let invitations = $state<Invitation[]>([]);
	let loading = $state(true);
	let error = $state('');
	let notice = $state('');
	let formOpen = $state(false);
	let editOpen = $state(false);
	let selected = $state<Invitation | null>(null);
	let expiresAt = $state('');
	let saving = $state(false);
	async function load() {
		loading = true;
		try {
			invitations = (await api.listInvitations()).invitations;
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to load invitations.';
		} finally {
			loading = false;
		}
	}
	onMount(load);
	async function create() {
		saving = true;
		try {
			const { invitation } = await api.createInvitation({
				expiresAt: expiresAt ? new Date(expiresAt) : undefined
			});
			invitations = [invitation, ...invitations];
			notice = `Created ${invitation.code}.`;
			expiresAt = '';
			formOpen = false;
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to create invitation.';
		} finally {
			saving = false;
		}
	}
	async function saveExpiry() {
		if (!selected) return;
		saving = true;
		try {
			const { invitation } = await api.updateInvitation(selected.code, {
				expiresAt: expiresAt ? new Date(expiresAt) : null
			});
			invitations = invitations.map((item) => (item.code === invitation.code ? invitation : item));
			editOpen = false;
			notice = 'Invitation updated.';
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to update invitation.';
		} finally {
			saving = false;
		}
	}
	async function revoke(invitation: Invitation) {
		if (!confirm(`Revoke ${invitation.code}? This cannot be undone.`)) return;
		try {
			const { invitation: updated } = await api.revokeInvitation(invitation.code);
			invitations = invitations.map((item) => (item.code === updated.code ? updated : item));
			notice = 'Invitation revoked.';
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to revoke invitation.';
		}
	}
</script>

<div class="space-y-6">
	<div class="flex items-center justify-between">
		<h1 class="text-3xl font-semibold">Invitations</h1>
		<Button
			onclick={() => {
				expiresAt = '';
				formOpen = true;
			}}><IconPlus class="mr-1 size-4" /> Create invitation</Button
		>
	</div>
	{#if error}<p
			class="rounded-lg border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive"
		>
			{error}
		</p>{/if}{#if notice}<p class="rounded-lg border border-border bg-muted p-3 text-sm">
			{notice}
		</p>{/if}
	<section class="overflow-x-auto rounded-xl border border-border bg-card">
		<table class="w-full min-w-180 text-left text-sm">
			<thead class="border-b border-border text-muted-foreground"
				><tr
					><th class="p-4 font-medium">Code</th><th class="p-4 font-medium">Status</th><th
						class="p-4 font-medium">Expiry</th
					><th class="p-4"></th></tr
				></thead
			><tbody
				>{#if loading}<tr
						><td colspan="4" class="p-6 text-muted-foreground">Loading invitations…</td></tr
					>					{:else}{#each invitations as invitation (invitation.code)}<tr class="border-b border-border last:border-0"
							><td class="p-4"
								><code class="font-semibold">{invitation.code}</code>
								<p class="mt-1 text-xs text-muted-foreground">
									Created {new Date(invitation.createdAt).toLocaleString()}
								</p></td
							><td class="p-4"
								><span class="rounded-full bg-muted px-2 py-1 text-xs capitalize"
									>{invitation.status}</span
								></td
							><td class="p-4 text-muted-foreground"
								>{invitation.expiresAt
									? new Date(invitation.expiresAt).toLocaleString()
									: 'Never'}</td
							><td class="p-4"
								><div class="flex justify-end gap-1">
									<Button
										variant="ghost"
										size="icon-sm"
										onclick={() => navigator.clipboard.writeText(invitation.code)}
										><IconCopy class="size-4" /><span class="sr-only">Copy</span></Button
									>{#if invitation.status === 'active'}<Button
											variant="ghost"
											size="icon-sm"
											onclick={() => {
												selected = invitation;
												expiresAt = invitation.expiresAt
													? new Date(invitation.expiresAt).toISOString().slice(0, 16)
													: '';
												editOpen = true;
											}}><IconEdit class="size-4" /><span class="sr-only">Edit</span></Button
										><Button variant="ghost" size="icon-sm" onclick={() => revoke(invitation)}
											><IconTrash class="size-4" /><span class="sr-only">Revoke</span></Button
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
			><Dialog.Title>Create invitation</Dialog.Title><Dialog.Description
				>Creates a one-time six-character access code.</Dialog.Description
			></Dialog.Header
		>
		<div class="py-4">
			<Label for="new-expiry">Expiry (optional)</Label><Input
				id="new-expiry"
				type="datetime-local"
				bind:value={expiresAt}
			/>
		</div>
		<Dialog.Footer
			><Button variant="outline" onclick={() => (formOpen = false)}>Cancel</Button><Button
				disabled={saving}
				onclick={create}>{saving ? 'Creating…' : 'Create code'}</Button
			></Dialog.Footer
		></Dialog.Content
	></Dialog.Root
>
<Dialog.Root bind:open={editOpen}
	><Dialog.Content
		><Dialog.Header
			><Dialog.Title>Edit invitation</Dialog.Title><Dialog.Description
				>Only active, unused invitations can be edited.</Dialog.Description
			></Dialog.Header
		>
		<div class="py-4">
			<Label for="edit-expiry">Expiry</Label><Input
				id="edit-expiry"
				type="datetime-local"
				bind:value={expiresAt}
			/>
			<p class="mt-2 text-xs text-muted-foreground">Clear the field to remove expiry.</p>
		</div>
		<Dialog.Footer
			><Button variant="outline" onclick={() => (editOpen = false)}>Cancel</Button><Button
				disabled={saving}
				onclick={saveExpiry}>{saving ? 'Saving…' : 'Save changes'}</Button
			></Dialog.Footer
		></Dialog.Content
	></Dialog.Root
>
