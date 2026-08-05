<script lang="ts">
  import { onMount } from 'svelte';

  let { open = $bindable(false) } = $props();

  type GamepadInfo = {
    index: number;
    id: string;
    mapping: string;
    connected: boolean;
    axes: number[];
    buttons: { pressed: boolean; value: number }[];
  };

  let connectedGamepads = $state<GamepadInfo[]>([]);
  let animFrameId: number;

  function pollGamepads() {
    const rawPads = navigator.getGamepads ? navigator.getGamepads() : [];
    const list: GamepadInfo[] = [];

    for (let i = 0; i < rawPads.length; i++) {
      const pad = rawPads[i];
      if (pad && pad.connected) {
        const axes = Array.from(pad.axes).map((v) => Number(v.toFixed(3)));
        const buttons = Array.from(pad.buttons).map((b) => ({
          pressed: b.pressed,
          value: Number(b.value.toFixed(2))
        }));

        list.push({
          index: pad.index,
          id: pad.id,
          mapping: pad.mapping || 'non-standard',
          connected: pad.connected,
          axes,
          buttons
        });
      }
    }

    connectedGamepads = list;
    animFrameId = requestAnimationFrame(pollGamepads);
  }

  onMount(() => {
    animFrameId = requestAnimationFrame(pollGamepads);

    const onConnect = (e: GamepadEvent) => {
      console.log('[GamepadDebug] Gamepad connected:', e.gamepad.id);
    };
    const onDisconnect = (e: GamepadEvent) => {
      console.log('[GamepadDebug] Gamepad disconnected:', e.gamepad.id);
    };

    window.addEventListener('gamepadconnected', onConnect);
    window.addEventListener('gamepaddisconnected', onDisconnect);

    return () => {
      cancelAnimationFrame(animFrameId);
      window.removeEventListener('gamepadconnected', onConnect);
      window.removeEventListener('gamepaddisconnected', onDisconnect);
    };
  });

  const buttonNames = [
    'A / Cross (0)',
    'B / Circle (1)',
    'X / Square (2)',
    'Y / Triangle (3)',
    'LB / L1 (4)',
    'RB / R1 (5)',
    'LT / L2 (6)',
    'RT / R2 (7)',
    'Back / Select (8)',
    'Start (9)',
    'L3 / Left Stick (10)',
    'R3 / Right Stick (11)',
    'D-Pad Up (12)',
    'D-Pad Down (13)',
    'D-Pad Left (14)',
    'D-Pad Right (15)',
    'Guide / Home (16)'
  ];
</script>

