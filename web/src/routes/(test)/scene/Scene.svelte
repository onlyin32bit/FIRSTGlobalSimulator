<script lang="ts">
  import { T, useThrelte, useTask } from '@threlte/core'
  import { OrbitControls, Sky, Grid } from '@threlte/extras'
  import { World, RigidBody, AutoColliders, CollisionGroups, Collider } from '@threlte/rapier'
  import type { RigidBody as RapierRigidBody } from '@dimforge/rapier3d-compat'
  import { Vector3 } from 'three'
  import { onMount } from 'svelte'
  import Robot from './Robot.svelte'
  import Balls from './Balls.svelte'

  let { resetTrigger = 0, fov = 75, speed = 10, potatoMode = false } = $props();
  let fieldAnchors = $state<Record<string, [number, number, number]>>({});
  let readyToSpawn = $state(false);

  // Reset scores when scene resets
  $effect(() => {
    if (resetTrigger > 0) resetScores();
  });


  // Extra height offset so robot/balls drop onto the field rather than clipping into it
  const SPAWN_HEIGHT_EXTRA = 3;

  // The body origin is at the bottom of the chassis. The packed field's surface
  // is around y=0.60, so this gives it a small, non-penetrating drop on spawn.
  const ROBOT_SPAWN_Y = 0.65;

  // ── Tune these to move the player spawn point on the field ─────────────────
  const PLAYER_SPAWN_OFFSET: [number, number] = [
    -4,   // X offset (positive = right)
    1,   // Z offset (positive = forward)
  ];
  // ───────────────────────────────────────────────────────────────────────────

  let robotSpawnPos = $derived<[number, number, number]>([
    (fieldAnchors['blueSpawn1'] || [0, 0, 3.15])[0] + PLAYER_SPAWN_OFFSET[0],
    ROBOT_SPAWN_Y,
    (fieldAnchors['blueSpawn1'] || [0, 0, 3.15])[2] + PLAYER_SPAWN_OFFSET[1],
  ]);

  let centerGoalPos = $derived(
    fieldAnchors['blueZone2'] || [0, 0, 0]
  );

  // Generate 500 balls clustered in a pile near the center
  const balls = $derived(Array.from({ length: 500 }).map((_, i) => {
    const angle = Math.random() * Math.PI * 2;
    // Concentrate them mostly within a 2.5-meter radius
    const r = Math.sqrt(Math.random()) * 2.5;
    return {
      id: i,
      x: Math.cos(angle) * r,
      y: 0.1 + Math.random() * 1.5, // Stacked up to 1.5m high
      z: Math.sin(angle) * r,
      color: '#f97316' // All balls are now orange
    };
  });

  // --- Camera Controls ---
  const keys = { w: false, a: false, s: false, d: false, space: false, shift: false };
  let cameraTarget = $state<[number, number, number]>([0, 1, 0]);
  
  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      if (key === 'w') keys.w = true;
      if (key === 'a') keys.a = true;
      if (key === 's') keys.s = true;
      if (key === 'd') keys.d = true;
      if (key === ' ') keys.space = true;
      if (key === 'shift') keys.shift = true;
    };
    const handleKeyUp = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      if (key === 'w') keys.w = false;
      if (key === 'a') keys.a = false;
      if (key === 's') keys.s = false;
      if (key === 'd') keys.d = false;
      if (key === ' ') keys.space = false;
      if (key === 'shift') keys.shift = false;
    };
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    }
  });

  const { camera } = useThrelte();

  useTask((delta) => {
    const cam = camera.current;
    if (!cam) return;

    const forward = new Vector3();
    cam.getWorldDirection(forward);
    forward.y = 0;
    if (forward.lengthSq() > 0.001) {
      forward.normalize();
    } else {
      forward.set(0, 0, -1);
    }
    
    const right = new Vector3().crossVectors(forward, new Vector3(0, 1, 0)).normalize();

    let moveVec = new Vector3();
    if (keys.w) moveVec.add(forward);
    if (keys.s) moveVec.sub(forward);
    if (keys.a) moveVec.sub(right);
    if (keys.d) moveVec.add(right);
    
    if (keys.space) moveVec.y += 1;
    if (keys.shift) moveVec.y -= 1;

    if (moveVec.lengthSq() > 0) {
      const camSpeed = speed * delta;
      moveVec.normalize().multiplyScalar(camSpeed);
      
      cameraTarget = [
        cameraTarget[0] + moveVec.x,
        cameraTarget[1] + moveVec.y,
        cameraTarget[2] + moveVec.z
      ];
      cam.position.add(moveVec);
    }
  });
