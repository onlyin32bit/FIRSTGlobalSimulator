<script lang="ts">
  import { useTask, T } from '@threlte/core';
  import { useRapier } from '@threlte/rapier';
  import { Object3D } from 'three';
  import { onMount, onDestroy } from 'svelte';
  import { robotPhysicsState, robotSpecs, robotStorage, ballsInPlay } from './stores';
  import { get } from 'svelte/store';
  import { addScore } from '$lib/scoreStore';
  import type { ZoneAABB } from '$lib/scoreStore';
  
  let {
    potatoMode = false,
    resetTrigger = 0,
    ballsData = [],
    scoringZones = [] as ZoneAABB[],
  } = $props();
  
  const { world, rapier } = useRapier();
  
  let instancedMeshRef: any = $state();
  const dummyObj = new Object3D();
  let rapierBodies: any[] = [];
  let ballStates: string[] = [];
  // Tracks which scoring zone each ball was inside last frame (by zone id or '').
  // Used for enter-edge detection: score fires only on the transition from outside → inside.
  let ballZoneState: string[] = [];
  // The previous physics position is needed because a fast ball can cross a
  // thin scoring zone between two rendered frames.
  let previousBallPositions: Array<{ x: number; y: number; z: number }> = [];
  let visualSwallows = new Map();
  let lastReportedInPlay = 500;
  
  let lastIntakeTime = 0;
  let lastShootTime = 0;
  let lastTransferTime = 0;

  // Lightweight PU foam: some rebound, but far less than a rubber ball.
  const BALL_RESTITUTION = 0.4;
  const BALL_FRICTION = 0.75;
  const INTAKE_FORWARD_MIN = 0.08;
  const INTAKE_FORWARD_MAX = 0.72;
  const INTAKE_HALF_WIDTH = 0.34;
  const INTAKE_MAX_HEIGHT = 0.5;
  const INTAKE_PULL_SPEED = 1.5;

  function hasClearIntakePath(
    mouth: { x: number; y: number; z: number },
    ball: { x: number; y: number; z: number }
  ) {
    const dx = ball.x - mouth.x;
    const dy = ball.y - mouth.y;
    const dz = ball.z - mouth.z;
    const distance = Math.hypot(dx, dy, dz);
    if (distance < 0.001) return true;

    const ray = new rapier.Ray(
      new rapier.Vector3(mouth.x, mouth.y, mouth.z),
      new rapier.Vector3(dx / distance, dy / distance, dz / distance)
    );

    // Dynamic bodies are the robot and balls. Ignore them so only fixed field
    // geometry can block the intake path.
    const hit = world.castRay(
      ray,
      distance,
      true,
      undefined,
      undefined,
      undefined,
      undefined,
      (collider) => {
        const parent = collider.parent();
        return !parent || !parent.isDynamic();
      }
    );

    return hit === null;
  }
  
  function initPhysics() {
    // Clear existing
    rapierBodies.forEach(b => world.removeRigidBody(b));
    rapierBodies = [];
    ballStates = [];
    ballZoneState = [];
    previousBallPositions = [];
    visualSwallows.clear();
    lastIntakeTime = 0;
    lastShootTime = 0;
    lastTransferTime = 0;
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
      ballZoneState.push(''); // not inside any scoring zone yet
      previousBallPositions.push({ x: ball.x, y: ball.y, z: ball.z });
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

          const mouth = {
            x: rState.pos.x + rState.forward.x * 0.3,
            y: rState.pos.y,
            z: rState.pos.z + rState.forward.z * 0.3
          };
          if (!hasClearIntakePath(mouth, bPos)) continue;

          // Hold the ball at the roller while the intake-rate cooldown runs.
          const pullX = mouth.x - bPos.x;
          const pullZ = mouth.z - bPos.z;
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
          // Do not sweep from the ball's old stored position when it is shot.
          previousBallPositions[i] = { x: outX, y: outY, z: outZ };
          ballZoneState[i] = '';
          
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

    // --- TRANSFER LOGIC ---
    // Transfers 3-4 balls at a time out of the FRONT (intake side) of the robot
    if (rState.isTransferActive && storage > 0) {
      lastTransferTime += delta;
      const transferInterval = 1.0 / (specs.transferRate || 2.5);
      
      if (lastTransferTime >= transferInterval) {
        // Transfer a cluster of 3-4 balls at a time to simulate physics unexpectedness!
        const burstMin = Math.max(1, Math.floor(specs.transferBurstMin ?? 3));
        const burstMax = Math.max(burstMin, Math.floor(specs.transferBurstMax ?? 4));
        const burstCount = Math.min(
          storage,
          burstMin + Math.floor(Math.random() * (burstMax - burstMin + 1))
        );
        
        let ballsToTransfer: number[] = [];
        for (let i = 0; i < rapierBodies.length; i++) {
          if (ballStates[i] === 'stored') {
            ballsToTransfer.push(i);
            if (ballsToTransfer.length === burstCount) break;
          }
        }
        
        for (let j = 0; j < ballsToTransfer.length; j++) {
          const i = ballsToTransfer[j];
          ballStates[i] = 'active';
          
          const rightX = rState.forward.z;
          const rightZ = -rState.forward.x;
          
          let offsetMag = 0;
          if (ballsToTransfer.length > 1) {
            // Span across +/- 0.16 meters across front intake width
            const maxSpread = 0.16;
            offsetMag = -maxSpread + (j / (ballsToTransfer.length - 1)) * (maxSpread * 2);
            offsetMag += (Math.random() - 0.5) * 0.04;
          }
          
          // Time-stagger along forward direction to create realistic physical cluster
          const timeStagger = (Math.random() - 0.5) * 0.25;
          
          // Transfer origin is at the FRONT of the robot (+0.35m forward), elevated by transferHeight
          const outX = rState.pos.x + rState.forward.x * 0.35 + rightX * offsetMag + rState.forward.x * timeStagger;
          const outY = rState.pos.y + (specs.transferHeight ?? 0.20) + (Math.random() * 0.04);
          const outZ = rState.pos.z + rState.forward.z * 0.35 + rightZ * offsetMag + rState.forward.z * timeStagger;
          
          rapierBodies[i].setTranslation(new rapier.Vector3(outX, outY, outZ), true);
          previousBallPositions[i] = { x: outX, y: outY, z: outZ };
          ballZoneState[i] = '';
          
          // Calculate trajectory in FORWARD direction (+rState.forward)
          const verticalVariance = (Math.random() - 0.5) * 5.0;
          const angleRad = ((specs.transferAngle ?? 20) + verticalVariance) * (Math.PI / 180);
          
          const speedMultiplier = 1.0 + (Math.random() - 0.5) * 0.12;
          const finalSpeed = (specs.transferVelocity ?? 5.0) * speedMultiplier;
          
          const vY = Math.sin(angleRad) * finalSpeed;
          const vHoriz = Math.cos(angleRad) * finalSpeed;
          
          const spread = (Math.random() - 0.5) * 0.12;
          const dirX = rState.forward.x;
          const dirZ = rState.forward.z;
          const spreadX = dirX * Math.cos(spread) - dirZ * Math.sin(spread);
          const spreadZ = dirX * Math.sin(spread) + dirZ * Math.cos(spread);
          
          const vX = spreadX * vHoriz + rState.vel.x;
          const vZ = spreadZ * vHoriz + rState.vel.z;
          const finalVy = vY + rState.vel.y;
          
          // Topspin / forward spin when ejected out the front
          const spinMag = 30.0 + (Math.random() - 0.5) * 10.0;
          const spinX = -rightX * spinMag + (Math.random() - 0.5) * 4.0;
          const spinY = (Math.random() - 0.5) * 4.0;
          const spinZ = -rightZ * spinMag + (Math.random() - 0.5) * 4.0;
          
          rapierBodies[i].setAngvel(new rapier.Vector3(spinX, spinY, spinZ), true);
          rapierBodies[i].setLinvel(new rapier.Vector3(vX, finalVy, vZ), true);
          
          storage--;
          storageChanged = true;
        }
        lastTransferTime = 0;
      }
    } else {
      lastTransferTime = 0;
    }

    if (storageChanged) {
      robotStorage.set(storage);
    }

    // --- ZONE SCORING DETECTION ---
    // Only run when scoring zones have been loaded from the field semantics.
    // For each active ball, test both its current position and the segment
    // travelled since the previous frame. The zones are thin, so checking
    // only the current centre under-counts fast balls that tunnel through.
    if (scoringZones.length > 0) {
      for (let i = 0; i < rapierBodies.length; i++) {
        const pos = rapierBodies[i].translation();
        if (ballStates[i] !== 'active') {
          previousBallPositions[i] = { x: pos.x, y: pos.y, z: pos.z };
          ballZoneState[i] = '';
          continue;
        }

        const previous = previousBallPositions[i] ?? { x: pos.x, y: pos.y, z: pos.z };
        let hitZone = '';

        for (const zone of scoringZones) {
          const insideNow =
            pos.x >= zone.min[0] && pos.x <= zone.max[0] &&
            pos.y >= zone.min[1] && pos.y <= zone.max[1] &&
            pos.z >= zone.min[2] && pos.z <= zone.max[2];

          let entry = 0;
          let exit = 1;
          const starts = [previous.x, previous.y, previous.z];
          const deltas = [pos.x - previous.x, pos.y - previous.y, pos.z - previous.z];
          let crossedZone = true;

          for (let axis = 0; axis < 3; axis++) {
            const min = zone.min[axis];
            const max = zone.max[axis];
            const start = starts[axis];
            const delta = deltas[axis];
            if (Math.abs(delta) < 1e-8) {
              if (start < min || start > max) crossedZone = false;
              continue;
            }
            let near = (min - start) / delta;
            let far = (max - start) / delta;
            if (near > far) [near, far] = [far, near];
            entry = Math.max(entry, near);
            exit = Math.min(exit, far);
            if (entry > exit) crossedZone = false;
          }

          if (insideNow) hitZone = zone.id;
          if ((insideNow || crossedZone) && ballZoneState[i] !== zone.id) {
            addScore(zone.id);

            // Balls are consumed when they enter either Fire Shield.
            if (zone.id === 'blueFSscore' || zone.id === 'redFSscore') {
              const removedPosition = { x: 0, y: -100, z: 0 };
              ballStates[i] = 'scored';
              rapierBodies[i].setTranslation(
                new rapier.Vector3(removedPosition.x, removedPosition.y, removedPosition.z),
                true
              );
              rapierBodies[i].setLinvel(new rapier.Vector3(0, 0, 0), true);
              rapierBodies[i].setAngvel(new rapier.Vector3(0, 0, 0), true);
              previousBallPositions[i] = removedPosition;
              ballZoneState[i] = '';
              break;
            }
          }
        }

        ballZoneState[i] = hitZone;
        previousBallPositions[i] = { x: pos.x, y: pos.y, z: pos.z };
      }
    } else {
      // Keep the baseline current while field semantics are still loading.
      // Otherwise the first scoring frame could sweep from the spawn point.
      for (let i = 0; i < rapierBodies.length; i++) {
        const pos = rapierBodies[i].translation();
        previousBallPositions[i] = { x: pos.x, y: pos.y, z: pos.z };
        ballZoneState[i] = '';
      }
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
