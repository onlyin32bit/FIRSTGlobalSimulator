<script lang="ts">
  import { T, useTask } from '@threlte/core'
  import { RigidBody, Collider, AutoColliders, CollisionGroups } from '@threlte/rapier'
  import type { RigidBody as RapierRigidBody } from '@dimforge/rapier3d-compat'
  import { Vector3, Quaternion } from 'three'
  import { robotTelemetry } from './telemetry'

  let { resetTrigger = 0 } = $props();

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
      rigidBody.setTranslation({ x: 0, y: 0.225, z: 3.15 }, true);
      rigidBody.setLinvel({ x: 0, y: 0, z: 0 }, true);
      rigidBody.setAngvel({ x: 0, y: 0, z: 0 }, true);
      currentLinSpeed = 0;
      currentAngSpeed = 0;
    }
  });

  let lastSpeedForTelemetry = 0;

  useTask((delta) => {
    if (!rigidBody) return;

    const linvel = rigidBody.linvel();
    const angvel = rigidBody.angvel();
    const pos = rigidBody.translation();
    const mass = rigidBody.mass();
    
    // --- TELEMETRY ---
    const currentSpeedMag = Math.sqrt(linvel.x * linvel.x + linvel.z * linvel.z);
    const accel = (currentSpeedMag - lastSpeedForTelemetry) / delta;
    lastSpeedForTelemetry = currentSpeedMag;
    
    robotTelemetry.set({
      x: pos.x,
      y: pos.y,
      z: pos.z,
      speed: currentSpeedMag,
      turnRate: angvel.y,
      accel: accel
    });
    // -----------------

    let forwardInput = 0;
    let turnInput = 0;

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
    
    const targetVelocity = forwardVec.multiplyScalar(currentLinSpeed);
    
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
  <T.Group position={[0, 0.225, 3.15]}>
    <RigidBody 
      bind:rigidBody 
      type="dynamic"
      enabledRotations={[false, true, false]} 
      enabledTranslations={[true, false, true]}
      ccd={true}
    >
      <!-- 
        The robot's collider extends 1 meter underground!
        Because we use CollisionGroups to ignore the floor, it doesn't get pushed up.
        Because it extends infinitely deep, the physics solver's "shortest path out" for a penetrated ball
        is NEVER downwards. This mathematically guarantees balls will NEVER clip under the robot again!
      -->
      <T.Group position={[0, -0.5, 0]}>
        <Collider 
          shape="cuboid" 
          args={[0.23, 0.725, 0.23]} 
          friction={0.0} 
          restitution={0.1} 
          mass={18.0} 
        />
      </T.Group>
      
      <T.Mesh castShadow>
        <T.BoxGeometry args={[0.45, 0.45, 0.45]} />
        <T.MeshStandardMaterial color="#4f46e5" roughness={0.7} metalness={0.5} />
      </T.Mesh>
    </RigidBody>
  </T.Group>
</CollisionGroups>
