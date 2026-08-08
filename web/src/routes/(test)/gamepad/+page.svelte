<script lang="ts">
	import { onMount } from 'svelte';

	// We need to map the gamepad data into a plain object to ensure Svelte's reactivity
	// catches all updates when we re-assign it every frame.
	type ButtonState = { pressed: boolean; touched: boolean; value: number };
	type GamepadState = {
		id: string;
		index: number;
		connected: boolean;
		mapping: string;
		axes: number[];
		buttons: ButtonState[];
	};

	let gamepads = $state<GamepadState[]>([]);
	let requestRef: number;

	function updateGamepads() {
		const rawGamepads = navigator.getGamepads ? navigator.getGamepads() : [];
		const activeGamepads: GamepadState[] = [];

		for (let i = 0; i < rawGamepads.length; i++) {
			const gp = rawGamepads[i];
			if (gp && gp.connected) {
				activeGamepads.push({
					id: gp.id,
					index: gp.index,
					connected: gp.connected,
					mapping: gp.mapping,
					axes: [...gp.axes],
					buttons: gp.buttons.map((b) => ({
						pressed: b.pressed,
						touched: b.touched,
						value: b.value
					}))
				});
			}
		}

		gamepads = activeGamepads;
		requestRef = requestAnimationFrame(updateGamepads);
	}

	onMount(() => {
		// Start the poll loop
		requestRef = requestAnimationFrame(updateGamepads);

		// Optional: We can listen to events, though polling handles updates anyway
		const onConnect = (e: GamepadEvent) => console.log('Gamepad connected:', e.gamepad.id);
		const onDisconnect = (e: GamepadEvent) => console.log('Gamepad disconnected:', e.gamepad.id);

		window.addEventListener('gamepadconnected', onConnect);
		window.addEventListener('gamepaddisconnected', onDisconnect);

		return () => {
			cancelAnimationFrame(requestRef);
			window.removeEventListener('gamepadconnected', onConnect);
			window.removeEventListener('gamepaddisconnected', onDisconnect);
		};
	});
</script>

<div class="min-h-screen bg-gray-900 p-4 font-sans text-white md:p-8">
	<div class="mx-auto max-w-7xl">
		<h1 class="mb-2 text-center text-3xl font-bold text-blue-400">Gamepad Tester</h1>
		<p class="mb-8 text-center text-gray-400">Test your controllers, joysticks, and wheels</p>

		{#if gamepads.length === 0}
			<div
				class="flex flex-col items-center justify-center rounded-xl border-2 border-dashed border-gray-700 bg-gray-800 p-12 shadow-lg"
			>
				<svg
					class="mb-4 h-16 w-16 text-gray-600"
					fill="none"
					stroke="currentColor"
					viewBox="0 0 24 24"
					xmlns="http://www.w3.org/2000/svg"
				>
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						stroke-width="2"
						d="M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z"
					></path>
				</svg>
				<p class="text-xl font-medium text-gray-300">No gamepads detected</p>
				<p class="mt-2 text-center text-sm text-gray-500">
					Plug in a controller and <strong class="text-blue-400">press any button</strong> to wake it
					up.
				</p>
			</div>
		{:else}
			<div class="grid grid-cols-1 gap-8 xl:grid-cols-2">
				{#each gamepads as gamepad (gamepad.index)}
					<div class="rounded-xl border border-gray-700 bg-gray-800 p-6 shadow-xl">
						<h2 class="mb-1 truncate text-xl font-semibold text-gray-100" title={gamepad.id}>
							{gamepad.id}
						</h2>
						<div class="mb-6 flex flex-wrap gap-4 text-sm text-gray-400">
							<span class="rounded bg-gray-700 px-2 py-1">Index: {gamepad.index}</span>
							<span class="rounded bg-gray-700 px-2 py-1"
								>Mapping: {gamepad.mapping || 'standard'}</span
							>
						</div>

						<div class="grid grid-cols-1 gap-8 lg:grid-cols-2">
							<!-- Axes Section -->
							<div>
								<h3
									class="mb-4 flex justify-between border-b border-gray-700 pb-2 text-lg font-medium text-gray-300"
								>
									<span>Axes</span>
									<span class="rounded bg-gray-800 px-2 py-1 text-xs text-gray-500"
										>({gamepad.axes.length})</span
									>
								</h3>
								<div class="space-y-4">
									{#each gamepad.axes as axis, i (i)}
										<div>
											<div class="mb-1 flex justify-between text-xs text-gray-400">
												<span>Axis {i}</span>
												<span class="font-mono {Math.abs(axis) > 0.1 ? 'text-blue-400' : ''}">
													{axis.toFixed(4)}
												</span>
											</div>
											<div
												class="relative h-4 w-full overflow-hidden rounded-full bg-gray-700 ring-1 ring-gray-900/50 ring-inset"
											>
												<!-- Center marker -->
												<div
													class="absolute top-0 bottom-0 left-1/2 z-10 w-0.5 bg-gray-400 shadow-[0_0_2px_rgba(0,0,0,0.5)]"
												></div>

												<!-- Value bar -->
												<div
													class="absolute top-0 h-full transition-all duration-75 ease-out
                            {axis < 0 ? 'bg-indigo-500' : 'bg-blue-500'}"
													style="
                            width: {Math.abs(axis) * 50}%;
                            left: {axis < 0 ? 50 - Math.abs(axis) * 50 : 50}%;
                          "
												></div>
											</div>
										</div>
									{/each}
								</div>
							</div>

							<!-- Buttons Section -->
							<div>
								<h3
									class="mb-4 flex justify-between border-b border-gray-700 pb-2 text-lg font-medium text-gray-300"
								>
									<span>Buttons</span>
									<span class="rounded bg-gray-800 px-2 py-1 text-xs text-gray-500"
										>({gamepad.buttons.length})</span
									>
								</h3>
								<div class="grid grid-cols-4 gap-3 sm:grid-cols-5">
									{#each gamepad.buttons as button, i (i)}
										<div class="flex flex-col items-center">
											<div
												class="relative flex h-12 w-12 items-center justify-center overflow-hidden rounded-lg border text-sm font-bold transition-all duration-75
                          {button.pressed
													? 'scale-95 border-blue-400 bg-blue-600 text-white shadow-[0_0_12px_rgba(37,99,235,0.6)]'
													: 'border-gray-600 bg-gray-700 text-gray-300 shadow-inner'}"
											>
												<span class="relative z-10">B{i}</span>

												<!-- Analog button value background (for triggers) -->
												{#if button.value > 0 && !button.pressed}
													<div
														class="absolute right-0 bottom-0 left-0 bg-blue-500/40"
														style="height: {button.value * 100}%"
													></div>
												{/if}
											</div>

											<!-- Value text (useful for analog triggers) -->
											<span
												class="mt-1 font-mono text-[10px] text-gray-500 {button.value > 0
													? 'text-blue-300'
													: ''}"
											>
												{button.value.toFixed(2)}
											</span>
										</div>
									{/each}
								</div>
							</div>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
</div>
