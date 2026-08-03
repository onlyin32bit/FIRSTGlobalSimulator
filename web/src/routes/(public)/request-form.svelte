<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { Textarea } from '$lib/components/ui/textarea';
	import { api } from '$lib/api';

	let inviteEmail = $state('');
	let inviteName = $state('');
	let inviteTeam = $state('');
	let inviteMessage = $state('');
	let inviteStatus = $state<'idle' | 'loading' | 'success' | 'error'>('idle');
	let errorMessage = $state('');

	let { isOpen = $bindable(false) } = $props();

	async function requestInvite(e: Event) {
		e.preventDefault();
		inviteStatus = 'loading';
		errorMessage = '';

		try {
			await api.requestInvite({
				email: inviteEmail,
				name: inviteName,
				team: inviteTeam,
				message: inviteMessage
			});
			inviteStatus = 'success';
		} catch (err: any) {
			inviteStatus = 'error';
			errorMessage = err instanceof Error ? err.message : 'Network error.';
		}
	}
</script>

<Dialog.Root bind:open={isOpen}>
	<Dialog.Content class="sm:max-w-md">
		<Dialog.Header>
			<Dialog.Title>Request Beta Invite</Dialog.Title>
			<Dialog.Description>
				Leave your contact info and we'll send you an invitation code when spots open up.
			</Dialog.Description>
		</Dialog.Header>

		{#if inviteStatus === 'success'}
			<div class="flex flex-col items-center gap-4 py-12 text-center">
				<div
					class="flex h-12 w-12 items-center justify-center rounded-full bg-green-500/20 text-green-500"
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						width="24"
						height="24"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
						><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" /><polyline
							points="22 4 12 14.01 9 11.01"
						/></svg
					>
				</div>
				<p class="font-bold">Request Received!</p>
				<p class="text-sm text-muted-foreground">Keep an eye on your inbox.</p>
				<Button onclick={() => (isInviteDialogOpen = false)} class="mt-4">Close</Button>
			</div>
		{:else}
			<form onsubmit={requestInvite} class="flex flex-col gap-4 py-4">
				{#if inviteStatus === 'error'}
					<div class="rounded-md border border-red-900 bg-red-950/50 p-3 text-sm text-red-200">
						{errorMessage}
					</div>
				{/if}

				<div class="flex flex-col gap-2">
					<Label for="req-name">Full Name</Label>
					<Input id="req-name" bind:value={inviteName} required />
				</div>
				<div class="flex flex-col gap-2">
					<Label for="req-team">Team / Organization</Label>
					<Input id="req-team" bind:value={inviteTeam} required placeholder="e.g. Team Vietnam" />
				</div>
				<div class="flex flex-col gap-2">
					<Label for="req-email">Email Address</Label>
					<Input id="req-email" type="email" bind:value={inviteEmail} required />
				</div>
				<div class="flex flex-col gap-2">
					<Label for="req-msg">Additional Info (Optional)</Label>
					<Textarea
						id="req-msg"
						bind:value={inviteMessage}
						placeholder="How did you hear about us?"
						rows={3}
					/>
				</div>

				<Dialog.Footer class="mt-4">
					<Button type="submit" class="w-full" disabled={inviteStatus === 'loading'}>
						{inviteStatus === 'loading' ? 'Sending...' : 'Submit Request'}
					</Button>
				</Dialog.Footer>
			</form>
		{/if}
	</Dialog.Content>
</Dialog.Root>
