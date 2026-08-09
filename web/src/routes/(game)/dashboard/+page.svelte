<script lang="ts">
	import {
		IconPlayerPlay,
		IconPlus,
		IconRobot,
		IconLogin
	} from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { ApiError, api } from '$lib/api';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';

	let matchId = $state('');
	let showCreateMatch = $state(false);
	let showJoinMatch = $state(false);
	let isCreating = $state(false);
	let errorMessage = $state('');

	function joinMatch() {
		const normalizedMatchId = matchId.trim();
		if (!normalizedMatchId) return;
		void goto(resolve(normalizedMatchId === 'test-match' ? '/match/test-match' : `/match/${encodeURIComponent(normalizedMatchId)}/lobby`));
	}

	async function submitCreateMatch() {
		isCreating = true;
		errorMessage = '';
		try {
			const { match_id } = await api.createMatch({ gamePackId: 'fgc-2026' });
			showCreateMatch = false;
			await goto(resolve(`/match/${match_id}/lobby`));
		} catch (error) {
			errorMessage =
				error instanceof ApiError ? error.message : 'Unable to create a match. Please try again.';
		} finally {
			isCreating = false;
		}
	}
</script>

<div class="relative flex min-h-[calc(100vh-4rem)] flex-col items-center justify-center overflow-hidden bg-background px-4 py-8 text-foreground selection:bg-primary selection:text-primary-foreground">
	<!-- Cinematic Game Glow Effects -->
	<div class="pointer-events-none absolute left-1/2 top-1/2 h-[40rem] w-[40rem] -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary/20 blur-[150px]"></div>
	
	<!-- Background grid pattern -->
	<div class="pointer-events-none absolute inset-0 bg-[linear-gradient(to_right,#ffffff05_1px,transparent_1px),linear-gradient(to_bottom,#ffffff05_1px,transparent_1px)] bg-[size:4rem_4rem] [mask-image:radial-gradient(ellipse_80%_80%_at_50%_50%,#000_20%,transparent_100%)]"></div>

	<main class="relative z-10 flex w-full max-w-sm flex-col items-center gap-10">
		<!-- Main Title -->
		<div class="text-center">
			<h1 class="font-daybreaker text-5xl tracking-widest text-transparent bg-clip-text bg-gradient-to-b from-white to-white/60 drop-shadow-[0_0_15px_rgba(255,255,255,0.3)] sm:text-6xl">
				FGSimulator
			</h1>
		</div>

		{#if errorMessage}
			<div class="w-full rounded-lg border border-destructive/50 bg-destructive/20 p-3 text-center font-mono text-sm text-destructive backdrop-blur-md animate-in fade-in zoom-in duration-300">
				{errorMessage}
			</div>
		{/if}

		<!-- Centered Button Menu -->
		<div class="flex w-full flex-col gap-4">
			<Button
				href={resolve('/match/test-match')}
				class="group relative flex h-14 w-full items-center justify-start overflow-hidden rounded-xl border border-primary/50 bg-primary/10 px-6 font-daybreaker text-xl tracking-wider text-white transition-all duration-300 hover:border-primary hover:bg-primary/20 hover:shadow-[0_0_30px_rgba(234,88,12,0.4)] hover:scale-[1.02]"
			>
				<span class="absolute inset-0 w-full translate-x-[-100%] bg-gradient-to-r from-transparent via-primary/20 to-transparent transition-transform duration-500 group-hover:translate-x-[100%]"></span>
				<div class="flex items-center gap-4 relative z-10">
					<IconPlayerPlay class="size-6 text-primary transition-transform group-hover:scale-110" />
					PLAY SOLO
				</div>
			</Button>

			<Button
				variant="ghost"
				onclick={() => { showJoinMatch = true; }}
				class="group relative flex h-14 w-full items-center justify-start overflow-hidden rounded-xl border border-cyan-500/30 bg-cyan-500/5 px-6 font-daybreaker text-xl tracking-wider text-white transition-all duration-300 hover:border-cyan-400 hover:bg-cyan-500/15 hover:shadow-[0_0_30px_rgba(6,182,212,0.3)] hover:scale-[1.02]"
			>
				<span class="absolute inset-0 w-full translate-x-[-100%] bg-gradient-to-r from-transparent via-cyan-500/10 to-transparent transition-transform duration-500 group-hover:translate-x-[100%]"></span>
				<div class="flex items-center gap-4 relative z-10">
					<IconLogin class="size-6 text-cyan-400 transition-transform group-hover:scale-110" />
					JOIN MATCH
				</div>
			</Button>

			<Button
				variant="ghost"
				onclick={() => { errorMessage = ''; showCreateMatch = true; }}
				class="group relative flex h-14 w-full items-center justify-start overflow-hidden rounded-xl border border-emerald-500/30 bg-emerald-500/5 px-6 font-daybreaker text-xl tracking-wider text-white transition-all duration-300 hover:border-emerald-400 hover:bg-emerald-500/15 hover:shadow-[0_0_30px_rgba(16,185,129,0.3)] hover:scale-[1.02]"
			>
				<span class="absolute inset-0 w-full translate-x-[-100%] bg-gradient-to-r from-transparent via-emerald-500/10 to-transparent transition-transform duration-500 group-hover:translate-x-[100%]"></span>
				<div class="flex items-center gap-4 relative z-10">
					<IconPlus class="size-6 text-emerald-400 transition-transform group-hover:scale-110" />
					CREATE MATCH
				</div>
			</Button>

			<Button
				variant="ghost"
				href={resolve('/robot')}
				class="group relative flex h-14 w-full items-center justify-start overflow-hidden rounded-xl border border-white/10 bg-white/5 px-6 font-daybreaker text-xl tracking-wider text-white transition-all duration-300 hover:border-white/30 hover:bg-white/10 hover:shadow-[0_0_30px_rgba(255,255,255,0.1)] hover:scale-[1.02]"
			>
				<span class="absolute inset-0 w-full translate-x-[-100%] bg-gradient-to-r from-transparent via-white/5 to-transparent transition-transform duration-500 group-hover:translate-x-[100%]"></span>
				<div class="flex items-center gap-4 relative z-10">
					<IconRobot class="size-6 text-slate-300 transition-transform group-hover:scale-110" />
					ROBOT GARAGE
				</div>
			</Button>
		</div>
	</main>
</div>

<!-- Join Match Dialog -->
<Dialog.Root bind:open={showJoinMatch}>
	<Dialog.Content class="sm:max-w-[400px] border-cyan-500/30 bg-card/95 backdrop-blur-2xl shadow-[0_0_50px_rgba(6,182,212,0.2)]">
		<Dialog.Header>
			<Dialog.Title class="font-daybreaker text-2xl tracking-wide text-white flex items-center gap-2">
				<IconLogin class="size-6 text-cyan-400" /> JOIN MATCH
			</Dialog.Title>
			<Dialog.Description class="font-mono text-xs text-muted-foreground">
				Enter the match ID to join a multiplayer lobby.
			</Dialog.Description>
		</Dialog.Header>

		<div class="py-6">
			<Label for="match-id-dialog" class="font-mono text-xs text-muted-foreground">MATCH ID</Label>
			<Input
				id="match-id-dialog"
				class="mt-2 h-12 bg-background/80 font-mono text-lg tracking-widest text-center border-border focus-visible:border-cyan-400 focus-visible:ring-cyan-400/40 uppercase"
				placeholder="ENTER CODE"
				bind:value={matchId}
				onkeydown={(event) => {
					if (event.key === 'Enter') {
						showJoinMatch = false;
						joinMatch();
					}
				}}
			/>
		</div>

		<Dialog.Footer>
			<Button variant="outline" onclick={() => (showJoinMatch = false)} class="font-mono text-xs h-10 w-full sm:w-auto">
				CANCEL
			</Button>
			<Button onclick={() => { showJoinMatch = false; joinMatch(); }} class="font-mono text-xs font-bold uppercase h-10 bg-cyan-600 hover:bg-cyan-500 text-white shadow-[0_0_15px_rgba(6,182,212,0.3)] w-full sm:w-auto mt-2 sm:mt-0">
				CONNECT
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<!-- Create Match Dialog -->
<Dialog.Root bind:open={showCreateMatch}>
	<Dialog.Content class="sm:max-w-[400px] border-emerald-500/30 bg-card/95 backdrop-blur-2xl shadow-[0_0_50px_rgba(16,185,129,0.2)]">
		<Dialog.Header>
			<Dialog.Title class="font-daybreaker text-2xl tracking-wide text-white flex items-center gap-2">
				<IconPlus class="size-6 text-emerald-400" /> CREATE MATCH
			</Dialog.Title>
			<Dialog.Description class="font-mono text-xs text-muted-foreground">
				Initialize a new multiplayer competition server.
			</Dialog.Description>
		</Dialog.Header>

		<div class="flex flex-col gap-5 py-6 font-mono text-xs">
			<div class="flex flex-col gap-2">
				<Label class="text-muted-foreground">RULESET</Label>
				<div class="rounded-md bg-background/80 px-3 py-3 text-white font-semibold border border-white/10">
					Igniting Innovation (FGC 2026)
				</div>
			</div>
			<div class="flex flex-col gap-2">
				<Label class="text-muted-foreground">MAX DRIVERS</Label>
				<div class="rounded-md bg-background/80 px-3 py-3 text-white font-semibold border border-white/10">
					8 Players
				</div>
			</div>
		</div>

		<Dialog.Footer>
			<Button variant="outline" onclick={() => (showCreateMatch = false)} disabled={isCreating} class="font-mono text-xs h-10 w-full sm:w-auto">
				CANCEL
			</Button>
			<Button onclick={submitCreateMatch} disabled={isCreating} class="font-mono text-xs font-bold uppercase h-10 bg-emerald-600 hover:bg-emerald-500 text-white shadow-[0_0_15px_rgba(16,185,129,0.3)] w-full sm:w-auto mt-2 sm:mt-0">
				{isCreating ? 'STARTING...' : 'START SERVER'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
