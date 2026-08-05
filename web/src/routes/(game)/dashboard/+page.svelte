<script lang="ts">
	import {
		IconActivity,
		IconArrowRight,
		IconBox,
		IconPlayerPlay,
		IconPlus,
		IconRobot,
		IconWorld
	} from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { ApiError, api } from '$lib/api';
	import { goto } from '$app/navigation';

	let matchId = $state('');
	let showCreateMatch = $state(false);
	let isCreating = $state(false);
	let errorMessage = $state('');

	function joinMatch() {
		const normalizedMatchId = matchId.trim();
		if (!normalizedMatchId) return;
		void goto(`/match/${encodeURIComponent(normalizedMatchId)}/lobby`);
	}

	async function submitCreateMatch() {
		isCreating = true;
		errorMessage = '';
		try {
			const { match_id } = await api.createMatch({ gamePackId: 'fgc-2026' });
			showCreateMatch = false;
			await goto(`/match/${match_id}/lobby`);
		} catch (error) {
			errorMessage =
				error instanceof ApiError ? error.message : 'Unable to create a match. Please try again.';
		} finally {
			isCreating = false;
		}
	}
</script>

<div
	class="relative min-h-[calc(100vh-3.5rem)] overflow-hidden bg-background px-5 py-8 text-foreground sm:px-8 lg:px-12 lg:py-12"
