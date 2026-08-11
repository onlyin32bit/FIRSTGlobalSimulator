<script lang="ts">
	import {
		IconAdjustments,
		IconCheck,
		IconDeviceGamepad2,
		IconLoader2,
		IconUser,
		IconVideo
	} from '@tabler/icons-svelte';
	import { api, ApiError, type ApiUser } from '$lib/api';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { NativeSelect } from '$lib/components/ui/native-select';
	import { Switch } from '$lib/components/ui/switch';
	import { cn } from '$lib/utils';
	import {
		defaultPreferences,
		loadPreferences,
		savePreferences,
		type DriveMode,
		type UserPreferences
	} from './preferences.svelte';

	type Section = 'account' | 'graphics' | 'controls';

	let { user, onUserChanged }: { user: ApiUser | null; onUserChanged: (user: ApiUser) => void } =
		$props();
	let open = $state(false);
	let section = $state<Section>('account');
	let name = $state('');
	let team = $state('');
	let email = $state('');
	let savingAccount = $state(false);
	let accountMessage = $state('');
	let preferences = $state<UserPreferences>(defaultPreferences());
	let preferencesReady = $state(false);
	let preferencesUserId = $state<string | null>(null);

	const sections = [
		{ id: 'account' as const, label: 'Account', icon: IconUser },
		{ id: 'graphics' as const, label: 'Graphics', icon: IconVideo },
		{ id: 'controls' as const, label: 'Controls', icon: IconDeviceGamepad2 }
	];
	const buttonNames = [
		'A',
		'B',
		'X',
		'Y',
		'Left bumper',
		'Right bumper',
		'Left trigger',
		'Right trigger'
	];

	function hydrateUser(nextUser: ApiUser | null) {
		name = nextUser?.name ?? '';
		team = nextUser?.team ?? '';
		email = nextUser?.email ?? '';
	}

	$effect(() => hydrateUser(user));

	$effect(() => {
		if (!user || preferencesUserId === user.id) return;
		preferences = loadPreferences(user.id);
		preferencesUserId = user.id;
		preferencesReady = true;
	});

	$effect(() => {
		if (preferencesReady && user) savePreferences(user.id, preferences);
	});

	async function saveAccount() {
		if (!user) return;
		accountMessage = '';
		savingAccount = true;
		try {
			const result = await api.updateProfile({
				name: name.trim(),
				team: team.trim(),
				email: email.trim()
			});
			onUserChanged(result.user);
			accountMessage = 'Saved';
		} catch (error) {
			accountMessage = error instanceof ApiError ? error.message : 'Could not save your account.';
		} finally {
			savingAccount = false;
		}
	}

	function updateDriveMode(mode: DriveMode) {
		preferences.controls.driveMode = mode;
		preferences = { ...preferences };
	}
</script>

<Button variant="ghost" size="sm" onclick={() => (open = true)} aria-label="Open settings">
	<IconAdjustments class="size-4" />
	<span class="hidden sm:inline">Settings</span>
</Button>