</script>

<T.PerspectiveCamera makeDefault {fov} position={[0, 5, 10]}>
  <OrbitControls target={cameraTarget} enableDamping={false} />
</T.PerspectiveCamera>

<!-- Environment -->
<Sky elevation={2} />
<T.AmbientLight intensity={0.5} />
<T.DirectionalLight 
  position={[10, 10, 5]} 
  intensity={1.5} 
  castShadow 
  shadow.mapSize={[potatoMode ? 512 : 1024, potatoMode ? 512 : 1024]}
/>

<Grid
  position={[0, 0.01, 0]}
  cellColor="#ffffff"
  sectionColor="#ffffff"
  sectionThickness={0}
  fadeDistance={20}
  cellSize={2}
/>

<World framerate={potatoMode ? 30 : 60}>
  <Robot {resetTrigger} />

  <!-- EVA Foam Ground (FTC Tiles) -->
  <CollisionGroups groups={[0]}>
    <RigidBody type="fixed">
      <AutoColliders shape="cuboid" friction={1.2} restitution={0.5}>
        <T.Mesh position={[0, -0.5, 0]} receiveShadow>
          <T.BoxGeometry args={[40, 1, 40]} />
          
          <!-- High-contrast grid for FTC floor visualization (EVA foam tiles) -->
        {#if potatoMode}
          <T.MeshLambertMaterial color="#333333" />
        {:else}
          <T.MeshStandardMaterial color="#333333" roughness={0.9} metalness={0.1} />
        {/if}
        </T.Mesh>
      </AutoColliders>
    </RigidBody>
  </CollisionGroups>

  <!-- 7m x 7m Perimeter Walls (Polycarbonate style, 20cm tall) -->
  <CollisionGroups groups={[1]}>
    <RigidBody type="fixed">
      <!-- North Wall -->
      <AutoColliders shape="cuboid">
        <T.Mesh position={[0, 0.1, -3.5]} castShadow receiveShadow>
          <T.BoxGeometry args={[7.1, 0.2, 0.1]} />
        {#if potatoMode}
          <T.MeshLambertMaterial color="#ffffff" transparent opacity={0.4} />
        {:else}
          <T.MeshStandardMaterial color="#ffffff" roughness={0.1} metalness={0.1} transparent opacity={0.4} />
        {/if}
        </T.Mesh>
      </AutoColliders>
      
      <!-- South Wall -->
      <AutoColliders shape="cuboid">
        <T.Mesh position={[0, 0.1, 3.5]} castShadow receiveShadow>
          <T.BoxGeometry args={[7.1, 0.2, 0.1]} />
        {#if potatoMode}
          <T.MeshLambertMaterial color="#ffffff" transparent opacity={0.4} />
        {:else}
          <T.MeshStandardMaterial color="#ffffff" roughness={0.1} metalness={0.1} transparent opacity={0.4} />
        {/if}
        </T.Mesh>
      </AutoColliders>
      
      <!-- East Wall -->
      <AutoColliders shape="cuboid">
        <T.Mesh position={[3.5, 0.1, 0]} castShadow receiveShadow>
          <T.BoxGeometry args={[0.1, 0.2, 7.1]} />
        {#if potatoMode}
          <T.MeshLambertMaterial color="#ffffff" transparent opacity={0.4} />
        {:else}
          <T.MeshStandardMaterial color="#ffffff" roughness={0.1} metalness={0.1} transparent opacity={0.4} />
        {/if}
        </T.Mesh>
      </AutoColliders>
      
      <!-- West Wall -->
      <AutoColliders shape="cuboid">
        <T.Mesh position={[-3.5, 0.1, 0]} castShadow receiveShadow>
          <T.BoxGeometry args={[0.1, 0.2, 7.1]} />
        {#if potatoMode}
          <T.MeshLambertMaterial color="#ffffff" transparent opacity={0.4} />
        {:else}
          <T.MeshStandardMaterial color="#ffffff" roughness={0.1} metalness={0.1} transparent opacity={0.4} />
        {/if}
        </T.Mesh>
      </AutoColliders>
    </RigidBody>
  </CollisionGroups>

  <!-- PU Foam Balls -->
  <Balls ballsData={balls} {potatoMode} {resetTrigger} />
</World>
