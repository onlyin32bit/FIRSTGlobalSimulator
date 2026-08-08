<script lang="ts">
	import { onMount } from 'svelte';
	import { IconEdit, IconLock, IconLockOpen, IconSearch } from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { ApiError, api, type AdminUser, type UserRole } from '$lib/api';

	let users = $state<AdminUser[]>([]);
	let search = $state('');
	let loading = $state(true);
	let error = $state('');
	let notice = $state('');
	let selected = $state<AdminUser | null>(null);
	let formOpen = $state(false);
	let disableOpen = $state(false);
	let name = $state('');
	let team = $state('');
	let role = $state<UserRole>('user');
	let reason = $state('');
	let saving = $state(false);
	async function load() {
		loading = true;
		try {
			users = (await api.listUsers({ search })).users.items;
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to load users.';
		} finally {
			loading = false;
		}
	}
	onMount(load);
	function edit(user: AdminUser) {
		selected = user;
		name = user.name;
		team = user.team || '';
		role = user.role;
		formOpen = true;
	}
	async function save() {
		if (!selected) return;
		saving = true;
		error = '';
		try {
			await api.updateUser(selected.id, { name, team, role });
			notice = 'User updated.';
			formOpen = false;
			await load();
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to update user.';
		} finally {
			saving = false;
		}
	}
	async function toggleDisabled() {
		if (!selected) return;
		saving = true;
		try {
			if (selected.disabledAt) {
				await api.enableUser(selected.id);
				notice = 'Account enabled.';
			} else {
				await api.disableUser(selected.id, reason);
				notice = 'Account disabled and sessions revoked.';
			}
			disableOpen = false;
			await load();
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to change account status.';
		} finally {
			saving = false;
		}
	}
	async function revoke(user: AdminUser) {
		if (!confirm(`Revoke all sessions for ${user.email}?`)) return;
		try {
			const result = await api.revokeUserSessions(user.id);
			notice = `${result.sessionsRevoked} session(s) revoked.`;
			await load();
		} catch (e) {
			error = e instanceof ApiError ? e.message : 'Unable to revoke sessions.';
		}
	}
</script>

<div class="space-y-6">
	<div class="flex items-center justify-between">
		<h1 class="text-3xl font-semibold">Users</h1>
		<form
			class="relative w-80"
			onsubmit={(e) => {
				e.preventDefault();
				load();
			}}
		>
			<IconSearch
				class="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
			/><Input class="pl-9" placeholder="Search users" bind:value={search} />
		</form>
	</div>
	{#if error}<p
			class="rounded-lg border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive"
		>
			{error}
		</p>{/if}{#if notice}<p class="rounded-lg border border-border bg-muted p-3 text-sm">
			{notice}
		</p>{/if}
	<section class="overflow-x-auto rounded-xl border border-border bg-card">
		<table class="w-full min-w-220 text-left text-sm">
			<thead class="border-b border-border text-muted-foreground"
				><tr
					><th class="p-4 font-medium">User</th><th class="p-4 font-medium">Role</th><th
						class="p-4 font-medium">Usage</th
					><th class="p-4 font-medium"></th></tr
				></thead
			><tbody
				>{#if loading}<tr><td class="p-6 text-muted-foreground" colspan="4">Loading users…</td></tr
					>					{:else}{#each users as user (user.id)}<tr class="border-b border-border last:border-0"
							><td class="p-4"
								><p class="font-medium">{user.name}</p>
								<p class="text-xs text-muted-foreground">
									{user.email} · {user.team || 'No team'}{#if user.disabledAt}
										· Disabled{/if}
								</p></td
							><td class="p-4"
								><span class="rounded-full bg-muted px-2 py-1 text-xs capitalize">{user.role}</span
								></td
							><td class="p-4 text-xs text-muted-foreground"
								>{user.robotCount} robots · {user.matchHostCount} matches · {user.sessionCount} sessions</td
							><td class="p-4"
								><div class="flex justify-end gap-1">
									<Button variant="ghost" size="icon-sm" onclick={() => edit(user)}
										><IconEdit class="size-4" /><span class="sr-only">Edit user</span></Button
									><Button
										variant="ghost"
										size="icon-sm"
										onclick={() => {
											selected = user;
											reason = '';
											disableOpen = true;
										}}
										>{#if user.disabledAt}<IconLockOpen class="size-4" />{:else}<IconLock
												class="size-4"
											/>{/if}<span class="sr-only">Change account status</span></Button
									><Button variant="ghost" size="sm" onclick={() => revoke(user)}>Sessions</Button>
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
			><Dialog.Title>Edit user</Dialog.Title><Dialog.Description
				>Update profile and access role.</Dialog.Description
			></Dialog.Header
		>
		<div class="space-y-4 py-4">
			<div><Label for="user-name">Name</Label><Input id="user-name" bind:value={name} /></div>
			<div><Label for="user-team">Team</Label><Input id="user-team" bind:value={team} /></div>
			<div>
				<Label for="user-role">Role</Label><select
					id="user-role"
					class="mt-1 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
					bind:value={role}
					><option value="user">User</option><option value="admin">Administrator</option></select
				>
			</div>
		</div>
		<Dialog.Footer
			><Button variant="outline" onclick={() => (formOpen = false)}>Cancel</Button><Button
				disabled={saving}
				onclick={save}>{saving ? 'Saving…' : 'Save changes'}</Button
			></Dialog.Footer
		></Dialog.Content
	></Dialog.Root
>
<Dialog.Root bind:open={disableOpen}
	><Dialog.Content
		><Dialog.Header
			><Dialog.Title>{selected?.disabledAt ? 'Enable account' : 'Disable account'}</Dialog.Title
			><Dialog.Description
				>{selected?.disabledAt
					? 'The user will be able to sign in again.'
					: 'This immediately revokes active sessions. Enter a reason for the audit log.'}</Dialog.Description
			></Dialog.Header
		>{#if !selected?.disabledAt}<div class="py-4">
				<Label for="disable-reason">Reason</Label><Input
					id="disable-reason"
					bind:value={reason}
					placeholder="Policy or support reason"
				/>
			</div>{/if}<Dialog.Footer
			><Button variant="outline" onclick={() => (disableOpen = false)}>Cancel</Button><Button
				variant={selected?.disabledAt ? 'default' : 'destructive'}
				disabled={saving || (!selected?.disabledAt && reason.trim().length < 3)}
				onclick={toggleDisabled}
				>{saving ? 'Saving…' : selected?.disabledAt ? 'Enable account' : 'Disable account'}</Button
			></Dialog.Footer
		></Dialog.Content
	></Dialog.Root
>
