<script lang="ts">
  import { T, useTask } from '@threlte/core'
  import { RigidBody, Collider, AutoColliders } from '@threlte/rapier'
  import type { RigidBody as RapierRigidBody } from '@dimforge/rapier3d-compat'
  import { Vector3, Quaternion } from 'three'
  import { HTML } from '@threlte/extras'
  import { robotTelemetry } from './telemetry'
  import { robotPhysicsState, robotStorage } from './stores'

  let { resetTrigger = 0, spawnPos = [0, 0.225, 3.15] } = $props();

  let rigidBody: RapierRigidBody | undefined = $state();
  
  // Lowered max speed to prevent physical tunneling (ghosting). 
  // 4.0 m/s was moving faster per-frame than the ball's radius!
  const MAX_SPEED = 2.5; 
  const MAX_TURN = 8.0; 
  const ACCEL = 15.0; 
  const TURN_ACCEL = 40.0; 
  
  let currentLinSpeed = 0;
  let currentAngSpeed = 0;

  $effect(() => {
    if (resetTrigger > 0 && rigidBody) {
      rigidBody.setTranslation({ x: spawnPos[0], y: spawnPos[1], z: spawnPos[2] }, true);
      rigidBody.setLinvel({ x: 0, y: 0, z: 0 }, true);
      rigidBody.setAngvel({ x: 0, y: 0, z: 0 }, true);
      currentLinSpeed = 0;
      currentAngSpeed = 0;
    }
  });

  let lastSpeedForTelemetry = 0;
  let smoothedFps = 60;
  let timeSinceLastTelemetry = 0;
  
  let rollerRotation = $state(0);

  useTask((delta) => {
    if (!rigidBody) return;

    const linvel = rigidBody.linvel();
    const angvel = rigidBody.angvel();
    const pos = rigidBody.translation();
    const mass = rigidBody.mass();
    
    // --- TELEMETRY ---
    const currentFps = delta > 0 ? 1 / delta : 0;
    smoothedFps = smoothedFps * 0.95 + currentFps * 0.05;

    const currentSpeedMag = Math.sqrt(linvel.x * linvel.x + linvel.z * linvel.z);
    const accel = (currentSpeedMag - lastSpeedForTelemetry) / delta;
    lastSpeedForTelemetry = currentSpeedMag;
    
    // --- TELEMETRY THROTTLING ---
    // Updating Svelte DOM text nodes 60 times a second blocks the main thread with Layout/Paint overhead!
    // We throttle the UI updates to 10Hz to free up the WebGL and Physics loops.
    timeSinceLastTelemetry += delta;
    if (timeSinceLastTelemetry > 0.1) {
      robotTelemetry.set({
        x: pos.x,
        y: pos.y,
        z: pos.z,
        speed: currentSpeedMag,
        turnRate: angvel.y,
        accel: accel,
        fps: smoothedFps
      });
      timeSinceLastTelemetry = 0;
    }
    // -----------------

    let forwardInput = 0;
    let turnInput = 0;
    let intakeBtn = false;
    let shootBtn = false;

    const gamepads = navigator.getGamepads ? navigator.getGamepads() : [];
    let gp: Gamepad | null = null;
    for (let i = 0; i < gamepads.length; i++) {
      if (gamepads[i] && gamepads[i]!.connected) {
        gp = gamepads[i];
        break;
      }
    }

    if (gp) {
      const rawForward = -gp.axes[1]; 
      const rawTurn = -gp.axes[2];    
      
      forwardInput = Math.abs(rawForward) > 0.1 ? rawForward : 0;
      turnInput = Math.abs(rawTurn) > 0.1 ? rawTurn : 0;
      
      intakeBtn = gp.buttons[5]?.pressed || gp.buttons[7]?.pressed || false;
      shootBtn = gp.buttons[4]?.pressed || gp.buttons[6]?.pressed || false;
    }

    let leftPower = forwardInput - turnInput;
    let rightPower = forwardInput + turnInput;
    
    const maxPower = Math.max(Math.abs(leftPower), Math.abs(rightPower));
    if (maxPower > 1.0) {
      leftPower /= maxPower;
      rightPower /= maxPower;
    }

    const driveSpeed = (leftPower + rightPower) / 2;
    const turnSpeed = (rightPower - leftPower) / 2;

    const targetSpeed = driveSpeed * MAX_SPEED;
    const targetTurn = turnSpeed * MAX_TURN;

    if (currentLinSpeed < targetSpeed) {
      currentLinSpeed = Math.min(targetSpeed, currentLinSpeed + ACCEL * delta);
    } else if (currentLinSpeed > targetSpeed) {
      currentLinSpeed = Math.max(targetSpeed, currentLinSpeed - ACCEL * delta);
    }

    if (currentAngSpeed < targetTurn) {
      currentAngSpeed = Math.min(targetTurn, currentAngSpeed + TURN_ACCEL * delta);
    } else if (currentAngSpeed > targetTurn) {
      currentAngSpeed = Math.max(targetTurn, currentAngSpeed - TURN_ACCEL * delta);
    }

    const rot = rigidBody.rotation();
    const quat = new Quaternion(rot.x, rot.y, rot.z, rot.w);
    const forwardVec = new Vector3(0, 0, -1).applyQuaternion(quat);
    
    // Sync robot state for the Intake/Outtake system
    robotPhysicsState.set({
      pos: { x: pos.x, y: pos.y, z: pos.z },
      vel: { x: linvel.x, y: linvel.y, z: linvel.z },
      forward: { x: forwardVec.x, y: forwardVec.y, z: forwardVec.z },
      isIntakeActive: intakeBtn,
      isShootActive: shootBtn
    });

    const targetVelocity = forwardVec.multiplyScalar(currentLinSpeed);
    
    if (intakeBtn) {
      rollerRotation -= delta * 30; // Spin rapidly inwards
    }
    
    const deltaVx = targetVelocity.x - linvel.x;
    const deltaVz = targetVelocity.z - linvel.z;
    
    const linearResponse = 0.5;
    let impulseX = deltaVx * mass * linearResponse;
    let impulseZ = deltaVz * mass * linearResponse;
    
    // CLAMP THE MAX IMPULSE!
    // This is the true fix for the no-clip:
    // Without clamping, the motor asks for infinite force to maintain speed when hitting balls,
    // crushing them through the geometry in a single frame.
    // By clamping the impulse (force), the robot actually slows down realistically when ramming a pile,
    // giving the physics engine time to safely roll the balls away.
    const maxLinearImpulse = 4.0; // Max pushing power (tuneable)
    const impulseMag = Math.sqrt(impulseX * impulseX + impulseZ * impulseZ);
    if (impulseMag > maxLinearImpulse) {
      impulseX = (impulseX / impulseMag) * maxLinearImpulse;
      impulseZ = (impulseZ / impulseMag) * maxLinearImpulse;
    }
    
    rigidBody.applyImpulse({
      x: impulseX,
      y: 0, 
      z: impulseZ
    }, true);

    const deltaW = currentAngSpeed - angvel.y;
    const inertia = (mass / 12.0) * (0.45 * 0.45 + 0.45 * 0.45);
    const angularResponse = 0.5;
    
    let torqueImpulse = deltaW * inertia * angularResponse;
    
    // Clamp turning torque as well
    const maxTorqueImpulse = 1.0;
    if (Math.abs(torqueImpulse) > maxTorqueImpulse) {
      torqueImpulse = Math.sign(torqueImpulse) * maxTorqueImpulse;
    }
    
    rigidBody.applyTorqueImpulse({
      x: 0,
      y: torqueImpulse,
      z: 0
    }, true);
  });
