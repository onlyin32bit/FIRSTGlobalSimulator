<script lang="ts">
	import { Canvas } from '@threlte/core';
	import Scene from './Scene.svelte';
	import { robotTelemetry } from './telemetry';
	import {
		robotSpecs,
		robotStorage,
		ballsInPlay,
		humanPlayerCharge,
		humanPlayerThrowMaxSpeed,
		humanPlayerStorage,
		humanPlayerTargetedBall,
		matchSlotsStore,
		activeRobotSlotId,
		humanPlayerAlliance,
		showRobotTagsStore
	} from './stores';
	import GamepadDebug from './GamepadDebug.svelte';
	import { scores } from '$lib/scoreStore';

	import { onMount } from 'svelte';

	let resetTrigger = $state(0);
	let fov = $state(75);
	let speed = $state(10);
	let potatoMode = $state(false);
	let showPhysicsDebug = $state(true);
	let showGamepadDebug = $state(false);
	let role = $state<'robot-controller' | 'human-player'>('robot-controller');
	let throwCharge = $derived($humanPlayerCharge);

	function resetScene() {
		resetTrigger += 1;
	}

	onMount(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.key === 'Tab') {
				e.preventDefault();
				showRobotTagsStore.set(true);
				return;
			}
			// Don't trigger if user is typing in an input
			if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
			if (e.key.toLowerCase() === 'r' && !e.repeat) {
				resetScene();
			}
			if (e.key.toLowerCase() === 'p' && !e.repeat) {
				showPhysicsDebug = !showPhysicsDebug;
			}
			if (e.key.toLowerCase() === 'v' && !e.repeat) {
				role = role === 'robot-controller' ? 'human-player' : 'robot-controller';
			}
		};

		const handleKeyUp = (e: KeyboardEvent) => {
			if (e.key === 'Tab') {
				e.preventDefault();
				showRobotTagsStore.set(false);
			}
		};

		const handleBlur = () => {
			showRobotTagsStore.set(false);
		};

		window.addEventListener('keydown', handleKeyDown);
		window.addEventListener('keyup', handleKeyUp);
		window.addEventListener('blur', handleBlur);

		return () => {
			window.removeEventListener('keydown', handleKeyDown);
			window.removeEventListener('keyup', handleKeyUp);
			window.removeEventListener('blur', handleBlur);
		};
	});
</script>

