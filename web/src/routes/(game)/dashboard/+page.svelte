<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import { Input } from '$lib/components/ui/input';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Label } from '$lib/components/ui/label';
	import { api } from '$lib/api';

	// Svelte 5 state runes
	let matchId = $state('');
	let showCreateMatch = $state(false);

	// Basic interactions mocked
	function joinMatch() {
		if (!matchId) return;
		window.location.href = `/join/${matchId}`;
	}

	function openCreateMatch() {
		showCreateMatch = true;
	}

	async function submitCreateMatch() {
		// Just a simple mock/actual fetch
		try {
			const { match_id } = await api.createMatch({ gamePackId: 'fgc-2026' });
			window.location.href = `/join/${match_id}`;
		} catch (e) {
			console.error('Network error creating match:', e);
		}
	}
</script>

<div
	class="relative flex min-h-screen flex-col items-center justify-center overflow-hidden bg-background p-6 text-foreground"
>
	<!-- Abstract shapes for a modern flat background vibe -->
	<div
		class="pointer-events-none absolute -top-40 -left-40 h-96 w-96 rounded-full bg-primary/20 blur-3xl"
	></div>
	<div
		class="pointer-events-none absolute -right-40 -bottom-40 h-96 w-96 rounded-full bg-primary/20 blur-3xl"
	></div>

	<main class="z-10 flex w-full max-w-xl flex-col gap-8 text-center">
		<!-- Hero Section -->
		<div class="flex flex-col gap-2">
			<h1 class="text-5xl font-bold tracking-tight text-primary">FGC 2026</h1>
			<p class="text-xl font-semibold tracking-widest text-muted-foreground uppercase">
				Igniting Innovation
			</p>
		</div>

		<!-- Action Cards -->
		<Card.Root class="w-full border-border bg-card/50 shadow-xl backdrop-blur">
			<Card.Header>
				<Card.Title class="text-2xl">Match Simulator</Card.Title>
			</Card.Header>
			<Card.Content class="flex flex-col gap-6">
				<!-- Join Match -->
				<div class="flex flex-col gap-2">
					<Label for="match-id" class="sr-only text-left">Match ID</Label>
					<div class="flex gap-2">
						<Input
							id="match-id"
							placeholder="Enter Match ID (e.g. 1234-abcd)"
							bind:value={matchId}
							class="bg-input/50"
						/>
						<Button onclick={joinMatch} variant="default" class="w-24">Join</Button>
					</div>
				</div>

				<!-- Divider -->
				<div class="relative">
					<div class="absolute inset-0 flex items-center">
						<span class="w-full border-t border-border"></span>
					</div>
					<div class="relative flex justify-center text-xs uppercase">
						<span class="bg-card px-2 font-semibold text-muted-foreground">Or</span>
					</div>
				</div>

				<!-- Other Actions -->
				<div class="flex flex-col gap-3">
					<Button variant="secondary" class="w-full" onclick={openCreateMatch}
						>Create New Match</Button
					>
					<Button variant="outline" class="w-full" href="/scene">Enter Sandbox (Offline)</Button>
					<Button variant="ghost" class="w-full" href="/robot">Robot Builder</Button>
				</div>
			</Card.Content>
		</Card.Root>
	</main>
</div>

<!-- Create Match Dialog -->
<Dialog.Root bind:open={showCreateMatch}>
	<Dialog.Content class="sm:max-w-[425px]">
		<Dialog.Header>
			<Dialog.Title>Create Match</Dialog.Title>
			<Dialog.Description>Select a game pack to initialize your match lobby.</Dialog.Description>
		</Dialog.Header>
		<div class="flex flex-col gap-4 py-4">
			<div class="flex flex-col gap-2">
				<Label for="game-pack">Game Pack</Label>
				<Input id="game-pack" value="Igniting Innovation (FGC 2026)" disabled />
			</div>
			<div class="flex flex-col gap-2">
				<Label for="max-players">Max Players</Label>
				<Input id="max-players" type="number" value="6" disabled />
			</div>
		</div>
		<Dialog.Footer>
			<Button variant="outline" onclick={() => (showCreateMatch = false)}>Cancel</Button>
			<Button onclick={submitCreateMatch}>Initialize Lobby</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
