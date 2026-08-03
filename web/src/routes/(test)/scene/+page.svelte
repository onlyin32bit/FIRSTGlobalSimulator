<script lang="ts">
  import { Canvas } from '@threlte/core'
  import Scene from './Scene.svelte'
  import { robotTelemetry } from './telemetry'
  import { robotSpecs, robotStorage, ballsInPlay } from './stores'
  import { scores } from '$lib/scoreStore'

  import { onMount } from 'svelte'

  let resetTrigger = $state(0);
  let fov = $state(75);
  let speed = $state(10);
  let potatoMode = $state(false);
  let showPhysicsDebug = $state(true);

  function resetScene() {
    resetTrigger += 1;
  }

  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't trigger if user is typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if (e.key.toLowerCase() === 'r' && !e.repeat) {
        resetScene();
      }
      if (e.key.toLowerCase() === 'p' && !e.repeat) {
        showPhysicsDebug = !showPhysicsDebug;
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  });
</script>

<div class="relative w-full h-screen bg-gray-900">
  <div class="absolute top-0 left-0 z-10 flex flex-col gap-2 p-2 bg-black/60 w-72 text-white font-mono text-[11px] leading-tight max-h-screen overflow-y-auto">
    <button
      class="bg-blue-600/80 hover:bg-blue-500 active:bg-blue-700 px-2 py-1 text-white w-full text-left cursor-pointer font-bold rounded transition-colors"
      onclick={resetScene}
    >
      Reset / Unstick Robot (R)
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

      <!-- TRANSFER SPECS -->
      <div class="pt-1 mt-1 border-t border-gray-500/50 space-y-2">
        <div class="text-gray-300">--- TRANSFER SPECS ---</div>
        
        <div class="flex flex-col gap-1">
          <label for="transfer-rate" class="flex justify-between">
            <span>Transfer Rate</span>
            <span>{$robotSpecs.transferRate.toFixed(1)} bursts/s</span>
          </label>
          <input id="transfer-rate" type="range" min="0.5" max="5" step="0.5" bind:value={$robotSpecs.transferRate} class="w-full" />
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
          <input id="transfer-height" type="range" min="0.05" max="0.60" step="0.05" bind:value={$robotSpecs.transferHeight} class="w-full" />
        </div>

        <div class="flex flex-col gap-1">
          <label for="transfer-angle" class="flex justify-between">
            <span>Angle</span>
            <span>{$robotSpecs.transferAngle}°</span>
          </label>
          <input id="transfer-angle" type="range" min="0" max="60" step="1" bind:value={$robotSpecs.transferAngle} class="w-full" />
        </div>

        <div class="flex flex-col gap-1">
          <label for="transfer-vel" class="flex justify-between">
            <span>Velocity</span>
            <span>{$robotSpecs.transferVelocity} m/s</span>
          </label>
          <input id="transfer-vel" type="range" min="1" max="12" step="0.5" bind:value={$robotSpecs.transferVelocity} class="w-full" />
        </div>
      </div>

      <div class="pt-1 mt-1 border-t border-gray-500/50 flex flex-col gap-1">
        <label class="flex items-center gap-2 cursor-pointer">
          <input type="checkbox" bind:checked={showPhysicsDebug} class="w-3 h-3" />
          <span>Physics Debug HUD (P)</span>
        </label>
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
      <div>Arrow Keys: Drive Robot</div>
      <div>E/Q/F: Intake/Shoot/Transfer</div>
      <div>P: Toggle Physics Debug</div>
      <div>R: Unstick / Reset Robot</div>
      <div>Gamepad: Drive, Intake/Shoot (R1/L1), Transfer (A)</div>
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
{#if showPhysicsDebug}
--- PHYSICS DEBUG ---
Contacts: {$robotTelemetry.contactCount} {$robotTelemetry.contacts.join(', ')}
Forward: {$robotTelemetry.forwardSpeed.toFixed(3)} / {$robotTelemetry.requestedForwardSpeed.toFixed(3)} m/s
Drive impulse: {$robotTelemetry.driveImpulse.toFixed(3)}
Contact force: {$robotTelemetry.contactForce.toFixed(2)}
Stuck timer: {$robotTelemetry.stuckTime.toFixed(2)}s
Auto-unsticks: {$robotTelemetry.autoUnstickCount}
{/if}
</div>

  <!-- Scoreboard HUD -->
  <div class="absolute top-4 left-1/2 -translate-x-1/2 z-10 pointer-events-none">
    <div class="flex items-stretch gap-0 rounded-xl overflow-hidden shadow-2xl border border-white/10 backdrop-blur-md">
      <div class="bg-blue-900/80 px-5 py-3 flex flex-col items-center min-w-[120px]">
        <span class="text-[10px] font-bold uppercase tracking-widest text-blue-300/80">Blue</span>
        <div class="flex gap-4 mt-1"><span class="text-2xl font-black text-white tabular-nums">{$scores.blueSU}</span><span class="text-2xl font-black text-white tabular-nums">{$scores.blueFS}</span></div>
      </div>
      <div class="bg-gray-900/90 px-5 py-3 flex flex-col items-center justify-center min-w-[80px] border-x border-white/10"><span class="text-[10px] font-bold uppercase tracking-widest text-orange-300/80">EXT</span><span class="text-3xl font-black text-orange-400 tabular-nums mt-0.5">{$scores.EXT}</span></div>
      <div class="bg-red-900/80 px-5 py-3 flex flex-col items-center min-w-[120px]"><span class="text-[10px] font-bold uppercase tracking-widest text-red-300/80">Red</span><div class="flex gap-4 mt-1"><span class="text-2xl font-black text-white tabular-nums">{$scores.redSU}</span><span class="text-2xl font-black text-white tabular-nums">{$scores.redFS}</span></div></div>
    </div>
  </div>

  {#key potatoMode}
    <Canvas dpr={potatoMode ? 1 : undefined}>
      <Scene {resetTrigger} {fov} {speed} {potatoMode} />
    </Canvas>
  {/key}
</div>
