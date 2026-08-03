<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { ApiError, api } from '$lib/api';

	let matchId = $state('');
	let showCreateMatch = $state(false);
	let isCreating = $state(false);
	let errorMessage = $state('');

	function joinMatch() {
		const normalizedMatchId = matchId.trim();
		if (normalizedMatchId === 'test-match') {
			window.location.href = '/match/test-match';
			return;
		}
		errorMessage = 'Only the always-on test-match is available to join right now.';
	}

	async function submitCreateMatch() {
		isCreating = true;
		errorMessage = '';
		try {
			const { match_id } = await api.createMatch({ gamePackId: 'fgc-2026' });
			matchId = match_id;
			showCreateMatch = false;
			errorMessage = `Match ${match_id} was created. Match joining will be available in a follow-up release.`;
		} catch (error) {
			errorMessage =
				error instanceof ApiError ? error.message : 'Unable to create a match. Please try again.';
		} finally {
			isCreating = false;
		}
	}
</script>

<div
	class="relative flex min-h-[calc(100vh-3.5rem)] flex-col items-center justify-center overflow-hidden bg-background p-6 text-foreground"
>
	<div
		class="pointer-events-none absolute -top-40 -left-40 h-96 w-96 rounded-full bg-primary/20 blur-3xl"
	></div>
	<div
		class="pointer-events-none absolute -right-40 -bottom-40 h-96 w-96 rounded-full bg-primary/20 blur-3xl"
	></div>

	<main class="z-10 flex w-full max-w-xl flex-col gap-8 text-center">
		<div class="flex flex-col gap-2">
			<h1 class="text-5xl font-bold tracking-tight text-primary">FGC 2026</h1>
			<p class="text-xl font-semibold tracking-widest text-muted-foreground uppercase">
				Igniting Innovation
			</p>
		</div>

		<Card.Root class="w-full border-border bg-card/50 shadow-xl backdrop-blur">
			<Card.Header><Card.Title class="text-2xl">Match Simulator</Card.Title></Card.Header>
			<Card.Content class="flex flex-col gap-6">
				{#if errorMessage}
					<div
						class="rounded-md border border-border bg-muted/50 p-3 text-left text-sm"
						role="status"
					>
						{errorMessage}
					</div>
				{/if}
				<div class="flex flex-col gap-2">
					<Label for="match-id" class="sr-only">Match ID</Label>
					<div class="flex gap-2">
						<Input
							id="match-id"
							placeholder="Enter Match ID"
							bind:value={matchId}
							class="bg-input/50"
						/>
						<Button onclick={joinMatch} class="w-24">Join</Button>
					</div>
				</div>
				<div class="relative">
					<div class="absolute inset-0 flex items-center">
						<span class="w-full border-t border-border"></span>
					</div>
					<div class="relative flex justify-center text-xs uppercase">
						<span class="bg-card px-2 font-semibold text-muted-foreground">Or</span>
					</div>
				</div>
				<div class="flex flex-col gap-3">
					<Button href="/match/test-match" class="w-full">Join live test match</Button>
					<Button
						variant="secondary"
						class="w-full"
						onclick={() => {
							errorMessage = '';
							showCreateMatch = true;
						}}>Create new match</Button
					>
					<Button variant="outline" class="w-full" href="/scene">Enter sandbox (offline)</Button>
					<Button variant="ghost" class="w-full" href="/robot">Robot builder</Button>
				</div>
			</Card.Content>
		</Card.Root>
	</main>
</div>

<Dialog.Root bind:open={showCreateMatch}>
	<Dialog.Content class="sm:max-w-[425px]">
		<Dialog.Header
			><Dialog.Title>Create match</Dialog.Title><Dialog.Description
				>Initialize a new FGC 2026 lobby.</Dialog.Description
			></Dialog.Header
		>
		<div class="flex flex-col gap-4 py-4">
			<div class="flex flex-col gap-2">
				<Label for="game-pack">Game pack</Label><Input
					id="game-pack"
					value="Igniting Innovation (FGC 2026)"
					disabled
				/>
			</div>
			<div class="flex flex-col gap-2">
				<Label for="max-players">Max players</Label><Input
					id="max-players"
					type="number"
					value="6"
					disabled
				/>
			</div>
		</div>
		<Dialog.Footer
			><Button variant="outline" onclick={() => (showCreateMatch = false)} disabled={isCreating}
				>Cancel</Button
			><Button onclick={submitCreateMatch} disabled={isCreating}
				>{isCreating ? 'Creating…' : 'Initialize lobby'}</Button
			></Dialog.Footer
		>
	</Dialog.Content>
</Dialog.Root>
