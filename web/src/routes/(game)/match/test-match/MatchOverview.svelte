<script lang="ts">
	import { fade } from 'svelte/transition';
	import { Button } from '$lib/components/ui/button';

	let {
		matchId,
		redScore,
		blueScore,
		globalScore,
		redRoster = [],
		blueRoster = []
	}: {
		matchId: string;
		redScore: number;
		blueScore: number;
		globalScore?: number;
		redRoster?: { name: string }[];
		blueRoster?: { name: string }[];
	} = $props();

	const outcome = $derived(
		redScore === blueScore
			? 'Match tied'
			: redScore > blueScore
				? 'Red alliance wins'
				: 'Blue alliance wins'
	);
	const fadeDuration =
		typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches
			? 0
			: 500;
</script>

<section
	in:fade={{ duration: fadeDuration }}
	class="fixed inset-0 z-50 grid min-h-dvh place-items-center overflow-auto bg-slate-950 px-5 py-10 text-white"
>
	<div
		class="absolute inset-x-0 top-0 h-2 bg-gradient-to-r from-red-500 via-white/70 to-blue-500"
	></div>
	<div class="relative w-full max-w-5xl text-center">
		<p class="text-sm font-semibold tracking-[0.28em] text-white/45 uppercase">
			Final result · Match {matchId.slice(0, 8)}
		</p>
		<h1 class="mt-4 font-daybreaker text-4xl tracking-wide sm:text-6xl">{outcome}</h1>

		<div
			class="mt-10 grid overflow-hidden rounded-3xl border border-white/10 bg-white/5 shadow-2xl shadow-black/50 sm:grid-cols-[1fr_auto_1fr]"
		>
			<div class="bg-red-500/15 px-6 py-10 sm:px-10">
				<p class="text-sm font-bold tracking-[0.2em] text-red-200 uppercase">Red</p>
				<p class="mt-2 text-7xl font-black text-red-300 tabular-nums sm:text-9xl">{redScore}</p>
				<p class="mt-6 text-sm text-white/55">
					{redRoster.map((player) => player.name).join(' · ') || 'No drivers'}
				</p>
			</div>
			<div
				class="grid place-items-center border-y border-white/10 bg-black/25 px-7 py-4 sm:border-x sm:border-y-0"
			>
				<span class="text-sm font-bold tracking-[0.3em] text-white/40 uppercase">Final</span>
				{#if globalScore > 0}<span class="mt-2 text-xs text-lime-200">EXT {globalScore}</span>{/if}
			</div>
			<div class="bg-blue-500/15 px-6 py-10 sm:px-10">
				<p class="text-sm font-bold tracking-[0.2em] text-blue-200 uppercase">Blue</p>
				<p class="mt-2 text-7xl font-black text-blue-300 tabular-nums sm:text-9xl">{blueScore}</p>
				<p class="mt-6 text-sm text-white/55">
					{blueRoster.map((player) => player.name).join(' · ') || 'No drivers'}
				</p>
			</div>
		</div>

		<div class="mt-8">
			<Button href="/dashboard" size="lg" class="bg-white px-7 text-slate-950 hover:bg-white/90"
				>Back to dashboard</Button
			>
		</div>
	</div>
</section>