{#if open}
  <div class="fixed top-4 right-4 z-50 w-96 max-h-[90vh] bg-gray-900/95 text-white border border-amber-500/50 rounded-xl shadow-2xl backdrop-blur-md overflow-hidden flex flex-col font-mono text-xs select-none">
    <!-- Header -->
    <div class="bg-gradient-to-r from-amber-600/40 to-yellow-600/40 px-3 py-2 border-b border-amber-500/30 flex items-center justify-between">
      <div class="flex items-center gap-2 font-bold text-amber-400">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 5v2m0 4v2m0 4v2M5 5a2 2 0 00-2 2v3a2 2 0 002 2h14a2 2 0 002-2V7a2 2 0 002-2H5z" />
        </svg>
        <span>GAMEPAD DIAGNOSTIC SYSTEM</span>
      </div>
      <button
        class="text-gray-400 hover:text-white px-1.5 py-0.5 rounded hover:bg-white/10 cursor-pointer"
        onclick={() => (open = false)}
      >
        ✕
      </button>
    </div>

    <div class="p-3 overflow-y-auto space-y-3">
      <!-- Status Banner -->
      {#if connectedGamepads.length === 0}
        <div class="bg-red-950/60 border border-red-500/40 p-3 rounded-lg text-red-200 text-center space-y-1">
          <div class="font-bold text-red-400">NO GAMEPAD DETECTED</div>
          <p class="text-[10px] text-gray-300">
            Plug in your controller or press any button on an already connected gamepad to activate Web API recognition.
          </p>
        </div>
      {:else}
        <div class="bg-emerald-950/60 border border-emerald-500/40 p-2 rounded-lg text-emerald-300 text-xs flex items-center justify-between">
          <span class="font-bold">GAMEPAD DETECTED ({connectedGamepads.length})</span>
          <span class="text-[9px] bg-emerald-500/20 px-2 py-0.5 rounded text-emerald-400 border border-emerald-500/30">
            ACTIVE
          </span>
        </div>
      {/if}

      <!-- Device List -->
      {#each connectedGamepads as pad}
        <div class="bg-gray-800/80 border border-gray-700 rounded-lg p-2.5 space-y-2">
          <!-- Controller Title -->
          <div class="flex flex-col gap-0.5 border-b border-gray-700/60 pb-1.5">
            <div class="font-bold text-yellow-300 text-[11px] truncate" title={pad.id}>
              [{pad.index}] {pad.id}
            </div>
            <div class="flex items-center gap-2 text-[9px] text-gray-400">
              <span>Mapping: <strong class="text-white">{pad.mapping}</strong></span>
              <span>Axes: <strong class="text-white">{pad.axes.length}</strong></span>
              <span>Buttons: <strong class="text-white">{pad.buttons.length}</strong></span>
            </div>
          </div>

          <!-- Analog Axes Section -->
          <div class="space-y-1">
            <div class="text-[10px] font-bold text-amber-300 uppercase">Analog Axes</div>
            <div class="grid grid-cols-2 gap-1.5">
              {#each pad.axes as val, idx}
                <div class="bg-gray-900/90 p-1.5 rounded border border-gray-700/50 flex flex-col gap-1">
                  <div class="flex justify-between text-[9px]">
                    <span class="text-gray-400">
                      {idx === 0 ? 'Axis 0 (LX)' : idx === 1 ? 'Axis 1 (LY)' : idx === 2 ? 'Axis 2 (RX)' : idx === 3 ? 'Axis 3 (RY)' : `Axis ${idx}`}
                    </span>
                    <span class="font-mono text-amber-400 font-bold">{val > 0 ? `+${val}` : val}</span>
                  </div>
                  <!-- Meter bar -->
                  <div class="relative w-full h-2 bg-gray-800 rounded overflow-hidden">
                    <div
                      class="absolute top-0 bottom-0 bg-amber-500 transition-all duration-75"
                      style={`left: ${val < 0 ? `${(val + 1) * 50}%` : '50%'}; right: ${val > 0 ? `${(1 - val) * 50}%` : '50%'}`}
                    ></div>
                    <div class="absolute top-0 bottom-0 left-1/2 w-0.5 bg-gray-500"></div>
                  </div>
                </div>
              {/each}
            </div>
          </div>

          <!-- Buttons Section -->
          <div class="space-y-1">
            <div class="text-[10px] font-bold text-amber-300 uppercase">Buttons</div>
            <div class="grid grid-cols-2 gap-1">
              {#each pad.buttons as btn, idx}
                <div
                  class={`px-2 py-1 rounded text-[10px] flex items-center justify-between border transition-all ${
                    btn.pressed
                      ? 'bg-amber-500 text-black font-bold border-amber-300 shadow-md shadow-amber-500/20'
                      : 'bg-gray-900/80 text-gray-400 border-gray-800'
                  }`}
                >
                  <span class="truncate">{buttonNames[idx] || `Btn ${idx}`}</span>
                  {#if btn.value > 0 && btn.value < 1}
                    <span class="text-[8px] opacity-75">{(btn.value * 100).toFixed(0)}%</span>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        </div>
      {/each}

      <!-- Troubleshooting Quick Tips -->
      <div class="bg-gray-950/80 border border-gray-800 p-2.5 rounded-lg space-y-1 text-[10px] text-gray-400">
        <div class="font-bold text-gray-200">Browser Gamepad Troubleshooting:</div>
        <ul class="list-disc pl-4 space-y-0.5">
          <li>Press any face button (A/B/X/Y) to register the controller with Chrome/Edge.</li>
          <li>If using a DualShock/DualSense or Switch Pro controller, ensure Steam input or DS4Windows isn't swallowing xinput.</li>
          <li>Default Controls: Left Stick = Forward / Back, Right Stick = Turn, R1/RT = Intake, L1/LT = Shoot, A/X = Transfer.</li>
        </ul>
      </div>
    </div>
  </div>
{/if}
