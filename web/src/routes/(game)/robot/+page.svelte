<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import * as Tabs from '$lib/components/ui/tabs';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { api } from '$lib/api';

	// State for the robot builder
	let robotName = $state('My Custom Bot');
	let driveType = $state('Mecanum');
	let selectedIntake = $state('Roller Claw');
	let selectedShooter = $state('Flywheel');
	let isSaving = $state(false);

	const driveTypes = ['Mecanum', 'Tank', 'Swerve', 'H-Drive'];
	const intakes = ['Roller Claw', 'Over-the-bumper', 'None'];
	const shooters = ['Flywheel', 'Catapult', 'Puncher', 'None'];

	async function saveRobot() {
		isSaving = true;
		try {
			const { robot_id } = await api.createRobot({
				name: robotName,
				buildData: { driveType, selectedIntake, selectedShooter }
			});
			console.log('Saved robot:', robot_id);
		} catch (e) {
			console.error('Network error saving robot:', e);
		} finally {
			isSaving = false;
		}
	}
</script>

<div class="flex h-screen w-full overflow-hidden bg-background text-foreground">
	<!-- Sidebar for Configuration -->
	<aside class="flex w-80 flex-col gap-6 overflow-y-auto border-r border-border bg-card/30 p-6">
		<div>
			<h2 class="text-2xl font-bold tracking-tight text-primary">Robot Builder</h2>
			<p class="mt-1 text-sm text-muted-foreground">Configure your FGC 2026 robot.</p>
		</div>

		<div class="flex flex-col gap-2">
			<Label for="robot-name">Robot Name</Label>
			<Input id="robot-name" bind:value={robotName} class="bg-input/50" />
		</div>

		<Tabs.Root value="drive" class="w-full">
			<Tabs.List class="grid w-full grid-cols-3">
				<Tabs.Trigger value="drive">Drive</Tabs.Trigger>
				<Tabs.Trigger value="intake">Intake</Tabs.Trigger>
				<Tabs.Trigger value="shooter">Scoring</Tabs.Trigger>
			</Tabs.List>

			<Tabs.Content value="drive" class="flex flex-col gap-3 pt-4">
				{#each driveTypes as dt}
					<button
						class="w-full rounded-lg border p-3 text-left transition-colors {driveType === dt
							? 'border-primary bg-primary/10 text-primary'
							: 'border-border bg-card hover:bg-accent'}"
						onclick={() => (driveType = dt)}
					>
						<span class="font-semibold">{dt}</span>
						{#if driveType === dt}
							<span class="float-right font-bold text-primary">✓</span>
						{/if}
					</button>
				{/each}
			</Tabs.Content>

			<Tabs.Content value="intake" class="flex flex-col gap-3 pt-4">
				{#each intakes as intake}
					<button
						class="w-full rounded-lg border p-3 text-left transition-colors {selectedIntake ===
						intake
							? 'border-primary bg-primary/10 text-primary'
							: 'border-border bg-card hover:bg-accent'}"
						onclick={() => (selectedIntake = intake)}
					>
						<span class="font-semibold">{intake}</span>
						{#if selectedIntake === intake}
							<span class="float-right font-bold text-primary">✓</span>
						{/if}
					</button>
				{/each}
			</Tabs.Content>

			<Tabs.Content value="shooter" class="flex flex-col gap-3 pt-4">
				{#each shooters as shooter}
					<button
						class="w-full rounded-lg border p-3 text-left transition-colors {selectedShooter ===
						shooter
							? 'border-primary bg-primary/10 text-primary'
							: 'border-border bg-card hover:bg-accent'}"
						onclick={() => (selectedShooter = shooter)}
					>
						<span class="font-semibold">{shooter}</span>
						{#if selectedShooter === shooter}
							<span class="float-right font-bold text-primary">✓</span>
						{/if}
					</button>
				{/each}
			</Tabs.Content>
		</Tabs.Root>

		<div class="mt-auto pt-6">
			<Button class="w-full" disabled={isSaving} onclick={saveRobot}>
				{isSaving ? 'Saving...' : 'Save Build'}
			</Button>
			<Button variant="ghost" class="mt-2 w-full" href="/">Back to Home</Button>
		</div>
	</aside>

	<!-- Main Stage for 3D Preview (Placeholder) -->
	<main class="relative flex flex-1 flex-col items-center justify-center bg-background/50">
		<div
			class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_center,var(--color-primary)_0,transparent_100%)] opacity-5"
		></div>

		<div class="z-10 flex flex-col items-center gap-4 text-center">
			<!-- 3D Canvas will go here eventually. Mocking it for now. -->
			<div
				class="flex h-96 w-96 items-center justify-center rounded-xl border-4 border-dashed border-border bg-card/20 shadow-2xl backdrop-blur-sm"
			>
				<p class="font-mono text-sm text-muted-foreground">
					[ 3D Viewport ]<br />{driveType} chassis with {selectedIntake}
				</p>
			</div>

			<Card.Root class="w-full max-w-md border-border bg-card/50 backdrop-blur">
				<Card.Header class="pb-3">
					<Card.Title>Specs Overview</Card.Title>
				</Card.Header>
				<Card.Content class="grid grid-cols-2 gap-4 text-sm">
					<div class="flex flex-col">
						<span class="text-muted-foreground">Est. Mass</span>
						<span class="font-semibold">32.4 kg</span>
					</div>
					<div class="flex flex-col">
						<span class="text-muted-foreground">Max Velocity</span>
						<span class="font-semibold">{driveType === 'Swerve' ? '4.5' : '3.8'} m/s</span>
					</div>
					<div class="flex flex-col">
						<span class="text-muted-foreground">Battery Drain</span>
						<span class="font-semibold">High</span>
					</div>
					<div class="flex flex-col">
						<span class="text-muted-foreground">Modules</span>
						<span class="font-semibold text-primary">3 Active</span>
					</div>
				</Card.Content>
			</Card.Root>
		</div>
	</main>
</div>
