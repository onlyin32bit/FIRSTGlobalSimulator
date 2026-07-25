<script lang="ts">
  import { useTask, T } from '@threlte/core';
  import { useRapier } from '@threlte/rapier';
  import { Object3D } from 'three';
  import { onMount, onDestroy } from 'svelte';
  import { robotPhysicsState, robotSpecs, robotStorage } from './stores';
  import { get } from 'svelte/store';
  
  let { potatoMode = false, resetTrigger = 0, ballsData = [] } = $props();
  
  const { world, rapier } = useRapier();
  
  let instancedMeshRef: any = $state();
  const dummyObj = new Object3D();
  let rapierBodies: any[] = [];
  let ballStates: string[] = [];
  
  let lastIntakeTime = 0;
  let lastShootTime = 0;
  
  function initPhysics() {
    // Clear existing
    rapierBodies.forEach(b => world.removeRigidBody(b));
    rapierBodies = [];
    ballStates = [];
    robotStorage.set(0);
    
    if (ballsData.length === 0) return;

    const rigidBodyDesc = rapier.RigidBodyDesc.dynamic()
      .setLinearDamping(0.8)
      .setAngularDamping(4.0)
      .setCcdEnabled(!potatoMode);
      
    const colliderDesc = rapier.ColliderDesc.ball(0.05)
      .setRestitution(0.5)
      .setFriction(1.0)
      .setMass(0.062);
      
    ballsData.forEach((ball: any) => {
      const body = world.createRigidBody(rigidBodyDesc);
      body.setTranslation(new rapier.Vector3(ball.x, ball.y, ball.z), true);
      world.createCollider(colliderDesc, body);
      rapierBodies.push(body);
      ballStates.push('active');
    });
  }

  onMount(() => {
    initPhysics();
  });
  
  $effect(() => {
    if (resetTrigger > 0) {
      initPhysics();
    }
  });
  
  onDestroy(() => {
    rapierBodies.forEach(b => world.removeRigidBody(b));
  });

  useTask((delta) => {
    if (!instancedMeshRef || rapierBodies.length === 0) return;
    
    const rState = get(robotPhysicsState);
    const specs = get(robotSpecs);
    let storage = get(robotStorage);
    let storageChanged = false;

    // --- INTAKE LOGIC ---
    if (rState.isIntakeActive && storage < specs.capacity) {
      lastIntakeTime += delta;
      const intakeInterval = 1.0 / specs.intakeRate;
      
      if (lastIntakeTime >= intakeInterval) {
        // Intake zone is slightly in front of the robot
        const intakeX = rState.pos.x + rState.forward.x * 0.4;
        const intakeZ = rState.pos.z + rState.forward.z * 0.4;
        const intakeRadiusSq = 0.4 * 0.4;

        for (let i = 0; i < rapierBodies.length; i++) {
          if (ballStates[i] === 'active') {
            const bPos = rapierBodies[i].translation();
            const dx = bPos.x - intakeX;
            const dz = bPos.z - intakeZ;
            const dy = Math.abs(bPos.y - rState.pos.y);
            
            if (dy < 0.5 && (dx*dx + dz*dz) < intakeRadiusSq) {
              // Pickup!
              ballStates[i] = 'stored';
              rapierBodies[i].setTranslation(new rapier.Vector3(0, -100, 0), true);
              rapierBodies[i].setLinvel(new rapier.Vector3(0, 0, 0), true);
              rapierBodies[i].setAngvel(new rapier.Vector3(0, 0, 0), true);
              storage++;
              storageChanged = true;
              lastIntakeTime = 0;
              break; // Only pickup one per interval
            }
          }
        }
      }
    } else {
      lastIntakeTime = 0;
    }

    // --- OUTTAKE LOGIC ---
    if (rState.isShootActive && storage > 0) {
      lastShootTime += delta;
      const shootInterval = 1.0 / specs.outtakeRate;
      
      if (lastShootTime >= shootInterval) {
        for (let i = 0; i < rapierBodies.length; i++) {
          if (ballStates[i] === 'stored') {
            // Shoot!
            ballStates[i] = 'active';
            
            // Outtake zone is at the back of the robot
            const outX = rState.pos.x - rState.forward.x * 0.4;
            const outY = rState.pos.y + 0.3;
            const outZ = rState.pos.z - rState.forward.z * 0.4;
            
            rapierBodies[i].setTranslation(new rapier.Vector3(outX, outY, outZ), true);
            
            // Calculate velocity trajectory
            const angleRad = specs.outtakeAngle * (Math.PI / 180);
            const vY = Math.sin(angleRad) * specs.outtakeVelocity;
            const vHoriz = Math.cos(angleRad) * specs.outtakeVelocity;
            const vX = -rState.forward.x * vHoriz; // backward
            const vZ = -rState.forward.z * vHoriz; // backward
            
            rapierBodies[i].setLinvel(new rapier.Vector3(vX, vY, vZ), true);
            
            storage--;
            storageChanged = true;
            lastShootTime = 0;
            break; // Only shoot one per interval
          }
        }
      }
    } else {
      lastShootTime = 0;
    }

    if (storageChanged) {
      robotStorage.set(storage);
    }
    
    // --- MATRIX SYNC ---
    for (let i = 0; i < rapierBodies.length; i++) {
      const body = rapierBodies[i];
      const pos = body.translation();
      const rot = body.rotation();
      
      dummyObj.position.set(pos.x, pos.y, pos.z);
      dummyObj.quaternion.set(rot.x, rot.y, rot.z, rot.w);
      dummyObj.updateMatrix();
      
      instancedMeshRef.setMatrixAt(i, dummyObj.matrix);
    }
    instancedMeshRef.instanceMatrix.needsUpdate = true;
  });
</script>

{#if ballsData.length > 0}
  <T.InstancedMesh bind:ref={instancedMeshRef} args={[undefined, undefined, ballsData.length]} castShadow={!potatoMode} receiveShadow>
    {#if potatoMode}
      <T.IcosahedronGeometry args={[0.05, 1]} />
    {:else}
      <T.SphereGeometry args={[0.05, 8, 8]} />
    {/if}
    {#if potatoMode}
      <T.MeshLambertMaterial color="#f97316" />
    {:else}
      <T.MeshStandardMaterial color="#f97316" roughness={0.9} metalness={0.0} />
    {/if}
  </T.InstancedMesh>
{/if}