</script>

<CollisionGroups groups={[1, 2]}>
  <T.Group position={[spawnPos[0], spawnPos[1], spawnPos[2]]}>
    <RigidBody 
      bind:rigidBody 
      type="dynamic"
      enabledRotations={[false, true, false]} 
      enabledTranslations={[true, true, true]}
      ccd={true}
    >
      <!-- 
        The "Deep Pillar" Collider Strategy:
        Instead of a tiny cube that can tunnel through balls at high speeds, 
        we make the robot's physical collider extend 50 meters underground!
        This completely eliminates physics tunneling issues on the X/Z plane.
        We also expand the bounds by 2.5cm to prevent clipping.
      -->
      <T.Group position={[0, -49.7, 0]}>
        <Collider 
          shape="cuboid" 
          args={[0.275, 50, 0.325]} 
          friction={0.0} 
          restitution={0.1} 
          mass={18.0} 
        />
      </T.Group>
      
      <T.Mesh castShadow receiveShadow position={[0, 0.15, 0]}>
      <T.BoxGeometry args={[0.5, 0.3, 0.6]} />
      <T.MeshStandardMaterial color="#2563eb" roughness={0.4} metalness={0.2} />
    </T.Mesh>
    
    <!-- Active Intake Mechanism (Compliant Roller) -->
    <T.Group position={[0, 0.05, -0.35]} rotation={[rollerRotation, 0, 0]}>
      <!-- The main roller axle -->
      <T.Mesh rotation={[0, 0, Math.PI / 2]} castShadow>
        <T.CylinderGeometry args={[0.04, 0.04, 0.48, 16]} />
        <T.MeshStandardMaterial color="#222222" roughness={0.9} />
      </T.Mesh>
      
      <!-- Green compliant wheels on the axle -->
      {#each [-0.15, 0, 0.15] as xOffset}
        <T.Mesh position={[xOffset, 0, 0]} rotation={[0, 0, Math.PI / 2]} castShadow>
          <T.CylinderGeometry args={[0.07, 0.07, 0.05, 16]} />
          <T.MeshStandardMaterial color="#22c55e" roughness={0.8} />
        </T.Mesh>
      {/each}
    </T.Group>
    
    <!-- Floating Storage Indicator -->
    <HTML position={[0, 0.7, 0]} center>
      <div class="bg-black/40 text-white font-mono font-bold text-lg px-2 py-0.5 pointer-events-none select-none">
        {$robotStorage}
      </div>
    </HTML>
    </RigidBody>
</T.Group>
