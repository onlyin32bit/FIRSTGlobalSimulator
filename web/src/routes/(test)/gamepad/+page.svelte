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
          buttons: gp.buttons.map(b => ({ 
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

<div class="min-h-screen bg-gray-900 text-white p-4 md:p-8 font-sans">
  <div class="max-w-7xl mx-auto">
    <h1 class="text-3xl font-bold mb-2 text-center text-blue-400">Gamepad Tester</h1>
    <p class="text-center text-gray-400 mb-8">Test your controllers, joysticks, and wheels</p>

    {#if gamepads.length === 0}
      <div class="flex flex-col items-center justify-center p-12 border-2 border-dashed border-gray-700 rounded-xl bg-gray-800 shadow-lg">
        <svg class="w-16 h-16 text-gray-600 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z"></path>
        </svg>
        <p class="text-xl text-gray-300 font-medium">No gamepads detected</p>
        <p class="text-sm text-gray-500 mt-2 text-center">
          Plug in a controller and <strong class="text-blue-400">press any button</strong> to wake it up.
        </p>
      </div>
    {:else}
      <div class="grid grid-cols-1 xl:grid-cols-2 gap-8">
        {#each gamepads as gamepad (gamepad.index)}
          <div class="bg-gray-800 rounded-xl p-6 shadow-xl border border-gray-700">
            <h2 class="text-xl font-semibold mb-1 text-gray-100 truncate" title={gamepad.id}>
              {gamepad.id}
            </h2>
            <div class="text-sm text-gray-400 mb-6 flex flex-wrap gap-4">
              <span class="bg-gray-700 px-2 py-1 rounded">Index: {gamepad.index}</span>
              <span class="bg-gray-700 px-2 py-1 rounded">Mapping: {gamepad.mapping || 'standard'}</span>
            </div>

            <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
              <!-- Axes Section -->
              <div>
                <h3 class="text-lg font-medium text-gray-300 mb-4 border-b border-gray-700 pb-2 flex justify-between">
                  <span>Axes</span>
                  <span class="text-xs text-gray-500 bg-gray-800 px-2 py-1 rounded">({gamepad.axes.length})</span>
                </h3>
                <div class="space-y-4">
                  {#each gamepad.axes as axis, i}
                    <div>
                      <div class="flex justify-between text-xs text-gray-400 mb-1">
                        <span>Axis {i}</span>
                        <span class="font-mono {Math.abs(axis) > 0.1 ? 'text-blue-400' : ''}">
                          {axis.toFixed(4)}
                        </span>
                      </div>
                      <div class="w-full bg-gray-700 rounded-full h-4 relative overflow-hidden ring-1 ring-inset ring-gray-900/50">
                        <!-- Center marker -->
                        <div class="absolute top-0 bottom-0 left-1/2 w-0.5 bg-gray-400 z-10 shadow-[0_0_2px_rgba(0,0,0,0.5)]"></div>
                        
                        <!-- Value bar -->
                        <div 
                          class="h-full absolute top-0 transition-all duration-75 ease-out
                            {axis < 0 ? 'bg-indigo-500' : 'bg-blue-500'}"
                          style="
                            width: {Math.abs(axis) * 50}%;
                            left: {axis < 0 ? (50 - Math.abs(axis) * 50) : 50}%;
                          "
                        ></div>
                      </div>
                    </div>
                  {/each}
                </div>
              </div>

              <!-- Buttons Section -->
              <div>
                <h3 class="text-lg font-medium text-gray-300 mb-4 border-b border-gray-700 pb-2 flex justify-between">
                  <span>Buttons</span>
                  <span class="text-xs text-gray-500 bg-gray-800 px-2 py-1 rounded">({gamepad.buttons.length})</span>
                </h3>
                <div class="grid grid-cols-4 sm:grid-cols-5 gap-3">
                  {#each gamepad.buttons as button, i}
                    <div class="flex flex-col items-center">
                      <div 
                        class="w-12 h-12 rounded-lg flex items-center justify-center text-sm font-bold transition-all duration-75 relative overflow-hidden border
                          {button.pressed 
                            ? 'bg-blue-600 border-blue-400 text-white shadow-[0_0_12px_rgba(37,99,235,0.6)] scale-95' 
                            : 'bg-gray-700 border-gray-600 text-gray-300 shadow-inner'}"
                      >
                        <span class="z-10 relative">B{i}</span>
                        
                        <!-- Analog button value background (for triggers) -->
                        {#if button.value > 0 && !button.pressed}
                          <div 
                            class="absolute bottom-0 left-0 right-0 bg-blue-500/40" 
                            style="height: {button.value * 100}%"
                          ></div>
                        {/if}
                      </div>
                      
                      <!-- Value text (useful for analog triggers) -->
                      <span class="text-[10px] text-gray-500 mt-1 font-mono {button.value > 0 ? 'text-blue-300' : ''}">
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
