<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import AppShell from '$lib/components/app/AppShell.svelte';
	import RobotOptionPicker from '$lib/features/robot/RobotOptionPicker.svelte';
	import SavedRobotList from '$lib/features/robot/SavedRobotList.svelte';
	import { RobotBuilder, robotOptions } from '$lib/features/robot/robot-builder.svelte';

	const builder = new RobotBuilder();
	onMount(() => builder.load());
</script>

<AppShell>
	<div class="grid gap-10 lg:grid-cols-[minmax(0,1fr)_22rem]">
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

		<aside class="space-y-6 lg:pt-8">
			<section class="rounded-xl bg-card p-5 shadow-sm">
				<h2 class="font-semibold">Current build</h2>
				<p class="mt-3 text-sm leading-6 text-muted-foreground">{builder.summary}</p>
			</section>
			<SavedRobotList robots={builder.robots} isLoading={builder.isLoading} />
		</aside>
	</div>
</AppShell>
