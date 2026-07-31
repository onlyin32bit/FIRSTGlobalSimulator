<script lang="ts">
  import { useTask, T } from '@threlte/core';
  import { useRapier } from '@threlte/rapier';
  import { Object3D } from 'three';
  import { onMount, onDestroy } from 'svelte';
  import { robotPhysicsState, robotSpecs, robotStorage, ballsInPlay } from './stores';
  import { get } from 'svelte/store';
  
  let { potatoMode = false, resetTrigger = 0, ballsData = [] } = $props();
  
  const { world, rapier } = useRapier();
  
  let instancedMeshRef: any = $state();
  const dummyObj = new Object3D();
  let rapierBodies: any[] = [];
  let ballStates: string[] = [];
  let visualSwallows = new Map();
  let lastReportedInPlay = 500;
  
  let lastIntakeTime = 0;
  let lastShootTime = 0;

  // Lightweight PU foam: some rebound, but far less than a rubber ball.
  const BALL_RESTITUTION = 0.4;
  const BALL_FRICTION = 0.75;
  const INTAKE_FORWARD_MIN = 0.08;
  const INTAKE_FORWARD_MAX = 0.72;
  const INTAKE_HALF_WIDTH = 0.34;
  const INTAKE_MAX_HEIGHT = 0.5;
  const INTAKE_PULL_SPEED = 1.5;
  
  function initPhysics() {
    // Clear existing
    rapierBodies.forEach(b => world.removeRigidBody(b));
    rapierBodies = [];
    ballStates = [];
    visualSwallows.clear();
    lastIntakeTime = 0;
    lastShootTime = 0;
    robotStorage.set(0);
    ballsInPlay.set(ballsData.length);
    lastReportedInPlay = ballsData.length;
    
    if (ballsData.length === 0) return;

    const rigidBodyDesc = rapier.RigidBodyDesc.dynamic()
      .setLinearDamping(0.8)
      .setAngularDamping(4.0)
      .setCcdEnabled(!potatoMode);
      
    const colliderDesc = rapier.ColliderDesc.ball(0.05)
      .setRestitution(BALL_RESTITUTION)
      .setFriction(BALL_FRICTION)
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
      const rightX = -rState.forward.z;
      const rightZ = rState.forward.x;
      let nearestBallIndex = -1;
      let nearestBallScore = Number.POSITIVE_INFINITY;

      for (let i = 0; i < rapierBodies.length; i++) {
        if (ballStates[i] === 'active') {
          const body = rapierBodies[i];
          const bPos = body.translation();
          const dx = bPos.x - rState.pos.x;
          const dz = bPos.z - rState.pos.z;
          const dy = Math.abs(bPos.y - rState.pos.y);
          const forwardDistance =
            dx * rState.forward.x + dz * rState.forward.z;
          const lateralDistance = Math.abs(dx * rightX + dz * rightZ);

          const insideIntakeFunnel =
            dy < INTAKE_MAX_HEIGHT &&
            forwardDistance > INTAKE_FORWARD_MIN &&
            forwardDistance < INTAKE_FORWARD_MAX &&
            lateralDistance < INTAKE_HALF_WIDTH;

          if (!insideIntakeFunnel) continue;

          // Hold the ball at the roller while the intake-rate cooldown runs.
          const mouthX = rState.pos.x + rState.forward.x * 0.3;
          const mouthZ = rState.pos.z + rState.forward.z * 0.3;
          const pullX = mouthX - bPos.x;
          const pullZ = mouthZ - bPos.z;
          const pullLength = Math.hypot(pullX, pullZ);

          if (pullLength > 0.001) {
            const velocity = body.linvel();
            const blend = Math.min(1, delta * 12);
            const targetVx =
              rState.vel.x + (pullX / pullLength) * INTAKE_PULL_SPEED;
            const targetVz =
              rState.vel.z + (pullZ / pullLength) * INTAKE_PULL_SPEED;
            body.setLinvel(
              new rapier.Vector3(
                velocity.x + (targetVx - velocity.x) * blend,
                velocity.y,
                velocity.z + (targetVz - velocity.z) * blend
              ),
              true
            );
          }

          const score =
            Math.abs(forwardDistance - 0.3) + lateralDistance * 1.5;
          if (score < nearestBallScore) {
            nearestBallScore = score;
            nearestBallIndex = i;
          }
        }
      }

      if (nearestBallIndex >= 0 && lastIntakeTime >= intakeInterval) {
        const body = rapierBodies[nearestBallIndex];
        const bPos = body.translation();

        ballStates[nearestBallIndex] = 'swallowing';
        visualSwallows.set(nearestBallIndex, {
          x: bPos.x,
          y: bPos.y,
          z: bPos.z
        });

        // Remove it from physics while the swallowing animation completes.
        body.setTranslation(new rapier.Vector3(0, -100, 0), true);
        body.setLinvel(new rapier.Vector3(0, 0, 0), true);
        body.setAngvel(new rapier.Vector3(0, 0, 0), true);

        storage++;
        storageChanged = true;
        lastIntakeTime = 0;
      }
    } else {
      lastIntakeTime = 0;
    }
    
    // --- SWALLOWING LOGIC (Visual Only!) ---
    for (let i = 0; i < rapierBodies.length; i++) {
      if (ballStates[i] === 'swallowing') {
        const vState = visualSwallows.get(i);
        if (!vState) continue;
        
        // Target the top center of the robot
        const targetX = rState.pos.x;
        const targetY = rState.pos.y + 0.3; // Up over the bumper
        const targetZ = rState.pos.z;
        
        const dx = targetX - vState.x;
        const dy = targetY - vState.y;
        const dz = targetZ - vState.z;
        const distSq = dx*dx + dy*dy + dz*dz;
        
        if (distSq < 0.01) {
          // It has reached the center of the hopper. Fully store it!
          ballStates[i] = 'stored';
          visualSwallows.delete(i);
        } else {
          // Smoothly interpolate visual position towards the hopper
          vState.x += dx * 0.2;
          vState.y += dy * 0.2;
          vState.z += dz * 0.2;
        }
      }
    }

    // --- OUTTAKE LOGIC ---
    if (rState.isShootActive && storage > 0) {
      lastShootTime += delta;
      const shootInterval = 1.0 / specs.outtakeRate;
      
      if (lastShootTime >= shootInterval) {
        // Shoot a wide burst of 3-4 balls!
        const burstCount = Math.min(storage, 3 + Math.floor(Math.random() * 2));
        
        let ballsToShoot = [];
        for (let i = 0; i < rapierBodies.length; i++) {
          if (ballStates[i] === 'stored') {
            ballsToShoot.push(i);
            if (ballsToShoot.length === burstCount) break;
          }
        }
        
        for (let j = 0; j < ballsToShoot.length; j++) {
          const i = ballsToShoot[j];
          ballStates[i] = 'active';
          
          const rightX = rState.forward.z;
          const rightZ = -rState.forward.x;
          
          let offsetMag = 0;
          if (ballsToShoot.length > 1) {
            // Span across +/- 0.18 meters (0.36m total width, fits inside the 0.5m robot)
            const maxSpread = 0.18;
            offsetMag = -maxSpread + (j / (ballsToShoot.length - 1)) * (maxSpread * 2);
            // Add a little randomness so they aren't perfectly spaced
            offsetMag += (Math.random() - 0.5) * 0.05;
          }
          
          // Time-stagger simulation: 
          // By moving the ball slightly forward/backward along the robot's forward vector,
          // it completely breaks the "perfect mathematical line" and makes them look like
          // they were fired a few milliseconds apart.
          const timeStagger = (Math.random() - 0.5) * 0.35; // +/- 17.5 cm stagger
          
          // Outtake zone is at the back of the robot, plus the lateral width offset and time stagger
          const outX = rState.pos.x - rState.forward.x * 0.4 + rightX * offsetMag + rState.forward.x * timeStagger;
          const outY = rState.pos.y + 0.3 + (Math.random() * 0.05); // tiny vertical jitter
          const outZ = rState.pos.z - rState.forward.z * 0.4 + rightZ * offsetMag + rState.forward.z * timeStagger;
          
          rapierBodies[i].setTranslation(new rapier.Vector3(outX, outY, outZ), true);
          
          // Calculate velocity trajectory with entropy (randomness)
          const verticalVariance = (Math.random() - 0.5) * 4.0;
          const angleRad = (specs.outtakeAngle + verticalVariance) * (Math.PI / 180);
          
          const speedMultiplier = 1.0 + (Math.random() - 0.5) * 0.1;
          const finalSpeed = specs.outtakeVelocity * speedMultiplier;
          
          const vY = Math.sin(angleRad) * finalSpeed;
          const vHoriz = Math.cos(angleRad) * finalSpeed;
          
          const spread = (Math.random() - 0.5) * 0.1;
          const dirX = -rState.forward.x;
          const dirZ = -rState.forward.z;
          const spreadX = dirX * Math.cos(spread) - dirZ * Math.sin(spread);
          const spreadZ = dirX * Math.sin(spread) + dirZ * Math.cos(spread);
          
          const vX = spreadX * vHoriz + rState.vel.x;
          const vZ = spreadZ * vHoriz + rState.vel.z;
          const finalVy = vY + rState.vel.y;
          
          // Massive backspin
          const backspin = 40.0 + (Math.random() - 0.5) * 10.0;
          const spinX = rightX * backspin + (Math.random() - 0.5) * 5.0;
          const spinY = (Math.random() - 0.5) * 5.0;
          const spinZ = rightZ * backspin + (Math.random() - 0.5) * 5.0;
          
          rapierBodies[i].setAngvel(new rapier.Vector3(spinX, spinY, spinZ), true);
          rapierBodies[i].setLinvel(new rapier.Vector3(vX, finalVy, vZ), true);
          
          storage--;
          storageChanged = true;
        }
        lastShootTime = 0;
      }
    } else {
      lastShootTime = 0;
    }

    if (storageChanged) {
      robotStorage.set(storage);
    }
    
    let currentInPlay = 0;
    
    // --- MATRIX SYNC ---
    for (let i = 0; i < rapierBodies.length; i++) {
      const body = rapierBodies[i];
      let pos = body.translation();
      const rot = body.rotation();
      
      if (ballStates[i] === 'active') {
        if (Math.abs(pos.x) <= 3.5 && Math.abs(pos.z) <= 3.5) {
          currentInPlay++;
        }
      } else if (ballStates[i] === 'swallowing' && visualSwallows.has(i)) {
        const vState = visualSwallows.get(i);
        pos = { x: vState.x, y: vState.y, z: vState.z };
      }
      
      dummyObj.position.set(pos.x, pos.y, pos.z);
      dummyObj.quaternion.set(rot.x, rot.y, rot.z, rot.w);
      dummyObj.updateMatrix();
      
      instancedMeshRef.setMatrixAt(i, dummyObj.matrix);
    }
    instancedMeshRef.instanceMatrix.needsUpdate = true;
    
    if (currentInPlay !== lastReportedInPlay) {
      ballsInPlay.set(currentInPlay);
      lastReportedInPlay = currentInPlay;
    }
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
