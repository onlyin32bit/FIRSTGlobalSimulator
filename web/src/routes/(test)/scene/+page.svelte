<script lang="ts">
  import { Canvas } from '@threlte/core'
  import Scene from './Scene.svelte'
  import { robotTelemetry } from './telemetry'

  let resetTrigger = $state(0);
  let fov = $state(75);
  let speed = $state(10);

  function resetScene() {
    resetTrigger += 1;
  }
</script>

<div class="relative w-full h-screen bg-gray-900">
  <div class="absolute top-4 left-4 z-10 flex flex-col gap-4 p-5 bg-gray-800/90 rounded-2xl backdrop-blur-md border border-gray-700 shadow-2xl w-72">
    <button
      class="rounded-lg bg-blue-600 px-4 py-2.5 font-bold text-white shadow-lg hover:bg-blue-500 active:bg-blue-700 transition-colors w-full"
      onclick={resetScene}
    >
      Reset Scene
    </button>
    
    <div class="space-y-4 mt-2">
      <div>
        <label for="fov-input" class="flex justify-between text-sm font-medium text-gray-200 mb-2">
          <span>Camera FOV</span>
          <span class="text-blue-400 font-mono">{fov}°</span>
        </label>
        <input id="fov-input" type="range" min="30" max="120" bind:value={fov} class="w-full accent-blue-500" />
      </div>

      <div>
        <label for="speed-input" class="flex justify-between text-sm font-medium text-gray-200 mb-2">
          <span>Fly Speed</span>
          <span class="text-blue-400 font-mono">{speed} m/s</span>
        </label>
        <input id="speed-input" type="range" min="1" max="50" bind:value={speed} class="w-full accent-blue-500" />
      </div>
    </div>
    
    <div class="text-xs text-gray-400 mt-4 bg-gray-900/50 p-3 rounded-lg border border-gray-700/50 space-y-1">
      <p><b class="text-gray-300">WASD</b>: Pan Camera</p>
      <p><b class="text-gray-300">Space / Shift</b>: Up / Down</p>
      <p><b class="text-gray-300">Mouse Drag</b>: Orbit Look</p>
      <p><b class="text-gray-300">Gamepad</b>: Drive Robot</p>
    </div>
  </div>
  
  <!-- Telemetry HUD (Minecraft F3 Style) -->
  <div class="absolute top-0 right-0 z-10 p-2 bg-black/60 text-white font-mono text-[11px] leading-tight pointer-events-none whitespace-pre">
FGC26 Simulator v1.0
XYZ: {$robotTelemetry.x.toFixed(3)} / {$robotTelemetry.y.toFixed(3)} / {$robotTelemetry.z.toFixed(3)}
Speed: {$robotTelemetry.speed.toFixed(3)} m/s
Accel: {$robotTelemetry.accel.toFixed(3)} m/s²
Turn: {($robotTelemetry.turnRate * (180 / Math.PI)).toFixed(1)}°/s
  </div>
  
  <Canvas>
    <Scene {resetTrigger} {fov} {speed} />
  </Canvas>
</div>
