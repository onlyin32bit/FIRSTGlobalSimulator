<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import { Input } from '$lib/components/ui/input';
	import * as Tabs from '$lib/components/ui/tabs';
	import { Label } from '$lib/components/ui/label';
	import { ApiError, api, type Robot } from '$lib/api';

	let robotName = $state('My Custom Bot');
	let driveType = $state('Mecanum');
	let selectedIntake = $state('Roller Claw');
	let selectedShooter = $state('Flywheel');
	let isSaving = $state(false);
	let isLoading = $state(true);
	let message = $state('');
	let savedRobots = $state<Robot[]>([]);

	const driveTypes = ['Mecanum', 'Tank', 'Swerve', 'H-Drive'];
	const intakes = ['Roller Claw', 'Over-the-bumper', 'None'];
	const shooters = ['Flywheel', 'Catapult', 'Puncher', 'None'];

	async function loadRobots() {
		isLoading = true;
		try {
			savedRobots = (await api.listRobots()).robots;
		} catch (error) {
			message = error instanceof ApiError ? error.message : 'Unable to load saved robots.';
		} finally {
			isLoading = false;
		}
	}

	onMount(loadRobots);

	async function saveRobot() {
		isSaving = true;
		message = '';
		try {
			const { robot } = await api.createRobot({
				name: robotName,
				buildData: { driveType, selectedIntake, selectedShooter }
			});
			savedRobots = [robot, ...savedRobots];
			message = `${robot.name} was saved.`;
		} catch (error) {
			message = error instanceof ApiError ? error.message : 'Unable to save your robot.';
		} finally {
			isSaving = false;
		}
	}
</script>

<div class="flex min-h-[calc(100vh-3.5rem)] w-full overflow-hidden bg-background text-foreground">
	<aside class="flex w-80 flex-col gap-6 overflow-y-auto border-r border-border bg-card/30 p-6">
		<div>
			<h2 class="text-2xl font-bold tracking-tight text-primary">Robot Builder</h2>
			<p class="mt-1 text-sm text-muted-foreground">Configure your FGC 2026 robot.</p>
		</div>
		<div class="flex flex-col gap-2">
			<Label for="robot-name">Robot name</Label><Input
				id="robot-name"
				bind:value={robotName}
				class="bg-input/50"
			/>
		</div>
		<Tabs.Root value="drive" class="w-full">
			<Tabs.List class="grid w-full grid-cols-3"
				><Tabs.Trigger value="drive">Drive</Tabs.Trigger><Tabs.Trigger value="intake"
					>Intake</Tabs.Trigger
				><Tabs.Trigger value="shooter">Scoring</Tabs.Trigger></Tabs.List
			>
			<Tabs.Content value="drive" class="flex flex-col gap-3 pt-4"
				>{#each driveTypes as option}<button
						class="w-full rounded-lg border p-3 text-left transition-colors {driveType === option
							? 'border-primary bg-primary/10 text-primary'
							: 'border-border bg-card hover:bg-accent'}"
						onclick={() => (driveType = option)}
						><span class="font-semibold">{option}</span>{#if driveType === option}<span
								class="float-right font-bold">✓</span
							>{/if}</button
					>{/each}</Tabs.Content
			>
			<Tabs.Content value="intake" class="flex flex-col gap-3 pt-4"
				>{#each intakes as option}<button
						class="w-full rounded-lg border p-3 text-left transition-colors {selectedIntake ===
						option
							? 'border-primary bg-primary/10 text-primary'
							: 'border-border bg-card hover:bg-accent'}"
						onclick={() => (selectedIntake = option)}
						><span class="font-semibold">{option}</span>{#if selectedIntake === option}<span
								class="float-right font-bold">✓</span
							>{/if}</button
					>{/each}</Tabs.Content
			>
			<Tabs.Content value="shooter" class="flex flex-col gap-3 pt-4"
				>{#each shooters as option}<button
						class="w-full rounded-lg border p-3 text-left transition-colors {selectedShooter ===
						option
							? 'border-primary bg-primary/10 text-primary'
							: 'border-border bg-card hover:bg-accent'}"
						onclick={() => (selectedShooter = option)}
						><span class="font-semibold">{option}</span>{#if selectedShooter === option}<span
								class="float-right font-bold">✓</span
							>{/if}</button
					>{/each}</Tabs.Content
			>
		</Tabs.Root>
		{#if message}<p class="rounded-md border border-border bg-muted/50 p-3 text-sm" role="status">
				{message}
			</p>{/if}
		<div class="mt-auto pt-6">
			<Button class="w-full" disabled={isSaving} onclick={saveRobot}
				>{isSaving ? 'Saving…' : 'Save build'}</Button
			><Button variant="ghost" class="mt-2 w-full" href="/dashboard">Back to dashboard</Button>
		</div>
	</aside>
	<main class="relative flex flex-1 flex-col items-center justify-center bg-background/50 p-6">
		<div
			class="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_center,var(--color-primary)_0,transparent_100%)] opacity-5"
		></div>
		<div class="z-10 flex w-full max-w-xl flex-col items-center gap-4 text-center">
			<div
				class="flex h-72 w-full items-center justify-center rounded-xl border-4 border-dashed border-border bg-card/20 shadow-2xl backdrop-blur-sm"
			>
				<p class="font-mono text-sm text-muted-foreground">
					[ 3D Viewport ]<br />{driveType} chassis with {selectedIntake}
				</p>
			</div>
			<Card.Root class="w-full border-border bg-card/50 backdrop-blur"
				><Card.Header class="pb-3"><Card.Title>Saved robots</Card.Title></Card.Header><Card.Content
					>{#if isLoading}<p class="text-sm text-muted-foreground">
							Loading saved robots…
						</p>{:else if savedRobots.length === 0}<p class="text-sm text-muted-foreground">
							Save your first robot build to use it in future matches.
						</p>{:else}<ul class="space-y-2 text-left">
							{#each savedRobots as robot}<li
									class="flex items-center justify-between rounded-md border border-border p-3"
								>
									<span class="font-medium">{robot.name}</span><span
										class="text-xs text-muted-foreground"
										>{new Date(robot.updatedAt).toLocaleDateString()}</span
									>
								</li>{/each}
						</ul>{/if}</Card.Content
				></Card.Root
			>
		</div>
	</main>
</div>
