<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import type { DashboardController } from './dashboard-controller.svelte';

	let {
		controller,
		onJoin,
		onCreate
	}: { controller: DashboardController; onJoin: () => void; onCreate: () => void } = $props();
</script>

<Dialog.Root open={controller.dialog !== null} onOpenChange={(open) => !open && controller.close()}>
	<Dialog.Content class="sm:max-w-md">
		<Dialog.Header>
			<Dialog.Title
				>{controller.dialog === 'create' ? 'Create a match' : 'Join a match'}</Dialog.Title
			>
			<Dialog.Description>
				{controller.dialog === 'create'
					? 'Set up an eight-player FGC 2026 lobby.'
					: 'Paste the match ID shared by the host.'}
			</Dialog.Description>
		</Dialog.Header>

		{#if controller.dialog === 'join'}
			<div class="py-4">
				<Label for="match-id">Match ID</Label>
				<Input
					id="match-id"
					class="mt-2 h-11"
					placeholder="e.g. 69ac…"
					bind:value={controller.matchId}
					onkeydown={(event) => event.key === 'Enter' && onJoin()}
				/>
			</div>
		{:else}
			<p class="py-5 text-sm text-muted-foreground">
				Your lobby will have three driver and one human-player station per alliance.
			</p>
		{/if}

		{#if controller.error}<p class="text-sm text-destructive" role="alert">
				{controller.error}
			</p>{/if}
		<Dialog.Footer>
			<Button variant="ghost" onclick={() => controller.close()}>Cancel</Button>
			<Button
				onclick={controller.dialog === 'create' ? onCreate : onJoin}
				disabled={controller.isCreating}
			>
				{controller.isCreating
					? 'Creating…'
					: controller.dialog === 'create'
						? 'Create match'
						: 'Join match'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
