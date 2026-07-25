<script lang="ts">
  import { T, useThrelte, useTask } from '@threlte/core'
  import { OrbitControls, Sky, Grid } from '@threlte/extras'
  import { World, RigidBody, AutoColliders, CollisionGroups } from '@threlte/rapier'
  import type { RigidBody as RapierRigidBody } from '@dimforge/rapier3d-compat'
  import { Vector3 } from 'three'
  import { onMount } from 'svelte'
  import Robot from './Robot.svelte'

  let { resetTrigger = 0, fov = 75, speed = 10 } = $props();

  // Generate 500 balls clustered in a pile on the ground
  const balls = Array.from({ length: 500 }).map((_, i) => {
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
  <OrbitControls target={cameraTarget} enableDamping />
</T.PerspectiveCamera>

<!-- Environment -->
<Sky elevation={2} />
<T.AmbientLight intensity={0.5} />
<T.DirectionalLight position={[10, 10, 5]} intensity={1.5} castShadow />

<Grid
  position={[0, 0.01, 0]}
  cellColor="#ffffff"
  sectionColor="#ffffff"
  sectionThickness={0}
  fadeDistance={20}
  cellSize={2}
/>

<World framerate={120}>
  <Robot {resetTrigger} />

  <!-- EVA Foam Ground (FTC Tiles) -->
  <CollisionGroups groups={[0]}>
    <RigidBody type="fixed">
      <AutoColliders shape="cuboid" friction={1.2} restitution={0.1}>
        <T.Mesh position={[0, -0.5, 0]} receiveShadow>
          <T.BoxGeometry args={[40, 1, 40]} />
          
          <!-- High-contrast grid for FTC floor visualization (EVA foam tiles) -->
          <T.MeshStandardMaterial color="#333333" roughness={0.9} metalness={0.1} />
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
          <T.MeshStandardMaterial color="#ffffff" roughness={0.1} metalness={0.1} transparent opacity={0.4} />
        </T.Mesh>
      </AutoColliders>
      
      <!-- South Wall -->
      <AutoColliders shape="cuboid">
        <T.Mesh position={[0, 0.1, 3.5]} castShadow receiveShadow>
          <T.BoxGeometry args={[7.1, 0.2, 0.1]} />
          <T.MeshStandardMaterial color="#ffffff" roughness={0.1} metalness={0.1} transparent opacity={0.4} />
        </T.Mesh>
      </AutoColliders>
      
      <!-- East Wall -->
      <AutoColliders shape="cuboid">
        <T.Mesh position={[3.5, 0.1, 0]} castShadow receiveShadow>
          <T.BoxGeometry args={[0.1, 0.2, 7.1]} />
          <T.MeshStandardMaterial color="#ffffff" roughness={0.1} metalness={0.1} transparent opacity={0.4} />
        </T.Mesh>
      </AutoColliders>
      
      <!-- West Wall -->
      <AutoColliders shape="cuboid">
        <T.Mesh position={[-3.5, 0.1, 0]} castShadow receiveShadow>
          <T.BoxGeometry args={[0.1, 0.2, 7.1]} />
          <T.MeshStandardMaterial color="#ffffff" roughness={0.1} metalness={0.1} transparent opacity={0.4} />
        </T.Mesh>
      </AutoColliders>
    </RigidBody>
  </CollisionGroups>

  <!-- PU Foam Balls (10cm diameter = 0.05m radius) -->
  {#key resetTrigger}
    {#each balls as ball (ball.id)}
      <CollisionGroups groups={[0, 1, 2]}>
        <T.Group position={[ball.x, ball.y, ball.z]}>
          <!-- Adding damping simulates rolling resistance against the foam -->
          <RigidBody 
            type="dynamic" 
            linearDamping={0.8} 
            angularDamping={4.0}
            ccd={true}
          >
            <!-- High friction and lower restitution for PU foam on EVA foam -->
            <AutoColliders shape="ball" restitution={0.5} friction={1.0} mass={0.062}>
              <T.Mesh castShadow>
                <T.SphereGeometry args={[0.05, 16, 16]} />
                <T.MeshStandardMaterial color={ball.color} roughness={0.9} metalness={0.0} />
              </T.Mesh>
            </AutoColliders>
          </RigidBody>
        </T.Group>
      </CollisionGroups>
    {/each}
  {/key}
</World>
