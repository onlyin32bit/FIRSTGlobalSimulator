<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { Button } from '$lib/components/ui/button';
	import AppShell from '$lib/components/app/AppShell.svelte';
	import LobbyField from '$lib/features/lobby/LobbyField.svelte';
	import { LobbyController } from '$lib/features/lobby/lobby-controller.svelte';

	const controller = new LobbyController();
	const matchId = $derived(page.params.matchId ?? '');
	const restrictedAlliance = $derived(page.url.searchParams.get('alliance'));

	onMount(() => {
		void controller.load(matchId);
		const poller = window.setInterval(() => controller.refresh(matchId), 5_000);
		return () => {
			window.clearInterval(poller);
			controller.destroy();
		};
	});
</script>

<AppShell>
	<section>
		<p class="text-sm font-medium text-primary">Match lobby</p>
		<h1 class="mt-2 font-daybreaker text-4xl tracking-wide">Set up your match</h1>
		<p class="mt-3 text-muted-foreground">
			Match ID: <code class="rounded bg-muted px-1.5 py-1 text-xs">{matchId}</code>
		</p>

		{#if controller.error}<p class="mt-5 text-sm text-destructive" role="alert">
				{controller.error}
			</p>{/if}

		{#if !controller.lobby}
			<p class="mt-10 text-muted-foreground">Loading lobby…</p>
		{:else}
			<div class="mt-8 grid gap-5 lg:grid-cols-[minmax(0,1fr)_20rem]">
				{#if controller.field}
					<LobbyField
						field={controller.field}
						lobby={controller.lobby}
						userId={controller.userId}
						{restrictedAlliance}
						working={controller.working}
						onClaim={(slotId) => controller.claim(matchId, slotId)}
					/>
				{:else}
					<p class="text-muted-foreground">Field layout is unavailable.</p>
				{/if}

				<aside class="space-y-6">
					<section class="rounded-xl bg-card p-5 shadow-sm">
						<h2 class="font-semibold">Your station</h2>
						{#if !controller.mySlot}
							<label class="mt-4 block text-sm text-muted-foreground" for="robot-select"
								>Robot</label
							>
							<select
								id="robot-select"
								class="mt-2 w-full rounded-lg bg-muted px-3 py-2 text-sm"
								bind:value={controller.selectedRobotId}
							>
								<option value="pack:starter-bot">Starter Bot</option>
								{#each controller.robots as robot (robot.id)}
									<option value={robot.id}>{robot.name}</option>
								{/each}
							</select>
						{/if}
						{#if controller.mySlot}
							<p class="mt-2 text-sm text-muted-foreground">
								{controller.mySlot.label} · {controller.mySlot.occupant?.ready
									? 'Ready'
									: 'Not ready'}
							</p>
							<Button
								class="mt-5 w-full"
								onclick={() => controller.setReady(matchId)}
								disabled={controller.working !== null}
							>
								{controller.mySlot.occupant?.ready ? 'Mark not ready' : 'Mark ready'}
							</Button>
							<Button
								variant="ghost"
								class="mt-2 w-full"
								onclick={() => controller.leave(matchId)}
								disabled={controller.working !== null}>Leave station</Button
							>
						{:else}
							<p class="mt-2 text-sm text-muted-foreground">
								Choose an available station to join the match.
							</p>
						{/if}
					</section>

					{#if controller.isHost}
						<section class="rounded-xl bg-card p-5 shadow-sm">
							<h2 class="font-semibold">Start match</h2>
							<p class="mt-2 text-sm text-muted-foreground">
								One ready player from each alliance is required.
							</p>
							<Button
								class="mt-5 w-full"
								disabled={!controller.canStart || controller.working !== null}
								onclick={() => controller.start(matchId)}>Start match</Button
							>
						</section>
					{/if}

					{#if controller.isAdmin && controller.lobby.status === 'LOBBY'}
						<Button
							variant="ghost"
							class="w-full text-muted-foreground"
							disabled={controller.working !== null}
							onclick={() => controller.start(matchId, true)}>Admin: start now</Button
						>
					{/if}
				</aside>
			</div>
		{/if}
	</section>
</AppShell>
