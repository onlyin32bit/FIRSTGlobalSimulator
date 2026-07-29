<script lang="ts">
  import { Canvas } from '@threlte/core'
  import Scene from './Scene.svelte'
  import { robotTelemetry } from './telemetry'
  import { robotSpecs, robotStorage, ballsInPlay } from './stores'

  let resetTrigger = $state(0);
  let fov = $state(75);
  let speed = $state(10);
  let potatoMode = $state(false);

  function resetScene() {
    resetTrigger += 1;
  }
</script>

<div class="relative w-full h-screen bg-gray-900">
  <div class="absolute top-0 left-0 z-10 flex flex-col gap-2 p-2 bg-black/60 w-72 text-white font-mono text-[11px] leading-tight">
    <button
      class="bg-gray-700/80 px-2 py-1 text-white hover:bg-gray-600 active:bg-gray-500 w-full text-left cursor-pointer"
      onclick={resetScene}
    >
      Reset Scene
    </button>
    
    <div class="space-y-2 mt-1">
      <div class="flex flex-col gap-1">
        <label for="fov-input" class="flex justify-between">
          <span>Camera FOV</span>
          <span>{fov}°</span>
        </label>
        <input id="fov-input" type="range" min="30" max="120" bind:value={fov} class="w-full" />
      </div>

      <div class="flex flex-col gap-1">
        <label for="speed-input" class="flex justify-between">
          <span>Fly Speed</span>
          <span>{speed} m/s</span>
        </label>
        <input id="speed-input" type="range" min="1" max="50" bind:value={speed} class="w-full" />
      </div>

      <!-- INTAKE & OUTTAKE SPECS -->
      <div class="pt-1 mt-1 border-t border-gray-500/50 space-y-2">
        <div class="text-gray-300">--- SHOOTER SPECS ---</div>
        
        <div class="flex flex-col gap-1">
          <label for="intake-rate" class="flex justify-between">
            <span>Intake Rate</span>
            <span>{($robotSpecs.intakeRate * 60).toFixed(0)} /min</span>
          </label>
          <input id="intake-rate" type="range" min="1" max="10" step="0.5" bind:value={$robotSpecs.intakeRate} class="w-full" />
        </div>

        <div class="flex flex-col gap-1">
          <label for="outtake-rate" class="flex justify-between">
            <span>Outtake Rate</span>
            <span>{($robotSpecs.outtakeRate * 60).toFixed(0)} /min</span>
          </label>
          <input id="outtake-rate" type="range" min="0.5" max="5" step="0.5" bind:value={$robotSpecs.outtakeRate} class="w-full" />
        </div>

        <div class="flex flex-col gap-1">
          <label for="outtake-angle" class="flex justify-between">
            <span>Angle</span>
            <span>{$robotSpecs.outtakeAngle}°</span>
          </label>
          <input id="outtake-angle" type="range" min="0" max="90" step="1" bind:value={$robotSpecs.outtakeAngle} class="w-full" />
        </div>

        <div class="flex flex-col gap-1">
          <label for="outtake-vel" class="flex justify-between">
            <span>Velocity</span>
            <span>{$robotSpecs.outtakeVelocity} m/s</span>
          </label>
          <input id="outtake-vel" type="range" min="1" max="15" step="0.5" bind:value={$robotSpecs.outtakeVelocity} class="w-full" />
        </div>
      </div>

      <div class="pt-1 mt-1 border-t border-gray-500/50">
        <label class="flex items-center gap-2 cursor-pointer">
          <input type="checkbox" bind:checked={potatoMode} class="w-3 h-3" />
          <span>Potato Mode (Performance)</span>
        </label>
      </div>
    </div>
    
    <div class="pt-1 mt-1 border-t border-gray-500/50 space-y-0.5 text-gray-300">
      <div>WASD: Pan Camera</div>
      <div>Space/Shift: Up/Down</div>
      <div>Mouse Drag: Orbit Look</div>
      <div>Gamepad: Drive Robot</div>
    </div>
  </div>
  
  <div class="absolute top-0 right-0 z-10 p-2 bg-black/60 text-white font-mono text-[11px] leading-tight pointer-events-none whitespace-pre">
FGC26 Simulator v1.0
FPS: {$robotTelemetry.fps.toFixed(0)}
XYZ: {$robotTelemetry.x.toFixed(3)} / {$robotTelemetry.y.toFixed(3)} / {$robotTelemetry.z.toFixed(3)}
Speed: {$robotTelemetry.speed.toFixed(3)} m/s
Accel: {$robotTelemetry.accel.toFixed(3)} m/s²
Turn: {($robotTelemetry.turnRate * (180 / Math.PI)).toFixed(1)}°/s
Capacity: {$robotStorage} / {$robotSpecs.capacity} balls
Field Balls: {$ballsInPlay}
  </div>
  
  <!-- Remount Canvas when potatoMode changes -->
  {#key potatoMode}
    <Canvas dpr={potatoMode ? 1 : undefined}>
      <Scene {resetTrigger} {fov} {speed} {potatoMode} />
    </Canvas>
  {/key}
</div>
