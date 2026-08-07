<script lang="ts">
	import {
		IconActivity,
		IconArrowRight,
		IconBox,
		IconBrandTelegram,
		IconCpu,
		IconFlame,
		IconHash,
		IconLayersIntersect,
		IconPlayerPlay,
		IconPlus,
		IconRobot,
		IconSettings,
		IconShield,
		IconSparkles,
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
		void goto(normalizedMatchId === 'test-match' ? '/match/test-match' : `/match/${encodeURIComponent(normalizedMatchId)}/lobby`);
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

<div class="relative min-h-[calc(100vh-4rem)] overflow-hidden bg-background px-4 py-8 text-foreground sm:px-8 lg:px-12 lg:py-10 selection:bg-primary selection:text-primary-foreground">
	<!-- Futuristic ambient glow effects -->
	<div class="pointer-events-none absolute -top-40 -left-20 h-[36rem] w-[36rem] rounded-full bg-primary/20 blur-[120px]"></div>
	<div class="pointer-events-none absolute top-1/2 -right-40 h-[32rem] w-[32rem] rounded-full bg-cyan-500/15 blur-[120px]"></div>
	<div class="pointer-events-none absolute bottom-0 left-1/3 h-[24rem] w-[24rem] rounded-full bg-emerald-500/10 blur-[100px]"></div>

	<!-- Background grid pattern -->
	<div class="pointer-events-none absolute inset-0 bg-[linear-gradient(to_right,#ffffff08_1px,transparent_1px),linear-gradient(to_bottom,#ffffff08_1px,transparent_1px)] bg-[size:3rem_3rem] [mask-image:radial-gradient(ellipse_60%_50%_at_50%_0%,#000_70%,transparent_100%)]"></div>

	<main class="relative z-10 mx-auto flex w-full max-w-6xl flex-col gap-8">
		<!-- Hero Header & Telemetry Bar -->
		<section class="flex flex-col justify-between gap-6 md:flex-row md:items-end">
			<div class="max-w-2xl">
				<div class="inline-flex items-center gap-2 rounded-full border border-primary/40 bg-primary/10 px-3 py-1 font-mono text-xs text-primary shadow-[0_0_15px_rgba(234,88,12,0.2)]">
					<IconFlame class="size-3.5 text-primary animate-pulse" />
					<span class="font-semibold uppercase tracking-wider">FIRST Global Challenge 2026</span>
				</div>
				<h1 class="mt-3 font-daybreaker text-4xl tracking-wide sm:text-6xl text-transparent bg-clip-text bg-gradient-to-r from-white via-slate-100 to-slate-400">
					COMMAND CENTER
				</h1>
				<p class="mt-3 max-w-xl text-sm leading-relaxed text-muted-foreground sm:text-base">
					Deploy your robot into high-speed competition, tune drivetrains in the engineering hangar, or join multiplayer match lobbies in real-time.
				</p>
			</div>

			<div class="flex flex-wrap items-center gap-3">
				<div class="flex items-center gap-2.5 rounded-xl border border-emerald-500/30 bg-emerald-500/10 px-4 py-2.5 font-mono text-xs shadow-[0_0_15px_rgba(34,197,94,0.15)] backdrop-blur">
					<span class="relative flex size-2.5">
						<span class="absolute inline-flex size-full animate-ping rounded-full bg-emerald-400 opacity-75"></span>
						<span class="relative inline-flex size-2.5 rounded-full bg-emerald-500"></span>
					</span>
					<span class="font-semibold text-emerald-400">60 TPS · RAPIER3D ONLINE</span>
				</div>

				<button
					onclick={() => { errorMessage = ''; showCreateMatch = true; }}
					class="group flex items-center gap-2 rounded-xl border border-primary/40 bg-primary px-4 py-2.5 font-mono text-xs font-semibold text-primary-foreground shadow-[0_0_20px_rgba(234,88,12,0.3)] transition-all hover:bg-primary/90 hover:shadow-[0_0_30px_rgba(234,88,12,0.5)] active:scale-95 cursor-pointer"
				>
					<IconPlus class="size-4 transition-transform group-hover:rotate-90" />
					<span>HOST LOBBY</span>
				</button>
			</div>
		</section>

		{#if errorMessage}
			<div
				class="flex items-center gap-3 rounded-xl border border-destructive/40 bg-destructive/15 px-4 py-3 text-sm text-destructive font-mono backdrop-blur shadow-lg"
				role="alert"
			>
				<span class="size-2 rounded-full bg-destructive animate-ping"></span>
				<span>{errorMessage}</span>
			</div>
		{/if}

		<!-- Main Hero Card & Ruleset Badge -->
		<section class="grid gap-6 md:grid-cols-3">
			<div class="group relative overflow-hidden rounded-2xl border border-primary/40 bg-gradient-to-br from-card/90 via-card/60 to-background p-6 shadow-[0_0_40px_rgba(234,88,12,0.1)] backdrop-blur-xl md:col-span-2 transition-all hover:border-primary/70">
				<div class="pointer-events-none absolute -right-12 -bottom-12 size-64 rounded-full bg-primary/15 blur-2xl transition-all group-hover:bg-primary/25"></div>
				
				<div class="flex flex-col h-full justify-between gap-6 relative z-10">
					<div class="flex items-start justify-between gap-4">
						<div>
							<div class="flex items-center gap-2 text-xs font-mono font-semibold text-primary uppercase tracking-widest">
								<IconActivity class="size-4" /> Live Field Arena
							</div>
							<h2 class="mt-2 font-daybreaker text-2xl tracking-wide text-white sm:text-3xl">
								PRACTICE ARENA & TELEOP
							</h2>
							<p class="mt-2 max-w-lg text-sm text-muted-foreground leading-relaxed">
								Jump directly into the always-on arena field to test intake rollers, flywheel shooting trajectory, and mecanum drivetrain response.
							</p>
						</div>
						<span class="rounded-full border border-emerald-500/40 bg-emerald-500/10 px-3 py-1 font-mono text-xs font-semibold text-emerald-400 shadow-[0_0_10px_rgba(34,197,94,0.2)]">
							● LIVE
						</span>
					</div>

					<div class="flex flex-wrap items-center gap-4 pt-2">
						<Button
							href="/match/test-match"
							class="group/btn relative overflow-hidden font-mono text-xs font-bold uppercase tracking-wider px-6 py-5 shadow-[0_0_20px_rgba(234,88,12,0.3)] transition-all hover:shadow-[0_0_35px_rgba(234,88,12,0.6)]"
						>
							<span class="relative z-10 flex items-center gap-2">
								<IconPlayerPlay class="size-4 fill-current" /> DEPLOY TO TEST ARENA <IconArrowRight class="size-4 transition-transform group-hover/btn:translate-x-1" />
							</span>
						</Button>
						
						<div class="flex items-center gap-2 rounded-lg border border-border bg-black/40 px-3 py-2 font-mono text-xs text-muted-foreground">
							<IconHash class="size-3.5 text-primary" />
							<span>MATCH CODE: <code class="font-bold text-white">test-match</code></span>
						</div>
					</div>
				</div>
			</div>

			<!-- Game Pack Ruleset Spec -->
			<div class="flex flex-col justify-between rounded-2xl border border-border/80 bg-card/60 p-6 backdrop-blur-xl shadow-lg">
				<div>
					<div class="flex items-center justify-between">
						<span class="font-mono text-xs font-semibold text-muted-foreground uppercase tracking-wider">Active Ruleset</span>
						<span class="rounded border border-primary/30 bg-primary/10 px-2 py-0.5 font-mono text-[10px] text-primary">v1.0.0</span>
					</div>
					<div class="mt-4 flex items-center gap-4">
						<div class="flex size-12 items-center justify-center rounded-xl border border-primary/40 bg-primary/15 text-primary shadow-[0_0_15px_rgba(234,88,12,0.2)]">
							<IconBox class="size-6" />
						</div>
						<div>
							<h3 class="font-bold text-base text-white">Igniting Innovation</h3>
							<p class="font-mono text-xs text-muted-foreground">FGC 2026 Season Pack</p>
						</div>
					</div>
				</div>

				<div class="mt-6 border-t border-border/60 pt-4 font-mono text-xs text-muted-foreground space-y-1.5">
					<div class="flex justify-between">
						<span>Max Game Pieces:</span>
						<span class="font-semibold text-white">500 Wildfire Balls</span>
					</div>
					<div class="flex justify-between">
						<span>Intake / Shoot:</span>
						<span class="font-semibold text-emerald-400">Space / E Key</span>
					</div>
					<div class="flex justify-between">
						<span>Alliance Stations:</span>
						<span class="font-semibold text-white">8 Max Drivers</span>
					</div>
				</div>
			</div>
		</section>

		<!-- Match Access Hub: Connect by Match ID -->
		<section class="grid gap-6 lg:grid-cols-[1.2fr_0.8fr]">
			<div class="rounded-2xl border border-border/80 bg-card/70 p-6 backdrop-blur-xl shadow-md">
				<div class="flex items-center gap-2 font-mono text-xs font-semibold text-primary uppercase tracking-wider">
					<IconBrandTelegram class="size-4" /> Multiplayer Lobby Connect
				</div>
				<h3 class="mt-2 font-daybreaker text-xl tracking-wide text-white">ENTER MATCH CODE</h3>
				<p class="mt-1 text-xs text-muted-foreground leading-relaxed">
					Joining a team lobby? Enter the match ID provided by your alliance captain to enter the pre-match station.
				</p>

				<div class="mt-5 flex flex-col gap-3 sm:flex-row sm:items-end">
					<div class="flex-1">
						<Label for="match-id" class="font-mono text-xs text-muted-foreground">MATCH IDENTIFIER</Label>
						<div class="relative mt-1.5">
							<Input
								id="match-id"
								class="h-11 bg-background/80 font-mono text-sm tracking-wider pl-3 pr-20 border-border focus-visible:border-primary focus-visible:ring-primary/40"
								placeholder="e.g. test-match"
								bind:value={matchId}
								onkeydown={(event) => event.key === 'Enter' && joinMatch()}
							/>
							<span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 font-mono text-[10px] text-muted-foreground">
								[ENTER]
							</span>
						</div>
					</div>
					<Button onclick={joinMatch} class="h-11 font-mono text-xs font-bold uppercase tracking-wider sm:w-28 shadow-[0_0_15px_rgba(234,88,12,0.2)]">
						CONNECT
					</Button>
				</div>
			</div>

			<!-- Quick Lobby Creation Launcher -->
			<div class="flex flex-col justify-between rounded-2xl border border-border/80 bg-card/70 p-6 backdrop-blur-xl shadow-md">
				<div>
					<div class="flex items-center gap-2 font-mono text-xs font-semibold text-cyan-400 uppercase tracking-wider">
						<IconLayersIntersect class="size-4" /> Match Initialization
					</div>
					<h3 class="mt-2 font-daybreaker text-xl tracking-wide text-white">CREATE PRIVATE LOBBY</h3>
					<p class="mt-1 text-xs text-muted-foreground leading-relaxed">
						Host an official 8-player alliance match with custom practice seed and team alliance colors.
					</p>
				</div>

				<Button
					variant="secondary"
					class="mt-5 h-11 w-full font-mono text-xs font-bold uppercase tracking-wider border border-white/10 hover:border-primary/40"
					onclick={() => {
						errorMessage = '';
						showCreateMatch = true;
					}}
				>
					<IconPlus class="size-4" /> CREATE MATCH LOBBY
				</Button>
			</div>
		</section>

		<!-- Esports Game Modes Matrix -->
		<section class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
			<!-- Robot Hangar Card -->
			<a
				href="/robot"
				class="group relative overflow-hidden rounded-2xl border border-border/80 bg-card/40 p-6 backdrop-blur-xl transition-all duration-300 hover:border-primary/60 hover:bg-card/80 hover:shadow-[0_0_25px_rgba(234,88,12,0.15)] hover:-translate-y-1"
			>
				<div class="flex items-center justify-between">
					<span class="flex size-11 items-center justify-center rounded-xl border border-primary/30 bg-primary/10 text-primary shadow-[0_0_10px_rgba(234,88,12,0.2)]">
						<IconRobot class="size-6" />
					</span>
					<IconArrowRight class="size-5 text-muted-foreground transition-transform group-hover:translate-x-1 group-hover:text-primary" />
				</div>
				<h3 class="mt-5 font-daybreaker text-lg tracking-wide text-white">ROBOT HANGAR</h3>
				<p class="mt-1.5 text-xs text-muted-foreground leading-relaxed">
					Configure Mecanum, Swerve, or Tank drive options. Adjust intake roller friction & flywheel exit speeds.
				</p>
				<div class="mt-4 inline-flex items-center gap-1 font-mono text-[11px] text-primary">
					<span>BUILD & SPEC</span> →
				</div>
			</a>

			<!-- Offline Field Sandbox Card -->
			<a
				href="/scene"
				class="group relative overflow-hidden rounded-2xl border border-border/80 bg-card/40 p-6 backdrop-blur-xl transition-all duration-300 hover:border-cyan-500/60 hover:bg-card/80 hover:shadow-[0_0_25px_rgba(6,182,212,0.15)] hover:-translate-y-1"
			>
				<div class="flex items-center justify-between">
					<span class="flex size-11 items-center justify-center rounded-xl border border-cyan-500/30 bg-cyan-500/10 text-cyan-400 shadow-[0_0_10px_rgba(6,182,212,0.2)]">
						<IconWorld class="size-6" />
					</span>
					<IconArrowRight class="size-5 text-muted-foreground transition-transform group-hover:translate-x-1 group-hover:text-cyan-400" />
				</div>
				<h3 class="mt-5 font-daybreaker text-lg tracking-wide text-white">OFFLINE SANDBOX</h3>
				<p class="mt-1.5 text-xs text-muted-foreground leading-relaxed">
					Free-roam single player field environment. Practice robot maneuvering and ball manipulation without network latency.
				</p>
				<div class="mt-4 inline-flex items-center gap-1 font-mono text-[11px] text-cyan-400">
					<span>ENTER SANDBOX</span> →
				</div>
			</a>

			<!-- Field Controls Quick Reference Card -->
			<div class="rounded-2xl border border-border/60 bg-black/40 p-6 backdrop-blur-xl">
				<div class="flex size-11 items-center justify-center rounded-xl border border-white/10 bg-white/5 text-slate-300">
					<IconSettings class="size-6" />
				</div>
				<h3 class="mt-5 font-daybreaker text-lg tracking-wide text-white">KEYBOARD & GAMEPAD</h3>
				<div class="mt-3 font-mono text-xs text-muted-foreground space-y-1.5">
					<div class="flex justify-between">
						<span class="text-slate-400">Drive / Turn:</span>
						<span class="text-white font-semibold">W/A/S/D or Left Stick</span>
					</div>
					<div class="flex justify-between">
						<span class="text-slate-400">Intake Roller:</span>
						<span class="text-cyan-400 font-semibold">SPACEBAR / LT</span>
					</div>
					<div class="flex justify-between">
						<span class="text-slate-400">Flywheel Shoot:</span>
						<span class="text-lime-400 font-semibold">E KEY / RT</span>
					</div>
					<div class="flex justify-between">
						<span class="text-slate-400">Debug Panel:</span>
						<span class="text-amber-400 font-semibold">B KEY</span>
					</div>
				</div>
			</div>
		</section>
	</main>
</div>

<!-- High-Tech Match Creation Dialog -->
<Dialog.Root bind:open={showCreateMatch}>
	<Dialog.Content class="sm:max-w-[440px] border-primary/30 bg-card/95 backdrop-blur-2xl shadow-[0_0_50px_rgba(0,0,0,0.8)]">
		<Dialog.Header>
			<Dialog.Title class="font-daybreaker text-2xl tracking-wide text-white flex items-center gap-2">
				<IconSparkles class="size-5 text-primary" /> CREATE MATCH LOBBY
			</Dialog.Title>
			<Dialog.Description class="font-mono text-xs text-muted-foreground">
				Initialize a new FGC 2026 multiplayer competition instance.
			</Dialog.Description>
		</Dialog.Header>

		<div class="flex flex-col gap-4 py-4 font-mono text-xs">
			<div class="flex flex-col gap-2">
				<Label for="game-pack" class="text-muted-foreground">GAME PACK RULESET</Label>
				<Input
					id="game-pack"
					value="Igniting Innovation (FGC 2026)"
					disabled
					class="bg-background/80 text-white font-semibold"
				/>
			</div>
			<div class="flex flex-col gap-2">
				<Label for="max-players" class="text-muted-foreground">MAXIMUM ALLIANCE DRIVERS</Label>
				<Input
					id="max-players"
					type="number"
					value="8"
					disabled
					class="bg-background/80 text-white font-semibold"
				/>
			</div>
			<div class="rounded-xl border border-primary/20 bg-primary/5 p-3 text-[11px] text-muted-foreground leading-relaxed">
				⚡ <strong class="text-primary">XPBD Physics Engine</strong>: 500 Wildfire ball rigid-body solver will run at 60Hz authoritative tickrate.
			</div>
		</div>

		<Dialog.Footer class="gap-2">
			<Button variant="outline" onclick={() => (showCreateMatch = false)} disabled={isCreating} class="font-mono text-xs">
				Cancel
			</Button>
			<Button onclick={submitCreateMatch} disabled={isCreating} class="font-mono text-xs font-bold uppercase shadow-[0_0_15px_rgba(234,88,12,0.3)]">
				{isCreating ? 'INITIALIZING…' : 'INITIALIZE LOBBY'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
