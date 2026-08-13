<script lang="ts">
	import { onMount } from 'svelte';
	import { Canvas, T } from '@threlte/core';
	import { Grid, OrbitControls } from '@threlte/extras';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import AppShell from '$lib/components/app/AppShell.svelte';
	import RobotOptionPicker from '$lib/features/robot/RobotOptionPicker.svelte';
	import SavedRobotList from '$lib/features/robot/SavedRobotList.svelte';
	import RobotPreview from '$lib/features/robot/RobotPreview.svelte';
	import { RobotBuilder, robotOptions } from '$lib/features/robot/robot-builder.svelte';
	import { api, ApiError } from '$lib/api';

	const builder = new RobotBuilder();
	let starterBot = $state<{ name: string; visual: string } | null>(null);
	let starterBotError = $state('');

	onMount(async () => {
		void builder.load();
		try {
			const assets = await api.getGamePackRobotAssets('fgc-2026', 'starter-bot');
			starterBot = { name: assets.name, visual: assets.visual };
		} catch (error) {
			starterBotError =
				error instanceof ApiError ? error.message : 'The Starter Bot preview is unavailable.';
		}
	});
</script>

<AppShell>
	<div class="grid gap-10 xl:grid-cols-[minmax(0,1fr)_minmax(20rem,0.75fr)]">
		<section>
			<p class="text-sm font-medium text-primary">Robot builder</p>
			<h1 class="mt-2 font-daybreaker text-4xl tracking-wide">Build a robot</h1>
			<p class="mt-3 max-w-xl text-muted-foreground">
				Choose a drivetrain and mechanisms. This build is available when you take a driver station.
			</p>

			<div class="mt-8 max-w-2xl space-y-8">
				<div>
					<Label for="robot-name">Robot name</Label>
					<Input id="robot-name" class="mt-2 max-w-md" bind:value={builder.name} />
				</div>
				<RobotOptionPicker
					label="Drivetrain"
					options={robotOptions.driveType}
					bind:value={builder.driveType}
					description="How the chassis moves around the field."
				/>
				<RobotOptionPicker
					label="Intake"
					options={robotOptions.intake}
					bind:value={builder.intake}
					description="How the robot collects Wildfire balls."
				/>
				<RobotOptionPicker
					label="Scorer"
					options={robotOptions.shooter}
					bind:value={builder.shooter}
					description="How the robot returns balls to the field."
				/>
			</div>

			<div class="mt-8 flex items-center gap-4">
				<Button onclick={() => builder.save()} disabled={builder.isSaving}
					>{builder.isSaving ? 'Saving…' : 'Save build'}</Button
				>
				{#if builder.message}<p class="text-sm text-muted-foreground" role="status">
						{builder.message}
					</p>{/if}
			</div>
		</section>

		<aside class="space-y-6 xl:pt-8">
			<section class="overflow-hidden rounded-2xl border bg-card shadow-sm">
				<div class="border-b px-5 py-4">
					<h2 class="font-semibold">Starter Bot</h2>
					<p class="mt-1 text-sm text-muted-foreground">
						Reference model for the current game pack.
					</p>
				</div>
				<div
					class="h-80 bg-[radial-gradient(circle_at_50%_20%,hsl(var(--muted)),hsl(var(--card))_68%)]"
				>
					{#if starterBot}
						<Canvas dpr={[0.75, 1.25]} shadows>
							<T.PerspectiveCamera makeDefault position={[1.55, 1.15, 1.8]} fov={42}>
								<OrbitControls
									target={[0, 0.28, 0]}
									enablePan={false}
									minDistance={0.8}
									maxDistance={3.5}
								/>
							</T.PerspectiveCamera>
							<T.HemisphereLight args={['#dcecff', '#1e293b', 1.6]} />
							<T.DirectionalLight
								position={[2, 3, 2]}
								intensity={2.4}
								castShadow
								shadow-mapSize={[1024, 1024]}
							/>
							<Grid
								position={[0, 0.001, 0]}
								cellColor="#94a3b8"
								sectionColor="#64748b"
								cellSize={0.25}
								sectionSize={1}
								fadeDistance={4}
							/>
							<RobotPreview assetUrl={starterBot.visual} />
						</Canvas>
					{:else if starterBotError}
						<div
							class="grid h-full place-items-center px-6 text-center text-sm text-muted-foreground"
						>
							{starterBotError}
						</div>
					{:else}
						<div class="grid h-full place-items-center text-sm text-muted-foreground">
							Loading reference model…
						</div>
					{/if}
				</div>
				<div class="px-5 py-3 text-sm text-muted-foreground">
					Orbit to inspect the modeled intake, transfer, and outtake assemblies.
				</div>
			</section>
			<section class="rounded-xl bg-card p-5 shadow-sm">
				<h2 class="font-semibold">Current build</h2>
				<p class="mt-3 text-sm leading-6 text-muted-foreground">{builder.summary}</p>
			</section>
			<SavedRobotList robots={builder.robots} isLoading={builder.isLoading} />
		</aside>
	</div>
</AppShell>