<Dialog.Root bind:open>
	<Dialog.Content
		class="h-[min(720px,calc(100dvh-2rem))] max-w-[calc(100%-1rem)] overflow-hidden p-0 sm:max-w-4xl"
		showCloseButton={false}
	>
		<div class="grid h-full min-h-0 md:grid-cols-[13rem_1fr]">
			<aside
				class="flex border-b border-border/70 bg-muted/35 p-3 md:flex-col md:border-r md:border-b-0"
			>
				<div class="hidden px-2 pt-1 pb-6 md:block">
					<p class="font-daybreaker text-base tracking-wide text-foreground">Settings</p>
					<p class="mt-1 text-xs text-muted-foreground">Your simulator setup</p>
				</div>
				<nav class="flex w-full gap-1 overflow-auto md:flex-col" aria-label="Settings sections">
					{#each sections as item (item.id)}
						{@const Icon = item.icon}
						<button
							class={cn(
								'flex h-10 shrink-0 items-center gap-2 rounded-lg px-3 text-sm font-medium transition-colors',
								section === item.id
									? 'bg-background text-foreground shadow-sm ring-1 ring-border/80'
									: 'text-muted-foreground hover:bg-background/60 hover:text-foreground'
							)}
							onclick={() => (section = item.id)}
						>
							<Icon class="size-4" />{item.label}
						</button>
					{/each}
				</nav>
			</aside>

			<div class="min-h-0 overflow-y-auto">
				<div class="mx-auto max-w-2xl px-5 py-6 sm:px-8 sm:py-8">
					<div class="mb-8 flex items-start justify-between gap-4">
						<div>
							<h2 class="text-xl font-semibold tracking-tight">
								{sections.find((item) => item.id === section)?.label}
							</h2>
							<p class="mt-1 text-sm text-muted-foreground">
								{section === 'account'
									? 'How you appear in the simulator.'
									: section === 'graphics'
										? 'Saved now; applied to the renderer as options land.'
										: 'Your gamepad layout for robot matches.'}
							</p>
						</div>
						<Button
							variant="ghost"
							size="icon-sm"
							onclick={() => (open = false)}
							aria-label="Close settings">×</Button
						>
					</div>

					{#if section === 'account'}
						<form
							class="space-y-5"
							onsubmit={(event) => {
								event.preventDefault();
								void saveAccount();
							}}
						>
							<div class="grid gap-5 sm:grid-cols-2">
								<div class="space-y-2">
									<Label for="settings-name">Display name</Label><Input
										id="settings-name"
										bind:value={name}
										autocomplete="name"
										required
									/>
								</div>
								<div class="space-y-2">
									<Label for="settings-team">Team</Label><Input
										id="settings-team"
										bind:value={team}
										autocomplete="organization"
										required
									/>
								</div>
							</div>
							<div class="space-y-2">
								<Label for="settings-email">Email</Label><Input
									id="settings-email"
									type="email"
									bind:value={email}
									autocomplete="email"
									required
								/>
								<p class="text-xs text-muted-foreground">
									Changing this signs the address out of verification until email verification is
									added.
								</p>
							</div>
							<div class="flex items-center gap-3 pt-1">
								<Button type="submit" disabled={savingAccount}
									><span class="flex items-center gap-2"
										>{#if savingAccount}<IconLoader2 class="size-4 animate-spin" />{/if}Save changes</span
									></Button
								>{#if accountMessage}<span
										class={cn(
											'text-sm',
											accountMessage === 'Saved'
												? 'text-emerald-600 dark:text-emerald-400'
												: 'text-destructive'
										)}
										>{#if accountMessage === 'Saved'}<IconCheck
												class="mr-1 inline size-4"
											/>{/if}{accountMessage}</span
									>{/if}
							</div>
						</form>
					{:else if section === 'graphics'}
						<div class="space-y-7">
							<div class="grid gap-4 sm:grid-cols-3">
								<p class="text-sm font-medium sm:pt-2">Quality preset</p>
								<div class="sm:col-span-2">
									<div class="grid grid-cols-3 rounded-lg bg-muted p-1">
										{#each ['low', 'medium', 'high'] as quality}<button
												class={cn(
													'rounded-md px-3 py-2 text-sm font-medium capitalize transition-colors',
													preferences.graphics.quality === quality
														? 'bg-background text-foreground shadow-sm'
														: 'text-muted-foreground hover:text-foreground'
												)}
												onclick={() => {
													preferences.graphics.quality =
														quality as UserPreferences['graphics']['quality'];
													preferences = { ...preferences };
												}}>{quality}</button
											>{/each}
									</div>
									<p class="mt-2 text-xs text-muted-foreground">
										A convenient baseline for future rendering options.
									</p>
								</div>
							</div>
							<div class="grid gap-4 sm:grid-cols-3">
								<div>
									<p class="text-sm font-medium">Render resolution</p>
									<p class="mt-1 text-xs text-muted-foreground">
										{preferences.graphics.resolutionScale}% of display resolution
									</p>
								</div>
								<div class="flex items-center gap-3 sm:col-span-2">
									<input
										min="50"
										max="100"
										step="10"
										type="range"
										bind:value={preferences.graphics.resolutionScale}
										class="h-2 flex-1 cursor-pointer appearance-none rounded-full bg-muted accent-primary"
										aria-label="Render resolution"
									/><span class="w-10 text-right text-sm font-medium tabular-nums"
										>{preferences.graphics.resolutionScale}%</span
									>
								</div>
							</div>
							<div class="grid gap-4 sm:grid-cols-3">
								<div>
									<p class="text-sm font-medium">Camera field of view</p>
									<p class="mt-1 text-xs text-muted-foreground">
										Wider values show more of the field.
									</p>
								</div>
								<div class="flex items-center gap-3 sm:col-span-2">
									<input
										min="35"
										max="90"
										step="1"
										type="range"
										bind:value={preferences.graphics.cameraFov}
										class="h-2 flex-1 cursor-pointer appearance-none rounded-full bg-muted accent-primary"
										aria-label="Camera field of view"
									/><span class="w-10 text-right text-sm font-medium tabular-nums"
										>{preferences.graphics.cameraFov}°</span
									>
								</div>
							</div>
							<div class="divide-y divide-border rounded-xl border border-border/80">
								<div class="flex items-center justify-between gap-4 p-4">
									<div>
										<p class="text-sm font-medium">Shadows</p>
										<p class="mt-0.5 text-xs text-muted-foreground">
											Field, robot, and object shadow maps.
										</p>
									</div>
									<Switch bind:checked={preferences.graphics.shadows} />
								</div>
								<div class="flex items-center justify-between gap-4 p-4">
									<div>
										<p class="text-sm font-medium">Anti-aliasing</p>
										<p class="mt-0.5 text-xs text-muted-foreground">
											Smoother geometry at the cost of GPU work.
										</p>
									</div>
									<Switch bind:checked={preferences.graphics.antiAliasing} />
								</div>
								<div class="flex items-center justify-between gap-4 p-4">
									<div>
										<p class="text-sm font-medium">Visual effects</p>
										<p class="mt-0.5 text-xs text-muted-foreground">
											Non-essential arena effects and animation.
										</p>
									</div>
									<Switch bind:checked={preferences.graphics.showEffects} />
								</div>
							</div>
						</div>
					{:else}
						<div class="space-y-8">
							<div>
								<p class="text-sm font-medium">Drive style</p>
								<p class="mt-1 text-sm text-muted-foreground">
									Choose how the sticks translate into drive and turn.
								</p>
								<div class="mt-4 grid gap-2 sm:grid-cols-2">
									{#each [{ value: 'arcade-left', title: 'Left arcade', body: 'Left stick drives and turns.' }, { value: 'arcade-right', title: 'Right arcade', body: 'Right stick drives and turns.' }, { value: 'split-arcade', title: 'Split arcade', body: 'Left stick drives; right stick turns.' }, { value: 'tank', title: 'Tank', body: 'One stick for each side of the drivetrain.' }] as mode}<button
											class={cn(
												'rounded-xl border p-4 text-left transition-all',
												preferences.controls.driveMode === mode.value
													? 'border-primary bg-primary/5 ring-1 ring-primary/25'
													: 'border-border hover:border-foreground/25 hover:bg-muted/50'
											)}
											onclick={() => updateDriveMode(mode.value as DriveMode)}
											><span class="block text-sm font-semibold">{mode.title}</span><span
												class="mt-1 block text-xs leading-relaxed text-muted-foreground"
												>{mode.body}</span
											></button
										>{/each}
								</div>
							</div>
							<div class="grid gap-5 sm:grid-cols-2">
								<div class="space-y-2">
									<Label for="intake-button">Intake</Label><NativeSelect
										id="intake-button"
										class="w-full"
										bind:value={preferences.controls.intakeButton}
										>{#each buttonNames as button, index}<option value={index}>{button}</option
											>{/each}</NativeSelect
									>
								</div>
								<div class="space-y-2">
									<Label for="outtake-button">Outtake</Label><NativeSelect
										id="outtake-button"
										class="w-full"
										bind:value={preferences.controls.outtakeButton}
										>{#each buttonNames as button, index}<option value={index}>{button}</option
											>{/each}</NativeSelect
									>
								</div>
							</div>
							<p
								class="rounded-lg bg-muted/65 px-3 py-2 text-xs leading-relaxed text-muted-foreground"
							>
								Mappings are saved to this browser and will be used the next time you enter a match.
							</p>
						</div>
					{/if}
				</div>
			</div>
		</div>
	</Dialog.Content>
</Dialog.Root>