>
	<div
		class="pointer-events-none absolute -top-48 -left-32 h-[32rem] w-[32rem] rounded-full bg-primary/15 blur-3xl"
	></div>
	<div
		class="pointer-events-none absolute -right-48 bottom-0 h-[28rem] w-[28rem] rounded-full bg-primary/10 blur-3xl"
	></div>

	<main class="relative z-10 mx-auto flex w-full max-w-6xl flex-col gap-8">
		<section class="flex flex-col justify-between gap-6 md:flex-row md:items-end">
			<div class="max-w-2xl">
				<p class="text-xs font-semibold tracking-[0.24em] text-primary uppercase">
					Simulator control
				</p>
				<h1 class="mt-3 text-4xl font-semibold tracking-tight sm:text-5xl">
					Ready to run the next match?
				</h1>
				<p class="mt-3 max-w-xl text-base leading-7 text-muted-foreground">
					Build, test, and join the FGC 2026 field from one place. Start with the live arena or open
					a private lobby.
				</p>
			</div>
			<div
				class="flex items-center gap-2 rounded-full border border-border bg-card/70 px-3 py-2 text-sm shadow-sm backdrop-blur"
			>
				<span
					class="size-2 rounded-full bg-emerald-500 shadow-[0_0_0_4px_color-mix(in_oklab,#22c55e_20%,transparent)]"
				></span>
				<span class="font-medium">Systems operational</span>
			</div>
		</section>

		{#if errorMessage}
			<div
				class="rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive"
				role="alert"
			>
				{errorMessage}
			</div>
		{/if}

		<section class="grid gap-4 md:grid-cols-3">
			<Card.Root class="border-primary/30 bg-card/80 shadow-lg backdrop-blur md:col-span-2">
				<Card.Header class="flex flex-row items-start justify-between gap-4 space-y-0">
					<div>
						<Card.Title class="flex items-center gap-2 text-xl"
							><IconActivity class="size-5 text-primary" /> Live arena</Card.Title
						><Card.Description class="mt-2 max-w-lg"
							>Create a persisted match and validate your robot and controls in the live arena.</Card.Description
						>
					</div>
					<span
						class="rounded-full bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-600"
						>Online</span
					>
				</Card.Header>
				<Card.Footer class="flex flex-col items-stretch gap-3 sm:flex-row sm:items-center">
					<Button onclick={submitCreateMatch} disabled={isCreating} class="sm:w-auto"
						>Create live match <IconArrowRight data-icon="inline-end" /></Button
					>
					<span class="text-xs text-muted-foreground"
						>A lobby record will be created in the database.</span
					>
				</Card.Footer>
			</Card.Root>
			<Card.Root class="border-border bg-card/70 shadow-sm backdrop-blur">
				<Card.Header
					><Card.Title class="text-base">Game pack</Card.Title><Card.Description
						>Current simulation ruleset</Card.Description
					></Card.Header
				>
				<Card.Content
					><div class="flex items-center gap-3">
						<span
							class="flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary"
							><IconBox class="size-5" /></span
						>
						<div>
							<p class="font-semibold">Igniting Innovation</p>
							<p class="text-xs text-muted-foreground">FGC 2026 · v1.0.0</p>
						</div>
					</div></Card.Content
				>
			</Card.Root>
		</section>

		<section class="grid gap-4 lg:grid-cols-[1.1fr_0.9fr]">
			<Card.Root class="border-border bg-card/80 shadow-sm backdrop-blur">
				<Card.Header
					><Card.Title class="text-xl">Enter a match</Card.Title><Card.Description
						>Have a match ID from a teammate? Enter it here to connect.</Card.Description
					></Card.Header
				>
				<Card.Content class="flex flex-col gap-2 sm:flex-row sm:items-end">
					<div class="flex-1">
						<Label for="match-id">Match ID</Label><Input
							id="match-id"
							class="mt-2 bg-background/70"
							placeholder="e.g. match ID from a teammate"
							bind:value={matchId}
							onkeydown={(event) => event.key === 'Enter' && joinMatch()}
						/>
					</div>
					<Button onclick={joinMatch} class="sm:w-24">Join</Button>
				</Card.Content>
			</Card.Root>
			<Card.Root class="border-border bg-card/80 shadow-sm backdrop-blur">
				<Card.Header
					><Card.Title class="text-xl">Create a lobby</Card.Title><Card.Description
						>Set up a new match for your team.</Card.Description
					></Card.Header
				>
				<Card.Footer
					><Button
						variant="secondary"
						class="w-full"
						onclick={() => {
							errorMessage = '';
							showCreateMatch = true;
						}}><IconPlus data-icon="inline-start" /> Create new match</Button
					></Card.Footer
				>
			</Card.Root>
		</section>

		<section class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
			<a
				href="/robot"
				class="group rounded-xl border border-border bg-card/50 p-5 transition-colors hover:border-primary/50 hover:bg-card"
				><div class="flex items-center justify-between">
					<span class="flex size-10 items-center justify-center rounded-lg bg-muted text-foreground"
						><IconRobot class="size-5" /></span
					><IconArrowRight
						class="size-4 text-muted-foreground transition-transform group-hover:translate-x-1"
					/>
				</div>
				<h2 class="mt-5 font-semibold">Robot builder</h2>
				<p class="mt-1 text-sm leading-6 text-muted-foreground">
					Tune your drivetrain and save a build for your next match.
				</p></a
			>
			<a
				href="/scene"
				class="group rounded-xl border border-border bg-card/50 p-5 transition-colors hover:border-primary/50 hover:bg-card"
				><div class="flex items-center justify-between">
					<span class="flex size-10 items-center justify-center rounded-lg bg-muted text-foreground"
						><IconWorld class="size-5" /></span
					><IconArrowRight
						class="size-4 text-muted-foreground transition-transform group-hover:translate-x-1"
					/>
				</div>
				<h2 class="mt-5 font-semibold">Offline sandbox</h2>
				<p class="mt-1 text-sm leading-6 text-muted-foreground">
					Explore the field and test movement without joining a match.
				</p></a
			>
			<div class="rounded-xl border border-dashed border-border bg-transparent p-5">
				<div
					class="flex size-10 items-center justify-center rounded-lg bg-muted text-muted-foreground"
				>
					<IconPlayerPlay class="size-5" />
				</div>
				<h2 class="mt-5 font-semibold">What’s next</h2>
				<p class="mt-1 text-sm leading-6 text-muted-foreground">
					Fill the eight alliance stations, ready up, then let the host start the field.
				</p>
			</div>
		</section>
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
					value="8"
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