<div class="relative h-screen w-full bg-gray-900">
	<div
		class="absolute top-0 left-0 z-10 flex max-h-screen w-72 flex-col gap-2 overflow-y-auto bg-black/60 p-2 font-mono text-[11px] leading-tight text-white"
	>
		<button
			class="w-full cursor-pointer rounded bg-blue-600/80 px-2 py-1 text-left font-bold text-white transition-colors hover:bg-blue-500 active:bg-blue-700"
			onclick={resetScene}
		>
			Reset / Unstick Robot (R)
		</button>

		<button
			class={`flex w-full cursor-pointer items-center justify-between rounded border px-2 py-1 text-left font-bold text-white transition-all ${showGamepadDebug ? 'border-amber-400 bg-amber-600' : 'border-gray-700 bg-gray-800/90 hover:bg-gray-700'}`}
			onclick={() => (showGamepadDebug = !showGamepadDebug)}
		>
			<span>🎮 Gamepad Diagnostic HUD</span>
			<span class="rounded bg-black/40 px-1.5 py-0.5 font-mono text-[9px] font-bold text-amber-300"
				>{showGamepadDebug ? 'OPEN' : 'OFF'}</span
			>
		</button>

		<div class="flex gap-1">
			<button
				class={`flex-1 rounded px-2 py-1 ${role === 'robot-controller' ? 'bg-blue-600 hover:bg-blue-500' : 'bg-gray-700/80 hover:bg-gray-600'}`}
				onclick={() => (role = 'robot-controller')}>Robot Controller</button
			>
			<button
				class={`flex-1 rounded px-2 py-1 ${role === 'human-player' ? 'bg-emerald-600 hover:bg-emerald-500' : 'bg-gray-700/80 hover:bg-gray-600'}`}
				onclick={() => (role = 'human-player')}>Human Player</button
			>
		</div>

		<div class="mt-1 space-y-2">
			<div class="flex flex-col gap-1">
				<label for="fov-input" class="flex justify-between">
					<span>Camera FOV</span>
					<span>{fov}°</span>
				</label>
				<input id="fov-input" type="range" min="30" max="120" bind:value={fov} class="w-full" />
			</div>

			<!-- MULTIPLAYER & MATCH SETUP CONFIG -->
			<div class="space-y-2 border-t border-gray-500/50 pt-2">
				<div
					class="flex items-center justify-between text-[11px] font-bold tracking-wider text-amber-400 uppercase"
				>
					<span>MATCH SETUP (6v6 ROSTER)</span>
				</div>

				{#if role === 'human-player'}
					<div
						class="flex flex-col gap-1.5 rounded border border-emerald-500/40 bg-gray-800/80 p-2"
					>
						<span class="text-[10px] font-bold text-emerald-400 uppercase"
							>HUMAN PLAYER ALLIANCE</span
						>
						<div class="flex gap-1">
							<button
								class={`flex-1 cursor-pointer rounded px-2 py-1 text-center text-xs font-bold transition-colors ${$humanPlayerAlliance === 'red' ? 'bg-red-600 text-white shadow-md' : 'bg-gray-700 text-gray-300 hover:bg-gray-600'}`}
								onclick={() => ($humanPlayerAlliance = 'red')}
							>
								Red Alliance
							</button>
							<button
								class={`flex-1 cursor-pointer rounded px-2 py-1 text-center text-xs font-bold transition-colors ${$humanPlayerAlliance === 'blue' ? 'bg-blue-600 text-white shadow-md' : 'bg-gray-700 text-gray-300 hover:bg-gray-600'}`}
								onclick={() => ($humanPlayerAlliance = 'blue')}
							>
								Blue Alliance
							</button>
						</div>
						<span class="text-[9px] text-gray-300">
							Positioned at {$humanPlayerAlliance === 'red'
								? 'Red HP Zone (redFSscore)'
								: 'Blue HP Zone (blueFSscore)'}
						</span>
					</div>
				{:else}
					<div class="flex flex-col gap-1 rounded border border-blue-500/40 bg-gray-800/80 p-2">
						<span class="text-[10px] font-bold text-blue-400 uppercase">CONTROLLED ROBOT SLOT</span>
						<select
							bind:value={$activeRobotSlotId}
							class="cursor-pointer rounded border border-gray-600 bg-gray-900 px-2 py-1 font-mono text-xs text-white"
						>
							{#each $matchSlotsStore.filter((s) => s.controller !== 'disabled') as slot (slot.id)}
								<option value={slot.id}>
									{slot.name} ({slot.alliance.toUpperCase()} · {slot.spawnAnchor})
								</option>
							{/each}
						</select>
					</div>
				{/if}

				<!-- 6 ROBOT SLOTS CONFIGURATOR -->
				<div class="mt-2 space-y-1.5">
					<div class="text-[10px] font-bold text-gray-300 uppercase">ROBOT SLOTS CONFIGURATION</div>
					{#each $matchSlotsStore as slot, idx (slot.id)}
						<div
							class={`flex flex-col gap-1 rounded border p-1.5 text-[10px] ${slot.alliance === 'red' ? 'border-red-800/60 bg-red-950/40' : 'border-blue-800/60 bg-blue-950/40'}`}
						>
							<div class="flex items-center justify-between font-bold">
								<span class={slot.alliance === 'red' ? 'text-red-400' : 'text-blue-400'}>
									{slot.name}
								</span>
								{#if role === 'robot-controller' && slot.id === $activeRobotSlotId}
									<span class="rounded bg-emerald-500 px-1 text-[8px] font-black text-white"
										>DRIVING</span
									>
								{/if}
							</div>

							<div class="grid grid-cols-2 gap-1">
								<div>
									<label class="block text-[9px] text-gray-400">Controller</label>
									<select
										value={slot.controller}
										onchange={(e) => {
											const val = (e.target as HTMLSelectElement).value as any;
											matchSlotsStore.update((slots) => {
												slots[idx].controller = val;
												return [...slots];
											});
										}}
										class="w-full cursor-pointer rounded border border-gray-700 bg-gray-900 px-1 py-0.5 text-[10px] text-white"
									>
										<option value="human-drive">Human Driver</option>
										<option value="ai-bot">AI Bot</option>
										<option value="disabled">Disabled</option>
									</select>
								</div>

								<div>
									<label class="block text-[9px] text-gray-400">Spawn Anchor</label>
									<select
										value={slot.spawnAnchor}
										onchange={(e) => {
											const val = (e.target as HTMLSelectElement).value;
											matchSlotsStore.update((slots) => {
												slots[idx].spawnAnchor = val;
												return [...slots];
											});
										}}
										class="w-full cursor-pointer rounded border border-gray-700 bg-gray-900 px-1 py-0.5 text-[10px] text-white"
									>
										<option value="redSpawn1">redSpawn1</option>
										<option value="redSpawn2">redSpawn2</option>
										<option value="redSpawn3">redSpawn3</option>
										<option value="blueSpawn1">blueSpawn1</option>
										<option value="blueSpawn2">blueSpawn2</option>
										<option value="blueSpawn3">blueSpawn3</option>
									</select>
								</div>
							</div>
						</div>
					{/each}
				</div>
			</div>

			<!-- INTAKE & OUTTAKE SPECS -->
			<div class="mt-1 space-y-2 border-t border-gray-500/50 pt-1">
				<div class="text-gray-300">--- SHOOTER SPECS ---</div>

				<div class="flex flex-col gap-1">
					<label for="intake-rate" class="flex justify-between">
						<span>Intake Rate</span>
						<span>{($robotSpecs.intakeRate * 60).toFixed(0)} /min</span>
					</label>
					<input
						id="intake-rate"
						type="range"
						min="1"
						max="10"
						step="0.5"
						bind:value={$robotSpecs.intakeRate}
						class="w-full"
					/>
				</div>

				<div class="flex flex-col gap-1">
					<label for="outtake-rate" class="flex justify-between">
						<span>Outtake Rate</span>
						<span>{($robotSpecs.outtakeRate * 60).toFixed(0)} /min</span>
					</label>
					<input
						id="outtake-rate"
						type="range"
						min="0.5"
						max="5"
						step="0.5"
						bind:value={$robotSpecs.outtakeRate}
						class="w-full"
					/>
				</div>

				<div class="flex flex-col gap-1">
					<label for="outtake-angle" class="flex justify-between">
						<span>Angle</span>
						<span>{$robotSpecs.outtakeAngle}°</span>
					</label>
					<input
						id="outtake-angle"
						type="range"
						min="0"
						max="90"
						step="1"
						bind:value={$robotSpecs.outtakeAngle}
						class="w-full"
					/>
				</div>

				<div class="flex flex-col gap-1">
					<label for="outtake-vel" class="flex justify-between">
						<span>Velocity</span>
						<span>{$robotSpecs.outtakeVelocity} m/s</span>
					</label>
					<input
						id="outtake-vel"
						type="range"
						min="1"
						max="15"
						step="0.5"
						bind:value={$robotSpecs.outtakeVelocity}
						class="w-full"
					/>
				</div>
			</div>

			<!-- TRANSFER SPECS -->
			<div class="mt-1 space-y-2 border-t border-gray-500/50 pt-1">
				<div class="text-gray-300">--- TRANSFER SPECS ---</div>

				<div class="flex flex-col gap-1">
					<label for="transfer-rate" class="flex justify-between">
						<span>Transfer Rate</span>
						<span>{$robotSpecs.transferRate.toFixed(1)} bursts/s</span>
					</label>
					<input
						id="transfer-rate"
						type="range"
						min="0.5"
						max="5"
						step="0.5"
						bind:value={$robotSpecs.transferRate}
						class="w-full"
					/>
				</div>

				<div class="flex justify-between">
					<span>Burst Size</span>
					<span>{$robotSpecs.transferBurstMin}–{$robotSpecs.transferBurstMax} balls</span>
				</div>

				<div class="flex flex-col gap-1">
					<label for="transfer-height" class="flex justify-between">
						<span>Height (Elevation)</span>
						<span>{$robotSpecs.transferHeight.toFixed(2)} m</span>
					</label>
					<input
						id="transfer-height"
						type="range"
						min="0.05"
						max="0.60"
						step="0.05"
						bind:value={$robotSpecs.transferHeight}
						class="w-full"
					/>
				</div>

				<div class="flex flex-col gap-1">
					<label for="transfer-angle" class="flex justify-between">
						<span>Angle</span>
						<span>{$robotSpecs.transferAngle}°</span>
					</label>
					<input
						id="transfer-angle"
						type="range"
						min="0"
						max="60"
						step="1"
						bind:value={$robotSpecs.transferAngle}
						class="w-full"
					/>
				</div>

				<div class="flex flex-col gap-1">
					<label for="transfer-vel" class="flex justify-between">
						<span>Velocity</span>
						<span>{$robotSpecs.transferVelocity} m/s</span>
					</label>
					<input
						id="transfer-vel"
						type="range"
						min="1"
						max="12"
						step="0.5"
						bind:value={$robotSpecs.transferVelocity}
						class="w-full"
					/>
				</div>
			</div>

			<div class="mt-1 flex flex-col gap-1 border-t border-gray-500/50 pt-1">
				<label class="flex cursor-pointer items-center gap-2">
					<input type="checkbox" bind:checked={showPhysicsDebug} class="h-3 w-3" />
					<span>Physics Debug HUD (P)</span>
				</label>
				<label class="flex cursor-pointer items-center gap-2">
					<input type="checkbox" bind:checked={potatoMode} class="h-3 w-3" />
					<span>Potato Mode (Performance)</span>
				</label>
			</div>

			<div class="mt-1 space-y-1 border-t border-gray-500/50 pt-1">
				<label for="human-throw-max-speed" class="flex justify-between">
					<span>Human Throw Max Speed</span>
					<span>{$humanPlayerThrowMaxSpeed.toFixed(1)} m/s</span>
				</label>
				<input
					id="human-throw-max-speed"
					type="range"
					min="3"
					max="20"
					step="0.5"
					bind:value={$humanPlayerThrowMaxSpeed}
					class="w-full"
				/>
			</div>
		</div>

		<div class="mt-1 space-y-0.5 border-t border-gray-500/50 pt-1 text-gray-300">
			<div>WASD: Pan Camera</div>
			<div>Space/Shift: Up/Down</div>
			<div>Mouse Drag: Orbit Look</div>
			<div>Arrow Keys: Drive Robot</div>
			<div>E/Q/F: Intake/Shoot/Transfer</div>
			<div>P: Toggle Physics Debug</div>
			<div>R: Unstick / Reset Robot</div>
			<div>Gamepad: Drive, Intake/Shoot (R1/L1), Transfer (A)</div>
			<div>V: Switch Role · Human Player: WASD/mouse or gamepad</div>
			<div>Human: E/Click/X grab ball under crosshair · hold/release mouse or A to throw</div>
		</div>
	</div>

	<div
		class="pointer-events-none absolute top-0 right-0 z-10 bg-black/60 p-2 font-mono text-[11px] leading-tight whitespace-pre text-white"
	>
		FGC26 Simulator v1.0 FPS: {$robotTelemetry.fps.toFixed(0)}
		XYZ: {$robotTelemetry.x.toFixed(3)} / {$robotTelemetry.y.toFixed(3)} / {$robotTelemetry.z.toFixed(
			3
		)}
		Speed: {$robotTelemetry.speed.toFixed(3)} m/s Accel: {$robotTelemetry.accel.toFixed(3)} m/s² Turn:
		{($robotTelemetry.turnRate * (180 / Math.PI)).toFixed(1)}°/s Capacity: {$robotStorage} / {$robotSpecs.capacity}
		balls Human Player Balls: {$humanPlayerStorage}
		Field Balls: {$ballsInPlay}
		{#if showPhysicsDebug}
			--- PHYSICS DEBUG --- Contacts: {$robotTelemetry.contactCount}
			{$robotTelemetry.contacts.join(', ')}
			Forward: {$robotTelemetry.forwardSpeed.toFixed(3)} / {$robotTelemetry.requestedForwardSpeed.toFixed(
				3
			)} m/s Drive impulse: {$robotTelemetry.driveImpulse.toFixed(3)}
			Contact force: {$robotTelemetry.contactForce.toFixed(2)}
			Stuck timer: {$robotTelemetry.stuckTime.toFixed(2)}s Auto-unsticks: {$robotTelemetry.autoUnstickCount}
		{/if}
	</div>

	<!-- Scoreboard HUD -->
	<div class="pointer-events-none absolute top-4 left-1/2 z-10 -translate-x-1/2">
		<div
			class="flex items-stretch gap-0 overflow-hidden rounded-xl border border-white/10 shadow-2xl backdrop-blur-md"
		>
			<div class="flex min-w-[120px] flex-col items-center bg-blue-900/80 px-5 py-3">
				<span class="text-[10px] font-bold tracking-widest text-blue-300/80 uppercase">Blue</span>
				<div class="mt-1 flex gap-4">
					<span class="text-2xl font-black text-white tabular-nums">{$scores.blueSU}</span><span
						class="text-2xl font-black text-white tabular-nums">{$scores.blueFS}</span
					>
				</div>
			</div>
			<div
				class="flex min-w-[80px] flex-col items-center justify-center border-x border-white/10 bg-gray-900/90 px-5 py-3"
			>
				<span class="text-[10px] font-bold tracking-widest text-orange-300/80 uppercase">EXT</span
				><span class="mt-0.5 text-3xl font-black text-orange-400 tabular-nums">{$scores.EXT}</span>
			</div>
			<div class="flex min-w-[120px] flex-col items-center bg-red-900/80 px-5 py-3">
				<span class="text-[10px] font-bold tracking-widest text-red-300/80 uppercase">Red</span>
				<div class="mt-1 flex gap-4">
					<span class="text-2xl font-black text-white tabular-nums">{$scores.redSU}</span><span
						class="text-2xl font-black text-white tabular-nums">{$scores.redFS}</span
					>
				</div>
			</div>
		</div>
	</div>

	{#key potatoMode}
		<Canvas dpr={potatoMode ? 1 : undefined}>
			<Scene {resetTrigger} {fov} {speed} {potatoMode} {role} />
		</Canvas>
	{/key}

	{#if role === 'human-player'}
		<div class="pointer-events-none absolute inset-0 z-20 flex items-center justify-center">
			<div
				class="relative flex h-8 w-8 items-center justify-center text-white drop-shadow-[0_1px_2px_rgba(0,0,0,0.9)]"
			>
				<span
					class={`absolute h-5 w-0.5 transition-colors duration-150 ${$humanPlayerTargetedBall ? 'bg-cyan-400 shadow-[0_0_8px_rgba(56,189,248,0.8)]' : 'bg-white'}`}
				></span>
				<span
					class={`absolute h-0.5 w-5 transition-colors duration-150 ${$humanPlayerTargetedBall ? 'bg-cyan-400 shadow-[0_0_8px_rgba(56,189,248,0.8)]' : 'bg-white'}`}
				></span>
				{#if throwCharge > 0}
					<span
						class="absolute -bottom-5 h-1 rounded bg-emerald-400"
						style={`width: ${throwCharge * 32}px`}
					></span>
				{/if}
			</div>
		</div>

		{#if $humanPlayerTargetedBall && $humanPlayerTargetedBall.visible && $humanPlayerStorage === 0}
			<div
				class="pointer-events-none absolute z-30 flex animate-in items-center gap-2.5 rounded-xl border border-cyan-400/50 bg-gray-950/85 px-3.5 py-2 text-white shadow-xl shadow-cyan-500/20 backdrop-blur-md transition-all duration-200 ease-out zoom-in-95 fade-in"
				style={`left: ${$humanPlayerTargetedBall.screenX}px; top: ${$humanPlayerTargetedBall.screenY}px; transform: translate(-50%, -100%);`}
			>
				<kbd
					class="flex h-6 w-6 items-center justify-center rounded-md border border-cyan-300/80 bg-cyan-500/25 font-mono text-xs font-black text-cyan-200 shadow-[0_0_10px_rgba(56,189,248,0.4)]"
				>
					E
				</kbd>
				<span class="text-xs font-bold tracking-wider text-cyan-50 uppercase drop-shadow">
					Pick Up Ball
				</span>
			</div>
		{/if}
	{/if}

	<GamepadDebug bind:open={showGamepadDebug} />
</div>
